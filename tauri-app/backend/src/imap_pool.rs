//! IMAP connection pool with bounded timeouts and stale-connection recovery.
//!
//! Why this exists: every IMAP-backed action used to open a fresh TCP socket,
//! perform a full TLS handshake, and run `LOGIN` before doing any real work.
//! Against Gmail that round-trip dominates the latency of every interactive
//! action. This module keeps authenticated sessions warm and hands them back
//! out, so the handshake+login cost is paid once instead of per action. It also
//! adds the connect/read/write timeouts that the old path lacked, so a hung
//! server can no longer block a worker thread forever.
//!
//! Scope rule (see memory `feedback_imap_pool_active_account_only`): the pool
//! only ever holds connections for the **currently active account**. When the
//! account identity changes, the previous account's sessions are logged out and
//! dropped — a credential-bound session is never reused for a different user.
//!
//! Platform note: the pooled session is boxed as `Session<Box<dyn ReadWrite>>`,
//! which both the desktop `imap` 2.4.1 and the Android `imap` 3.0-alpha only
//! require to be `Read + Write`. The IDLE watcher (see `imap_idle`) needs a
//! `SetReadTimeout`-capable concrete type instead, so it builds its own
//! connection rather than borrowing from this pool.

use anyhow::{anyhow, Result};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::types::EmailConfig;

#[cfg(not(target_os = "android"))]
use native_tls::TlsConnector;
#[cfg(target_os = "android")]
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore};
#[cfg(target_os = "android")]
use std::sync::Arc;

/// How long to wait for the initial TCP connect before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Read/write timeout applied to every pooled socket. Bounds any single
/// blocking IMAP operation so a stalled server can't hang a worker thread.
const IO_TIMEOUT: Duration = Duration::from_secs(30);
/// A reused connection younger than this is handed out without a NOOP probe —
/// it was almost certainly alive moments ago, so skip the extra round-trip.
const VALIDATE_AFTER: Duration = Duration::from_secs(30);
/// A reused connection older than this is dropped outright (servers commonly
/// close idle IMAP connections after a few minutes; don't even probe).
const MAX_IDLE: Duration = Duration::from_secs(300);
/// Cap on warm connections kept per active account.
const MAX_IDLE_CONNS: usize = 3;

/// Any stream that can back a pooled IMAP session. The blanket impl means
/// `TcpStream`, `native_tls::TlsStream<_>`, and `rustls::StreamOwned<_>` all
/// qualify, so a single boxed type covers every transport on every platform.
pub trait ReadWrite: std::io::Read + std::io::Write + Send {}
impl<T: std::io::Read + std::io::Write + Send> ReadWrite for T {}

/// The single concrete session type stored in the pool.
pub type ImapSession = imap::Session<Box<dyn ReadWrite>>;

/// Identity of an IMAP account: anything that differs here is a different
/// connection. `email` is lowercased so case variations don't fragment the key.
type AccountKey = (String, String, u16, bool);

/// Connection parameters for a single IMAP account. Construct from an
/// `EmailConfig`, or inline at call sites that only have the cloned parts.
#[derive(Clone)]
pub struct ImapTarget {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
}

impl ImapTarget {
    pub fn from_config(c: &EmailConfig) -> Self {
        ImapTarget {
            host: c.imap_host.clone(),
            port: c.imap_port,
            username: c.email_address.clone(),
            password: c.password.clone(),
            use_tls: c.use_tls,
        }
    }

    fn key(&self) -> AccountKey {
        (
            self.username.trim().to_lowercase(),
            self.host.clone(),
            self.port,
            self.use_tls,
        )
    }

    fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

struct PooledConn {
    session: ImapSession,
    last_used: Instant,
}

#[derive(Default)]
struct PoolState {
    /// The account the pooled connections belong to. `None` until first use.
    owner: Option<AccountKey>,
    idle: Vec<PooledConn>,
}

fn pool() -> &'static Mutex<PoolState> {
    static POOL: OnceLock<Mutex<PoolState>> = OnceLock::new();
    POOL.get_or_init(|| Mutex::new(PoolState::default()))
}

/// Open a TCP stream with a bounded connect timeout, then apply read/write
/// timeouts. Tries every resolved address so dual-stack hosts still work.
fn connect_tcp(addr: &str) -> Result<TcpStream> {
    let socket_addrs: Vec<_> = addr
        .to_socket_addrs()
        .map_err(|e| anyhow!("failed to resolve {}: {}", addr, e))?
        .collect();
    if socket_addrs.is_empty() {
        return Err(anyhow!("no addresses resolved for {}", addr));
    }
    let mut last_err = None;
    for sa in socket_addrs {
        match TcpStream::connect_timeout(&sa, CONNECT_TIMEOUT) {
            Ok(tcp) => {
                tcp.set_read_timeout(Some(IO_TIMEOUT))?;
                tcp.set_write_timeout(Some(IO_TIMEOUT))?;
                return Ok(tcp);
            }
            Err(e) => last_err = Some(e),
        }
    }
    Err(anyhow!(
        "could not connect to {}: {}",
        addr,
        last_err.map(|e| e.to_string()).unwrap_or_default()
    ))
}

/// Establish a fresh, authenticated session for `target`. Replaces the old
/// `create_imap_tls_client!` macro + duplicated plain-TCP branches, adding
/// timeouts and boxing the stream into the single pooled session type.
pub fn connect_imap(target: &ImapTarget) -> Result<ImapSession> {
    let addr = target.addr();
    let stream: Box<dyn ReadWrite> = if target.use_tls {
        build_tls_stream(&target.host, &addr)?
    } else {
        Box::new(connect_tcp(&addr)?)
    };
    let client = imap::Client::new(stream);
    client
        .login(&target.username, &target.password)
        .map_err(|e| anyhow!(e.0))
}

#[cfg(not(target_os = "android"))]
fn build_tls_stream(host: &str, addr: &str) -> Result<Box<dyn ReadWrite>> {
    let tls = TlsConnector::builder().build()?;
    let tcp = connect_tcp(addr)?;
    let tls_stream = tls.connect(host, tcp)?;
    Ok(Box::new(tls_stream))
}

#[cfg(target_os = "android")]
fn build_tls_stream(host: &str, addr: &str) -> Result<Box<dyn ReadWrite>> {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });

    let mut root_store = RootCertStore::empty();
    root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth();
    let server_name =
        ServerName::try_from(host.to_string()).map_err(|_| anyhow!("invalid server name"))?;
    let tcp = connect_tcp(addr)?;
    let conn = ClientConnection::new(Arc::new(config), server_name)?;
    let tls_stream = rustls::StreamOwned::new(conn, tcp);
    Ok(Box::new(tls_stream))
}

/// Take a ready-to-use session for `target`: reuse a warm one when possible,
/// otherwise connect fresh. Switching accounts (owner mismatch) discards the
/// previous account's pooled sessions first. Stale reused connections are
/// validated with NOOP (or dropped when too old) so callers always get a live
/// session or an error from the fresh-connect attempt.
pub fn checkout(target: &ImapTarget) -> Result<ImapSession> {
    let key = target.key();

    let reused = {
        let mut guard = pool().lock().unwrap();
        if guard.owner.as_ref() != Some(&key) {
            // Account changed (or first use): tear down the old account's
            // connections and take ownership for this one.
            take_idle(&mut guard)
                .into_iter()
                .for_each(|mut c| drop(c.session.logout()));
            guard.owner = Some(key);
        }
        guard.idle.pop()
    };

    if let Some(mut conn) = reused {
        let age = conn.last_used.elapsed();
        if age <= MAX_IDLE {
            if age <= VALIDATE_AFTER || conn.session.noop().is_ok() {
                return Ok(conn.session);
            }
        }
        // Too old or failed validation: discard and fall through to reconnect.
        let _ = conn.session.logout();
    }

    connect_imap(target)
}

/// Return a session to the pool for reuse. Sessions whose account no longer
/// owns the pool, or that overflow the per-account cap, are logged out instead.
pub fn checkin(target: &ImapTarget, mut session: ImapSession) {
    let key = target.key();
    let mut guard = pool().lock().unwrap();
    if guard.owner.as_ref() != Some(&key) || guard.idle.len() >= MAX_IDLE_CONNS {
        drop(guard);
        let _ = session.logout();
        return;
    }
    guard.idle.push(PooledConn {
        session,
        last_used: Instant::now(),
    });
}

/// Run `f` against a pooled session for the duration of one operation.
///
/// On success the session is returned to the pool; on error it is dropped
/// (logged out) rather than re-pooled. Staleness is handled at checkout via the
/// NOOP probe, so a reused connection is already validated before `f` runs; a
/// failure inside `f` is therefore treated as a real error and not retried,
/// which avoids re-running side-effecting operations (move/delete) twice.
pub fn with_session<T>(
    target: &ImapTarget,
    f: impl FnOnce(&mut ImapSession) -> Result<T>,
) -> Result<T> {
    let mut session = checkout(target)?;
    match f(&mut session) {
        Ok(v) => {
            checkin(target, session);
            Ok(v)
        }
        Err(e) => {
            let _ = session.logout();
            Err(e)
        }
    }
}

/// Drop and log out every pooled connection and forget the owner. Called on
/// logout and account switch so no stale or foreign connection lingers.
pub fn clear_all() {
    let drained = {
        let mut guard = pool().lock().unwrap();
        guard.owner = None;
        take_idle(&mut guard)
    };
    // Log out outside the lock — these may block on dead sockets.
    for mut conn in drained {
        let _ = conn.session.logout();
    }
}

fn take_idle(state: &mut PoolState) -> Vec<PooledConn> {
    std::mem::take(&mut state.idle)
}
