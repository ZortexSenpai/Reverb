//! Password storage: OS keychain via `keyring` (Credential Manager /
//! macOS Keychain / Secret Service), with a plaintext file fallback for
//! Linux setups without a secret service. The fallback is flagged so the
//! UI can warn about it.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::error::ReverbError;

const SERVICE: &str = "dev.zortexsenpai.reverb";

pub struct Keychain {
    fallback_path: PathBuf,
    /// Fallback for `store_secret`. Deliberately a *different* file from
    /// `fallback_path`: that one holds a single account/password pair, so
    /// writing a second credential into it would silently evict the server
    /// password.
    secrets_path: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct FallbackFile {
    account: String,
    password: String,
}

impl Keychain {
    pub fn new(config_dir: PathBuf) -> Self {
        Self {
            fallback_path: config_dir.join("credentials.json"),
            secrets_path: config_dir.join("secrets.json"),
        }
    }

    fn account(server_url: &str, username: &str) -> String {
        format!("{username}@{server_url}")
    }

    pub fn store(&self, server_url: &str, username: &str, password: &str) -> Result<(), ReverbError> {
        let account = Self::account(server_url, username);
        match keyring::Entry::new(SERVICE, &account)
            .and_then(|entry| entry.set_password(password))
        {
            Ok(()) => {
                // Success in the real keychain: remove any stale fallback.
                let _ = fs::remove_file(&self.fallback_path);
                Ok(())
            }
            Err(e) => {
                log::warn!("keychain unavailable ({e}); storing credentials in a plain file");
                let raw = serde_json::to_string(&FallbackFile {
                    account,
                    password: password.to_owned(),
                })
                .map_err(|e| ReverbError::Io(e.to_string()))?;
                fs::write(&self.fallback_path, raw)?;
                Ok(())
            }
        }
    }

    pub fn retrieve(&self, server_url: &str, username: &str) -> Result<String, ReverbError> {
        let account = Self::account(server_url, username);
        if let Ok(password) =
            keyring::Entry::new(SERVICE, &account).and_then(|entry| entry.get_password())
        {
            return Ok(password);
        }
        let raw = fs::read_to_string(&self.fallback_path)
            .map_err(|_| ReverbError::Keyring("no stored credentials".into()))?;
        let fallback: FallbackFile =
            serde_json::from_str(&raw).map_err(|e| ReverbError::Keyring(e.to_string()))?;
        if fallback.account == account {
            Ok(fallback.password)
        } else {
            Err(ReverbError::Keyring("stored credentials are for a different account".into()))
        }
    }

    pub fn clear(&self, server_url: &str, username: &str) {
        let account = Self::account(server_url, username);
        if let Ok(entry) = keyring::Entry::new(SERVICE, &account) {
            let _ = entry.delete_credential();
        }
        let _ = fs::remove_file(&self.fallback_path);
    }

    /// True when the password lives in the plaintext fallback file.
    pub fn using_fallback(&self) -> bool {
        self.fallback_path.exists()
    }

    fn read_secrets(&self) -> BTreeMap<String, String> {
        fs::read_to_string(&self.secrets_path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn write_secrets(&self, secrets: &BTreeMap<String, String>) -> Result<(), ReverbError> {
        if secrets.is_empty() {
            let _ = fs::remove_file(&self.secrets_path);
            return Ok(());
        }
        let raw =
            serde_json::to_string(secrets).map_err(|e| ReverbError::Io(e.to_string()))?;
        fs::write(&self.secrets_path, raw)?;
        Ok(())
    }

    /// Store a named secret that is not the server password — currently the
    /// Last.fm shared secret and session key. Keyed by name rather than by
    /// account, since there is exactly one of each.
    pub fn store_secret(&self, name: &str, value: &str) -> Result<(), ReverbError> {
        match keyring::Entry::new(SERVICE, name).and_then(|entry| entry.set_password(value)) {
            Ok(()) => {
                // Landed in the real keychain: drop any stale fallback copy.
                let mut secrets = self.read_secrets();
                if secrets.remove(name).is_some() {
                    self.write_secrets(&secrets)?;
                }
                Ok(())
            }
            Err(e) => {
                log::warn!("keychain unavailable ({e}); storing {name} in a plain file");
                let mut secrets = self.read_secrets();
                secrets.insert(name.to_owned(), value.to_owned());
                self.write_secrets(&secrets)
            }
        }
    }

    pub fn retrieve_secret(&self, name: &str) -> Option<String> {
        if let Ok(value) =
            keyring::Entry::new(SERVICE, name).and_then(|entry| entry.get_password())
        {
            return Some(value);
        }
        self.read_secrets().remove(name)
    }

    pub fn clear_secret(&self, name: &str) {
        if let Ok(entry) = keyring::Entry::new(SERVICE, name) {
            let _ = entry.delete_credential();
        }
        let mut secrets = self.read_secrets();
        if secrets.remove(name).is_some() {
            let _ = self.write_secrets(&secrets);
        }
    }
}
