//! Background IMAP IDLE watcher.
//!
//! Why this exists: the inbox previously only refreshed on a manual action.
//! This watcher holds a dedicated IMAP connection in IDLE on the active
//! account's INBOX and, when the server reports a change, emits an
//! `imap-new-mail` event. The frontend reacts by running its normal sync, so
//! new mail shows up within seconds without polling.
//!
//! Scope: exactly one watcher runs at a time, for the active account only (see
//! memory `feedback_imap_pool_active_account_only`). Switching accounts stops
//! the old watcher and starts a new one.
//!
//! Connection ownership: IDLE monopolizes a connection for minutes at a time,
//! so the watcher uses its **own** dedicated connection rather than borrowing
//! from `imap_pool`. It also needs a `SetReadTimeout`-capable concrete session
//! type (the pool's boxed `dyn ReadWrite` can't satisfy that bound), which is
//! why the connection is built here with platform-specific code:
//!   - desktop (`imap` 2.4.1): native-tls `TlsStream<TcpStream>`
//!   - Android (`imap` 3.0-alpha): `ClientBuilder` → `Session<Connection>`
//! IDLE requires TLS here; on a non-TLS server the watcher is a no-op and the
//! app falls back to manual refresh.

use anyhow::{anyhow, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;

use crate::imap_pool::ImapTarget;
use crate::types::EmailConfig;

/// Connect timeout for the dedicated IDLE socket. Desktop only — the Android
/// path builds its connection through `imap::ClientBuilder`.
#[cfg(not(target_os = "android"))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// How long each IDLE cycle waits before re-issuing IDLE. Kept well under the
/// RFC 2177 29-minute limit so the server doesn't drop us, and short enough
/// that a stop request (logout / account switch) is observed promptly — the
/// watcher only checks the stop flag at cycle boundaries.
const IDLE_WAIT: Duration = Duration::from_secs(120);
/// Backoff between reconnect attempts after a connection error.
const RECONNECT_BACKOFF: Duration = Duration::from_secs(5);
/// Event emitted to the frontend when the watched mailbox changes.
const NEW_MAIL_EVENT: &str = "imap-new-mail";

#[cfg(not(target_os = "android"))]
type IdleSession = imap::Session<native_tls::TlsStream<std::net::TcpStream>>;
#[cfg(target_os = "android")]
type IdleSession = imap::Session<imap::Connection>;

/// Live handle to a running watcher. Dropping it does not stop the thread; call
/// [`IdleHandle::stop`] (the lib.rs commands always do).
#[derive(Debug)]
pub struct IdleHandle {
    pub account_email: String,
    stop: Arc<AtomicBool>,
}

impl IdleHandle {
    /// Signal the watcher thread to exit. It will observe this at the next IDLE
    /// cycle boundary (within `IDLE_WAIT`) and log out cleanly.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Start watching `config`'s INBOX. Returns a handle the caller stores so the
/// watcher can later be stopped. Caller is responsible for stopping any prior
/// watcher first (account switch).
pub fn start(app: tauri::AppHandle, config: &EmailConfig) -> IdleHandle {
    let target = ImapTarget::from_config(config);
    let account_email = config.email_address.clone();
    let stop = Arc::new(AtomicBool::new(false));

    let thread_stop = stop.clone();
    let thread_account = account_email.clone();
    std::thread::spawn(move || run(app, target, thread_account, thread_stop));

    IdleHandle { account_email, stop }
}

/// Outer reconnect loop: keep a watch connection alive until asked to stop.
fn run(app: tauri::AppHandle, target: ImapTarget, account_email: String, stop: Arc<AtomicBool>) {
    if !target.use_tls {
        crate::debug_log!(
            "[RUST] imap_idle: non-TLS server, IDLE push disabled for {}",
            account_email
        );
        return;
    }

    while !stop.load(Ordering::SeqCst) {
        match watch_connection(&app, &target, &account_email, &stop) {
            Ok(()) => break, // clean exit: stop was requested
            Err(e) => {
                crate::debug_log!(
                    "[RUST] imap_idle: connection error for {}: {} (reconnecting)",
                    account_email,
                    e
                );
            }
        }
        // Backoff before reconnect, waking early if a stop is requested.
        let mut waited = Duration::ZERO;
        while waited < RECONNECT_BACKOFF {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
            waited += Duration::from_millis(250);
        }
    }
    crate::debug_log!("[RUST] imap_idle: watcher stopped for {}", account_email);
}

/// Establish one connection, then IDLE in a loop emitting events on change.
/// Returns Ok only when `stop` was observed; any I/O failure is an Err so the
/// outer loop reconnects.
fn watch_connection(
    app: &tauri::AppHandle,
    target: &ImapTarget,
    account_email: &str,
    stop: &Arc<AtomicBool>,
) -> Result<()> {
    let mut session = connect_idle_session(target)?;
    session
        .select("INBOX")
        .map_err(|e| anyhow!("select INBOX failed: {}", e))?;
    crate::debug_log!("[RUST] imap_idle: watching INBOX for {}", account_email);

    while !stop.load(Ordering::SeqCst) {
        let changed = idle_once(&mut session, IDLE_WAIT).map_err(|e| anyhow!("idle failed: {}", e))?;
        if stop.load(Ordering::SeqCst) {
            break;
        }
        if changed {
            crate::debug_log!(
                "[RUST] imap_idle: mailbox changed for {}, notifying frontend",
                account_email
            );
            let _ = app.emit(NEW_MAIL_EVENT, account_email.to_string());
        }
    }

    let _ = session.logout();
    Ok(())
}

// --- Platform-specific connection + IDLE cycle ----------------------------

#[cfg(not(target_os = "android"))]
fn connect_idle_session(target: &ImapTarget) -> Result<IdleSession> {
    use std::net::{TcpStream, ToSocketAddrs};

    let addr = format!("{}:{}", target.host, target.port);
    let sock = addr
        .to_socket_addrs()
        .map_err(|e| anyhow!("resolve {} failed: {}", addr, e))?
        .next()
        .ok_or_else(|| anyhow!("no address for {}", addr))?;
    let tcp = TcpStream::connect_timeout(&sock, CONNECT_TIMEOUT)?;
    // Read timeout is managed per-IDLE by the idle handle; keep a write timeout
    // so a stalled send can't hang the watcher thread forever.
    tcp.set_write_timeout(Some(Duration::from_secs(30)))?;

    let tls = native_tls::TlsConnector::builder().build()?;
    let tls_stream = tls.connect(&target.host, tcp)?;
    let client = imap::Client::new(tls_stream);
    client
        .login(&target.username, &target.password)
        .map_err(|e| anyhow!(e.0))
}

#[cfg(target_os = "android")]
fn connect_idle_session(target: &ImapTarget) -> Result<IdleSession> {
    // imap 3.0's ClientBuilder yields a boxed `Connection` that implements
    // SetReadTimeout (required for timed IDLE waits). With the crate's
    // `rustls-tls` feature this uses rustls under the hood.
    let client = imap::ClientBuilder::new(target.host.clone(), target.port)
        .connect()
        .map_err(|e| anyhow!("connect failed: {}", e))?;
    client
        .login(&target.username, &target.password)
        .map_err(|e| anyhow!(e.0))
}

/// Run a single IDLE cycle. Returns Ok(true) if the mailbox changed, Ok(false)
/// if the wait timed out (re-IDLE keepalive). The IDLE handle API differs
/// between the desktop (2.4.1) and Android (3.0-alpha) imap crates.
#[cfg(not(target_os = "android"))]
fn idle_once(session: &mut IdleSession, wait: Duration) -> imap::error::Result<bool> {
    use imap::extensions::idle::WaitOutcome;
    let handle = session.idle()?;
    match handle.wait_with_timeout(wait)? {
        WaitOutcome::MailboxChanged => Ok(true),
        WaitOutcome::TimedOut => Ok(false),
    }
}

#[cfg(target_os = "android")]
fn idle_once(session: &mut IdleSession, wait: Duration) -> imap::error::Result<bool> {
    use imap::extensions::idle::WaitOutcome;
    let mut handle = session.idle();
    handle.timeout(wait);
    handle.keepalive(false);
    // Stop on the first unsolicited response (a mailbox change).
    match handle.wait_while(|_| false)? {
        WaitOutcome::MailboxChanged => Ok(true),
        WaitOutcome::TimedOut => Ok(false),
    }
}
