use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;

#[cfg(not(target_os = "android"))]
const SERVICE_NAME: &str = "nostr-mail";
#[cfg(not(target_os = "android"))]
const VAULT_ACCOUNT: &str = "vault";

/// All private keys stored as a single JSON blob in one keychain entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Vault {
    /// pubkey -> private key
    keys: HashMap<String, String>,
}

/// Manages private keys in the OS keychain with an in-memory cache.
/// The cache ensures only one keychain access prompt per app launch.
///
/// On Android the `keyring` crate has no working backend, so the vault is
/// persisted as JSON in the app-private files directory instead. The Android
/// per-app sandbox keeps it inaccessible to other apps without root.
#[derive(Debug)]
pub struct KeychainManager {
    cache: Mutex<Option<Vault>>,
}

impl Clone for KeychainManager {
    fn clone(&self) -> Self {
        let cached = self.cache.lock().unwrap().clone();
        KeychainManager {
            cache: Mutex::new(cached),
        }
    }
}

impl KeychainManager {
    pub fn new() -> Self {
        KeychainManager {
            cache: Mutex::new(None),
        }
    }

    #[cfg(not(target_os = "android"))]
    fn entry() -> keyring::Result<keyring::Entry> {
        keyring::Entry::new(SERVICE_NAME, VAULT_ACCOUNT)
    }

    /// Read the vault from the OS keychain (bypassing cache).
    #[cfg(not(target_os = "android"))]
    fn read_vault_from_keychain() -> Result<Vault, String> {
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.get_password() {
            Ok(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse vault: {}", e)),
            Err(keyring::Error::NoEntry) => Ok(Vault::default()),
            Err(e) => Err(format!("Keychain read error: {}", e)),
        }
    }

    /// Read the vault on Android via JNI into VaultStorage (Jetpack
    /// Security EncryptedFile, master key in Android Keystore).
    #[cfg(target_os = "android")]
    fn read_vault_from_keychain() -> Result<Vault, String> {
        match android::vault_read()? {
            Some(json) => serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse encrypted vault: {}", e)),
            None => Ok(Vault::default()),
        }
    }

    /// Get the vault, reading from keychain only on first access.
    fn load_vault(&self) -> Result<Vault, String> {
        let mut cache = self.cache.lock().unwrap();
        if let Some(ref vault) = *cache {
            return Ok(vault.clone());
        }
        let vault = Self::read_vault_from_keychain()?;
        *cache = Some(vault.clone());
        Ok(vault)
    }

    /// Write the vault to the OS keychain and update the cache.
    #[cfg(not(target_os = "android"))]
    fn save_vault(&self, vault: &Vault) -> Result<(), String> {
        let json = serde_json::to_string(vault)
            .map_err(|e| format!("Failed to serialize vault: {}", e))?;
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("Keychain store error: {}", e))?;
        *self.cache.lock().unwrap() = Some(vault.clone());
        Ok(())
    }

    /// Write the vault on Android via JNI into VaultStorage.
    #[cfg(target_os = "android")]
    fn save_vault(&self, vault: &Vault) -> Result<(), String> {
        let json = serde_json::to_string(vault)
            .map_err(|e| format!("Failed to serialize vault: {}", e))?;
        android::vault_write(&json)?;
        *self.cache.lock().unwrap() = Some(vault.clone());
        Ok(())
    }

    pub fn store_key(&self, public_key: &str, private_key: &str) -> Result<(), String> {
        let mut vault = self.load_vault()?;
        vault.keys.insert(public_key.to_string(), private_key.to_string());
        self.save_vault(&vault)
    }

    pub fn get_key(&self, public_key: &str) -> Result<Option<String>, String> {
        let vault = self.load_vault()?;
        Ok(vault.keys.get(public_key).cloned())
    }

    pub fn delete_key(&self, public_key: &str) -> Result<(), String> {
        let mut vault = self.load_vault()?;
        vault.keys.remove(public_key);
        self.save_vault(&vault)
    }

    pub fn list_pubkeys(&self) -> Result<Vec<String>, String> {
        let vault = self.load_vault()?;
        Ok(vault.keys.keys().cloned().collect())
    }

    #[cfg(not(target_os = "android"))]
    pub fn clear_all(&self) -> Result<(), String> {
        let entry = Self::entry().map_err(|e| format!("Keychain entry error: {}", e))?;
        match entry.delete_credential() {
            Ok(()) => {
                *self.cache.lock().unwrap() = Some(Vault::default());
                Ok(())
            },
            Err(keyring::Error::NoEntry) => {
                *self.cache.lock().unwrap() = Some(Vault::default());
                Ok(())
            },
            Err(e) => Err(format!("Keychain delete error: {}", e)),
        }
    }

    #[cfg(target_os = "android")]
    pub fn clear_all(&self) -> Result<(), String> {
        android::vault_clear()?;
        *self.cache.lock().unwrap() = Some(Vault::default());
        Ok(())
    }
}

/// JNI bridge to `com.nostr.mail.VaultStorage`. The Kotlin singleton holds
/// the application context; MainActivity.onCreate calls `VaultStorage.init`
/// before any Rust command can reach this code.
///
/// We dispatch onto Tauri's main Android thread via `tauri::wry::prelude::dispatch`,
/// which provides a JNIEnv plus the activity reference — and use `find_class`
/// (which calls the Activity's `getAppClass`) to resolve `VaultStorage` through
/// the app classloader. (Tauri 2.10+ dropped the `ndk_context` populated by tao
/// 0.34; this is the new path.)
#[cfg(target_os = "android")]
mod android {
    use jni::objects::{JClass, JObject, JString, JValue};
    use jni::JNIEnv;
    use std::sync::mpsc;

    /// If a Java exception is pending on `env`, dump its stack trace to logcat
    /// and clear it so subsequent JNI calls don't immediately fail.
    fn drain_exception(env: &mut JNIEnv) {
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
        }
    }

    /// Hop onto Tauri's main Android thread, look up `VaultStorage`, run `f`,
    /// and ferry the result back via a channel so the caller stays sync.
    fn with_env<F, R>(label: &'static str, f: F) -> Result<R, String>
    where
        F: for<'a> FnOnce(&mut JNIEnv<'a>, JClass<'a>) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<Result<R, String>>();
        tauri::wry::prelude::dispatch(move |env, activity, _webview| {
            let result = (|| -> Result<R, String> {
                let class = tauri::wry::prelude::find_class(
                    env,
                    activity,
                    "com.nostr.mail.VaultStorage".to_string(),
                )
                .map_err(|e| {
                    drain_exception(env);
                    format!("[{}] find_class(VaultStorage) failed: {}", label, e)
                })?;
                f(env, class)
            })();
            let _ = tx.send(result);
        });
        rx.recv()
            .map_err(|e| format!("[{}] dispatch channel closed: {}", label, e))?
    }

    pub fn vault_read() -> Result<Option<String>, String> {
        println!("[RUST] VaultStorage.read JNI call");
        with_env("vault_read", |env, class| {
            let result = env
                .call_static_method(class, "read", "()Ljava/lang/String;", &[])
                .map_err(|e| { drain_exception(env); format!("VaultStorage.read threw: {}", e) })?;
            let obj: JObject = result
                .l()
                .map_err(|e| format!("VaultStorage.read returned non-object: {}", e))?;
            if obj.is_null() {
                println!("[RUST] VaultStorage.read returned null (no vault yet)");
                return Ok(None);
            }
            let jstr: JString = obj.into();
            let s: String = env
                .get_string(&jstr)
                .map_err(|e| format!("Failed to read JString: {}", e))?
                .into();
            println!("[RUST] VaultStorage.read returned {} bytes", s.len());
            Ok(Some(s))
        })
    }

    pub fn vault_write(json: &str) -> Result<(), String> {
        println!("[RUST] VaultStorage.write JNI call ({} bytes)", json.len());
        let owned = json.to_owned();
        with_env("vault_write", move |env, class| {
            let arg = env
                .new_string(&owned)
                .map_err(|e| format!("Failed to allocate JString: {}", e))?;
            env.call_static_method(
                class,
                "write",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&arg)],
            )
            .map_err(|e| { drain_exception(env); format!("VaultStorage.write threw: {}", e) })?;
            println!("[RUST] VaultStorage.write completed");
            Ok(())
        })
    }

    pub fn vault_clear() -> Result<(), String> {
        println!("[RUST] VaultStorage.clear JNI call");
        with_env("vault_clear", |env, class| {
            env.call_static_method(class, "clear", "()V", &[])
                .map_err(|e| { drain_exception(env); format!("VaultStorage.clear threw: {}", e) })?;
            Ok(())
        })
    }
}
