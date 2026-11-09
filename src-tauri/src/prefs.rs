//! Per-user preferences and provider credentials.
//!
//! The repository only ships `.env.example`; real keys belong to whoever runs
//! the application. The desktop shell therefore keeps them in
//! `app_config_dir/settings.json` instead of the source tree, and values written
//! through the settings screen win over whatever `.env` contains. That keeps the
//! developer workflow intact while letting every user reconfigure a packaged
//! build without editing files by hand.
//!
//! Writes are atomic (temporary file plus rename) and, on Unix, the file is kept
//! readable by its owner only. Moving the secrets to the OS keychain stays on the
//! roadmap; this module is the single place that would change.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use cyberclergy_llm::{Pace, ProviderId, ProviderSpec, PROVIDER_SPECS};
use serde::{Deserialize, Serialize};

pub const SETTINGS_FILE: &str = "settings.json";

/// Every interface language the bundled locales cover. The list mirrors
/// `src/i18n/index.ts`; a mismatch would only cost a fallback to English.
pub const SUPPORTED_LANGUAGES: &[&str] = &["ru", "en", "uk", "he", "de", "es"];
pub const SUPPORTED_THEMES: &[&str] = &["night", "dawn"];
pub const DEFAULT_LANGUAGE: &str = "ru";
pub const DEFAULT_THEME: &str = "night";
pub const DEFAULT_RELIGION: &str = "christianity";
/// Highest number of robots the cloud holds: four tiers of three, matching the
/// `MAX_SLOTS` ceiling of the scheduler and the stepped platform in the scene.
pub const MAX_ROBOTS: usize = 12;

/// Settings the user can change from the interface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub language: String,
    pub theme: String,
    /// Religion used by the chapel. Validated against the profile catalog.
    pub religion: String,
    /// Networks standing on the cloud, front slot first. `None` means the person
    /// has never chosen one: every configured network then stands up.
    pub roster: Option<Vec<ProviderId>>,
    /// How often the cloud prays: `calm`, `normal` or `lively`.
    pub pace: String,
    /// Draft-era cleric count. Read so an old settings file still parses, never
    /// written back: the roster supersedes it.
    #[serde(rename = "clerics", default, skip_serializing)]
    legacy_clerics: Option<usize>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: DEFAULT_LANGUAGE.to_string(),
            theme: DEFAULT_THEME.to_string(),
            religion: DEFAULT_RELIGION.to_string(),
            roster: None,
            pace: Pace::default().as_str().to_string(),
            legacy_clerics: None,
        }
    }
}

impl Preferences {
    /// The stored pace, coerced to the default when the file was edited by hand.
    pub fn pace(&self) -> Pace {
        Pace::parse(&self.pace).unwrap_or_default()
    }

    /// Networks actually standing on the cloud, slot by slot.
    ///
    /// A stored roster keeps its order and loses only networks whose
    /// credentials are gone; `None` means the person never picked, so every
    /// configured network stands up. That keeps the chapel lit after an upgrade
    /// from the draft-era numeric count.
    pub fn cloud(&self, configured: &[ProviderId]) -> Vec<ProviderId> {
        match &self.roster {
            Some(roster) => roster
                .iter()
                .filter(|id| configured.contains(id))
                .copied()
                .collect(),
            None => configured.iter().copied().take(MAX_ROBOTS).collect(),
        }
    }

    /// Coerces out-of-range values instead of rejecting them: a typo in a
    /// hand-edited file must never keep the chapel from opening.
    pub fn sanitized(mut self) -> Self {
        if !SUPPORTED_LANGUAGES.contains(&self.language.as_str()) {
            self.language = DEFAULT_LANGUAGE.to_string();
        }
        if !SUPPORTED_THEMES.contains(&self.theme.as_str()) {
            self.theme = DEFAULT_THEME.to_string();
        }
        let religion = self.religion.trim().to_ascii_lowercase();
        self.religion = if religion.is_empty() {
            DEFAULT_RELIGION.to_string()
        } else {
            religion
        };
        self.roster = self.roster.map(|roster| {
            let mut seen = std::collections::BTreeSet::new();
            roster
                .into_iter()
                .filter(|id| seen.insert(*id))
                .take(MAX_ROBOTS)
                .collect()
        });
        self.legacy_clerics = None;
        self.pace = self.pace().as_str().to_string();
        self
    }
}

/// On-disk shape of `settings.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredFile {
    preferences: Preferences,
    /// Environment-style variable names mapped to their values, for example
    /// `OPENROUTER_API_KEY`.
    keys: BTreeMap<String, String>,
}

/// Owner of `settings.json`.
#[derive(Debug, Clone)]
pub struct PrefsStore {
    path: PathBuf,
    file: StoredFile,
}

impl PrefsStore {
    /// Reads an existing file, falling back to defaults when it is missing or
    /// corrupted so that startup never depends on the file's integrity.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file = read_file(&path);
        Self { path, file }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn preferences(&self) -> Preferences {
        self.file.preferences.clone()
    }

    pub fn set_preferences(&mut self, preferences: Preferences) -> Result<Preferences, String> {
        self.file.preferences = preferences.sanitized();
        self.persist()?;
        Ok(self.preferences())
    }

    /// Stores one provider variable. Only names declared by a provider spec are
    /// accepted, so the interface cannot inject unrelated settings here.
    pub fn set_key(&mut self, variable: &str, value: &str) -> Result<(), String> {
        if !is_known_variable(variable) {
            return Err(format!("`{variable}` is not a provider setting"));
        }
        let value = value.trim();
        if value.is_empty() {
            self.file.keys.remove(variable);
        } else {
            self.file.keys.insert(variable.to_string(), value.to_string());
        }
        self.persist()
    }

    /// Credentials physically present in the file (the keychain backend keeps
    /// its own copy, see [`crate::secrets`]).
    pub fn keys(&self) -> BTreeMap<String, String> {
        self.file.keys.clone()
    }

    pub fn remove_key(&mut self, variable: &str) -> Result<(), String> {
        if self.file.keys.remove(variable).is_some() {
            self.persist()?;
        }
        Ok(())
    }

    /// Forgets everything the person configured for one provider: credentials,
    /// extra variables and the chosen model, so "Forget" really resets the row.
    pub fn clear_provider(&mut self, id: ProviderId) -> Result<(), String> {
        let spec = ProviderSpec::from_id(id);
        self.file.keys.remove(spec.api_key_var);
        self.file.keys.remove(spec.model_var);
        self.file.keys.remove(spec.base_url_var);
        for variable in spec.required_vars {
            self.file.keys.remove(*variable);
        }
        self.persist()
    }

    fn persist(&self) -> Result<(), String> {
        write_json_atomic(&self.path, &self.file)
    }
}

/// Writes pretty JSON through a temporary sibling file and renames it into
/// place. A crash mid-write therefore cannot truncate the real file; on Unix the
/// result is readable by its owner only.
pub fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "cannot create the directory {}: {error}",
                    parent.display()
                )
            })?;
        }
    }

    let json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("cannot encode {}: {error}", path.display()))?;

    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json)
        .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
    restrict_permissions(&temporary)?;
    fs::rename(&temporary, path)
        .map_err(|error| format!("cannot replace {}: {error}", path.display()))?;
    Ok(())
}


/// True for the API key, every extra variable a provider requires and the
/// model name, which the settings screen edits as its own field.
pub fn is_known_variable(variable: &str) -> bool {
    PROVIDER_SPECS.iter().any(|spec| {
        spec.api_key_var == variable
            || spec.model_var == variable
            || spec.required_vars.contains(&variable)
    })
}

fn read_file(path: &Path) -> StoredFile {
    let Ok(text) = fs::read_to_string(path) else {
        return StoredFile::default();
    };
    match serde_json::from_str::<StoredFile>(&text) {
        Ok(mut file) => {
            file.preferences = file.preferences.sanitized();
            file
        }
        Err(error) => {
            eprintln!(
                "settings file {} is unreadable ({error}); starting from defaults",
                path.display()
            );
            StoredFile::default()
        }
    }
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("cannot restrict permissions of {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp_path(test: &str) -> PathBuf {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("cyberclergy-prefs-{}-{sequence}", std::process::id()))
            .join(test)
            .join(SETTINGS_FILE)
    }

    #[test]
    fn unknown_values_fall_back_to_defaults() {
        let preferences = Preferences {
            language: "klingon".to_string(),
            theme: "neon".to_string(),
            religion: "  ".to_string(),
            roster: Some(vec![ProviderId::Groq, ProviderId::Groq, ProviderId::Gemini]),
            pace: "hectic".to_string(),
            legacy_clerics: Some(4),
        }
        .sanitized();

        assert_eq!(preferences.language, DEFAULT_LANGUAGE);
        assert_eq!(preferences.theme, DEFAULT_THEME);
        assert_eq!(preferences.religion, DEFAULT_RELIGION);
        // Duplicates collapse and the legacy count is dropped.
        assert_eq!(
            preferences.roster,
            Some(vec![ProviderId::Groq, ProviderId::Gemini])
        );
        assert_eq!(preferences.pace, "normal");
    }

    #[test]
    fn an_unset_roster_stands_up_every_configured_network() {
        let preferences = Preferences::default();
        let configured = [ProviderId::Groq, ProviderId::Mistral];
        assert_eq!(preferences.cloud(&configured), configured.to_vec());

        let chosen = Preferences {
            roster: Some(vec![ProviderId::Nvidia, ProviderId::Groq]),
            ..Preferences::default()
        };
        // The stored order wins and a network without credentials drops out.
        assert_eq!(chosen.cloud(&configured), vec![ProviderId::Groq]);
        // An emptied roster really means an empty cloud.
        let empty = Preferences {
            roster: Some(Vec::new()),
            ..Preferences::default()
        };
        assert!(empty.cloud(&configured).is_empty());
    }

    #[test]
    fn preferences_and_keys_survive_a_reload() {
        let path = temp_path("round-trip");
        let mut store = PrefsStore::open(&path);
        store
            .set_preferences(Preferences {
                language: "en".to_string(),
                theme: "dawn".to_string(),
                religion: "islam".to_string(),
                roster: Some(vec![ProviderId::Groq, ProviderId::Mistral]),
                pace: "Calm".to_string(),
                legacy_clerics: None,
            })
            .expect("preferences are writable");
        store
            .set_key("OPENROUTER_API_KEY", "  sk-test-value  ")
            .expect("the key is writable");

        let reloaded = PrefsStore::open(&path);
        assert_eq!(reloaded.preferences().religion, "islam");
        assert_eq!(reloaded.preferences().theme, "dawn");
        assert_eq!(
            reloaded.preferences().roster,
            Some(vec![ProviderId::Groq, ProviderId::Mistral])
        );
        // The pace is stored in its canonical spelling, whatever was typed.
        assert_eq!(reloaded.preferences().pace, "calm");
        assert_eq!(
            reloaded.keys().get("OPENROUTER_API_KEY"),
            Some(&"sk-test-value".to_string())
        );

        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn a_draft_era_file_keeps_its_keys() {
        let path = temp_path("legacy");
        let parent = path.parent().expect("the path has a parent");
        fs::create_dir_all(parent).expect("the directory is creatable");
        fs::write(
            &path,
            r#"{
  "preferences": {
    "language": "en",
    "theme": "dawn",
    "religion": "judaism",
    "clerics": 5,
    "pace": "lively"
  },
  "keys": { "GROQ_API_KEY": "gsk-legacy" }
}"#,
        )
        .expect("the fixture is writable");

        let store = PrefsStore::open(&path);
        let preferences = store.preferences();
        assert_eq!(preferences.language, "en");
        assert_eq!(preferences.religion, "judaism");
        // No roster yet: the chapel stands every configured network up.
        assert_eq!(preferences.roster, None);
        assert_eq!(
            store.keys().get("GROQ_API_KEY"),
            Some(&"gsk-legacy".to_string())
        );

        let _ = fs::remove_dir_all(parent);
    }

    #[test]
    fn unrelated_variables_are_rejected() {
        let mut store = PrefsStore::open(temp_path("rejected"));
        assert!(store.set_key("PATH", "/usr/bin").is_err());
        assert!(store.keys().is_empty());
    }

    #[test]
    fn a_model_override_is_a_known_setting() {
        let path = temp_path("model");
        let mut store = PrefsStore::open(&path);
        store
            .set_key("GROQ_MODEL", "llama-3.1-8b-instant")
            .expect("the model is writable");
        assert_eq!(
            store.keys().get("GROQ_MODEL"),
            Some(&"llama-3.1-8b-instant".to_string())
        );

        // Forgetting the provider also drops the chosen model.
        store
            .clear_provider(ProviderId::Groq)
            .expect("clearing is writable");
        assert!(store.keys().is_empty());
        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn clearing_a_provider_removes_every_variable() {
        let path = temp_path("clear");
        let mut store = PrefsStore::open(&path);
        store
            .set_key("CLOUDFLARE_API_TOKEN", "token")
            .expect("token is writable");
        store
            .set_key("CLOUDFLARE_ACCOUNT_ID", "account")
            .expect("account is writable");
        store
            .clear_provider(ProviderId::Cloudflare)
            .expect("clearing is writable");

        assert!(store.keys().is_empty());
        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }

    #[test]
    fn a_migrated_key_can_be_dropped_from_the_file() {
        let path = temp_path("migrate");
        let mut store = PrefsStore::open(&path);
        store
            .set_key("GROQ_API_KEY", "gsk-test")
            .expect("the key is written");
        store
            .remove_key("GROQ_API_KEY")
            .expect("the key is dropped");

        assert!(store.keys().is_empty());
        assert!(PrefsStore::open(&path).keys().is_empty());
        let _ = fs::remove_dir_all(path.parent().unwrap_or(&path));
    }
}
