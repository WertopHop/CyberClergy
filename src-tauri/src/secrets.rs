//! Provider credentials at rest.
//!
//! Two backends share one interface:
//!
//! * the **OS keychain** (Windows Credential Manager, macOS Keychain, Secret
//!   Service), used whenever it works — the service name is `CyberClergy` and
//!   the account is the variable name, for example `OPENROUTER_API_KEY`;
//! * **`settings.json`**, the fallback for environments without a keychain
//!   (headless sessions, locked wallets, a desktop the application cannot
//!   reach).
//!
//! The backend is chosen once at startup by writing, reading and deleting a
//! probe entry. Anything found in `settings.json` on the first keychain run is
//! copied into the keychain and removed from the file, so a secret never lives
//! in two places. A keychain failure at runtime silently degrades to the file
//! backend instead of blocking the settings screen.

use std::collections::BTreeMap;

use cyberclergy_llm::{provider_variables, ProviderId, PROVIDER_SPECS};
use keyring::{Entry, Error as KeyringError};

use crate::prefs::PrefsStore;

pub const SERVICE: &str = "CyberClergy";
const PROBE_ACCOUNT: &str = "__cyberclergy_probe__";

/// Where the credentials are actually kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Keychain,
    File,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::File => "file",
        }
    }
}

#[derive(Debug)]
pub struct SecretStore {
    prefs: PrefsStore,
    backend: Backend,
}

impl SecretStore {
    /// Opens the store, migrating file-stored keys into the keychain when it is
    /// available.
    pub fn open(mut prefs: PrefsStore) -> Self {
        let backend = if keychain_available() {
            migrate(&mut prefs);
            Backend::Keychain
        } else {
            Backend::File
        };
        Self { prefs, backend }
    }

    /// File-only store. Used by tests, which must never touch the real keychain.
    #[cfg(test)]
    pub fn file_only(prefs: PrefsStore) -> Self {
        Self {
            prefs,
            backend: Backend::File,
        }
    }

    pub fn backend(&self) -> Backend {
        self.backend
    }

    pub fn prefs(&self) -> &PrefsStore {
        &self.prefs
    }

    pub fn prefs_mut(&mut self) -> &mut PrefsStore {
        &mut self.prefs
    }

    /// Every known credential, merged over the process environment by the shell.
    /// Keychain values win over the file.
    pub fn keys(&self) -> BTreeMap<String, String> {
        let mut merged = self.prefs.keys();
        if self.backend == Backend::Keychain {
            for variable in known_variables() {
                let Ok(entry) = Entry::new(SERVICE, &variable) else {
                    continue;
                };
                if let Ok(value) = entry.get_password() {
                    if !value.trim().is_empty() {
                        merged.insert(variable, value);
                    }
                }
            }
        }
        merged
    }

    /// Stores or clears one variable. An empty value forgets the credential.
    pub fn set_key(&mut self, variable: &str, value: &str) -> Result<(), String> {
        let value = value.trim();
        if self.backend == Backend::Keychain {
            match write_keychain(variable, value) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    // Degrade rather than lose the value: the user asked to save
                    // it, so the file backend takes over from here.
                    eprintln!("keychain is unavailable ({error}); falling back to settings.json");
                    self.backend = Backend::File;
                }
            }
        }
        self.prefs.set_key(variable, value)
    }

    /// Forgets every setting of a provider from both backends, including the
    /// model override and the extra custom gateways, so "Forget" really resets
    /// the row.
    pub fn clear_provider(&mut self, id: ProviderId) -> Result<(), String> {
        let mut first_error = None;
        for variable in provider_variables(id) {
            if let Err(error) = delete_keychain(&variable) {
                first_error.get_or_insert(error);
            }
        }
        self.prefs.clear_provider(id)?;
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

/// Every variable name a provider may need, in a stable order. The model and
/// base-URL overrides live here too, otherwise a value written to the keychain
/// would never be read back into the settings snapshot.
fn known_variables() -> impl Iterator<Item = String> {
    PROVIDER_SPECS
        .iter()
        .flat_map(|spec| provider_variables(spec.id))
}

/// Writes, reads back and removes a throwaway entry. Only a full round trip
/// proves the keychain is usable, not merely present.
fn keychain_available() -> bool {
    let Ok(entry) = Entry::new(SERVICE, PROBE_ACCOUNT) else {
        return false;
    };
    if entry.set_password("probe").is_err() {
        return false;
    }
    let readable = entry.get_password().map(|value| value == "probe").unwrap_or(false);
    let _ = entry.delete_credential();
    readable
}

fn write_keychain(variable: &str, value: &str) -> Result<(), KeyringError> {
    let entry = Entry::new(SERVICE, variable)?;
    if value.is_empty() {
        return match entry.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(error),
        };
    }
    entry.set_password(value)
}

fn delete_keychain(variable: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE, variable).map_err(|error| error.to_string())?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

/// Moves file keys into the keychain, removing each one only after it has been
/// read back successfully.
fn migrate(prefs: &mut PrefsStore) {
    for (variable, value) in prefs.keys() {
        let Ok(entry) = Entry::new(SERVICE, &variable) else {
            continue;
        };
        let written = entry.set_password(&value).is_ok()
            && entry.get_password().map(|stored| stored == value).unwrap_or(false);
        if written {
            if let Err(error) = prefs.remove_key(&variable) {
                eprintln!("cannot drop the migrated key from settings.json: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_prefs(test: &str) -> PrefsStore {
        let path = std::env::temp_dir()
            .join(format!("cyberclergy-secrets-{}-{test}", std::process::id()))
            .join("settings.json");
        PrefsStore::open(path)
    }

    fn cleanup(store: &SecretStore) {
        let path: PathBuf = store.prefs().path().to_path_buf();
        if let Some(parent) = path.parent() {
            let _ = fs::remove_dir_all(parent);
        }
    }

    #[test]
    fn the_file_backend_round_trips_a_key() {
        let mut store = SecretStore::file_only(temp_prefs("round-trip"));
        assert_eq!(store.backend(), Backend::File);

        store
            .set_key("OPENROUTER_API_KEY", " sk-or-test ")
            .expect("the key is stored");
        assert_eq!(
            store.keys().get("OPENROUTER_API_KEY").map(String::as_str),
            Some("sk-or-test"),
            "values are trimmed before they are stored"
        );

        cleanup(&store);
    }

    #[test]
    fn an_empty_value_forgets_the_key() {
        let mut store = SecretStore::file_only(temp_prefs("forget"));
        store
            .set_key("GROQ_API_KEY", "gsk-test")
            .expect("the key is stored");
        store.set_key("GROQ_API_KEY", "   ").expect("the key is cleared");
        assert!(store.keys().is_empty());

        cleanup(&store);
    }

    #[test]
    fn clearing_a_provider_removes_all_of_its_variables() {
        let mut store = SecretStore::file_only(temp_prefs("clear"));
        store
            .set_key("CLOUDFLARE_API_TOKEN", "token")
            .expect("token is stored");
        store
            .set_key("CLOUDFLARE_ACCOUNT_ID", "account")
            .expect("account is stored");
        store
            .set_key("MISTRAL_API_KEY", "keep-me")
            .expect("other keys survive");

        store
            .clear_provider(ProviderId::Cloudflare)
            .expect("clearing succeeds");

        let keys = store.keys();
        assert_eq!(keys.len(), 1);
        assert!(keys.contains_key("MISTRAL_API_KEY"));

        cleanup(&store);
    }
}
