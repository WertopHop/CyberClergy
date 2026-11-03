//! Tauri entry point for CyberClergy.
//!
//! The shell owns the shared application state: preferences and credentials
//! persisted by [`prefs::PrefsStore`], the provider snapshot derived from them,
//! the prayer generator and the catalog of religion profiles. Profiles are
//! embedded into the binary, so packaged builds always carry their clergy
//! without shipping extra resource files.
//!
//! Network calls are kept out of the state lock: every command copies what it
//! needs (an `Arc` to the generator and an owned prayer spec) before awaiting,
//! which keeps the commands `Send` and the UI responsive.

mod prefs;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cyberclergy_llm::{
    ChatRequest, PrayerGenerator, ProfileCatalog, ProviderId, ProviderSpec, Registry,
    ReligionProfile, Settings, PROVIDER_SPECS,
};
use serde::Serialize;
use tauri::{Manager, State};

use prefs::{Preferences, PrefsStore, SETTINGS_FILE};

/// Shared state managed by Tauri and handed to commands.
pub struct AppState {
    prefs: PrefsStore,
    /// Snapshot rebuilt whenever a credential changes.
    settings: Settings,
    /// `None` when no provider is configured or the HTTP client could not be
    /// built; the UI still needs provider statuses in that case.
    generator: Option<Arc<PrayerGenerator>>,
    /// Empty only if the embedded profiles failed validation, which would be a
    /// build-time bug rather than a user error.
    profiles: ProfileCatalog,
}

impl AppState {
    fn new(prefs: PrefsStore) -> Self {
        let profiles = match ProfileCatalog::embedded() {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!("embedded religion profiles are invalid: {error}");
                ProfileCatalog::default()
            }
        };

        let mut state = Self {
            prefs,
            settings: Settings::from_vars(&BTreeMap::new()),
            generator: None,
            profiles,
        };
        state.rebuild_providers();
        state
    }

    /// Recomposes provider settings from `.env` plus the credentials stored by
    /// the user. Stored values win, so the settings screen always has the last
    /// word over whatever a developer left in `.env`.
    fn rebuild_providers(&mut self) {
        let mut vars: BTreeMap<String, String> = std::env::vars().collect();
        vars.extend(self.prefs.env_overrides());
        self.settings = Settings::from_vars(&vars);
        self.generator = Registry::new(&self.settings)
            .ok()
            .map(|registry| Arc::new(PrayerGenerator::new(registry)));
    }

    fn provider_views(&self) -> Vec<ProviderView> {
        PROVIDER_SPECS
            .iter()
            .map(|spec| {
                let config = self.settings.provider(spec.id);
                let mut fields = vec![ProviderField {
                    name: spec.api_key_var,
                    set: config.map(|config| config.api_key.is_some()).unwrap_or(false),
                }];
                for variable in spec.required_vars {
                    fields.push(ProviderField {
                        name: variable,
                        set: config
                            .map(|config| config.required_value(variable).is_some())
                            .unwrap_or(false),
                    });
                }
                ProviderView {
                    id: spec.id,
                    model: config
                        .map(|config| config.model.clone())
                        .unwrap_or_else(|| spec.default_model.to_string()),
                    configured: config.map(|config| config.is_configured()).unwrap_or(false),
                    fields,
                }
            })
            .collect()
    }

    /// Keeps preferences pointing at a religion the catalog actually knows.
    fn set_preferences(&mut self, preferences: Preferences) -> Result<Preferences, String> {
        let mut preferences = preferences.sanitized();
        if self.profiles.get(&preferences.religion).is_none() {
            if let Some(profile) = self.profiles.list().first() {
                preferences.religion = profile.id.clone();
            }
        }
        self.prefs.set_preferences(preferences)
    }
}

/// One credential field of a provider, as the settings screen sees it.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderField {
    pub name: &'static str,
    /// Whether the value is currently known (from `.env` or the stored file).
    pub set: bool,
}

/// Provider entry rendered by the settings screen. Only names are exposed;
/// secret values never travel back to the interface.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    pub id: ProviderId,
    pub model: String,
    pub configured: bool,
    pub fields: Vec<ProviderField>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PrayerOutcome {
    pub text: String,
    pub religion: String,
    pub provider: ProviderId,
    pub model: String,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckOutcome {
    pub ok: bool,
    pub detail: String,
    pub model: String,
    pub elapsed_ms: u64,
}

/// The prayer follows the interface language, so a Russian user gets Russian
/// prayers without touching the prompt.
fn prayer_language(preference: &str) -> &'static str {
    match preference {
        "ru" => "Russian",
        _ => "English",
    }
}

fn state_error() -> String {
    "application state is unavailable".to_string()
}

fn parse_provider(provider: &str) -> Result<ProviderId, String> {
    provider
        .parse::<ProviderId>()
        .map_err(|error: cyberclergy_llm::types::ParseProviderIdError| error.to_string())
}

#[tauri::command]
fn list_providers(state: State<'_, Mutex<AppState>>) -> Result<Vec<ProviderView>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.provider_views())
}

#[tauri::command]
fn list_religions(
    state: State<'_, Mutex<AppState>>,
) -> Result<Vec<ReligionProfile>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.profiles.list().into_iter().cloned().collect())
}

#[tauri::command]
fn load_preferences(state: State<'_, Mutex<AppState>>) -> Result<Preferences, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.prefs.preferences())
}

#[tauri::command]
fn save_preferences(
    state: State<'_, Mutex<AppState>>,
    preferences: Preferences,
) -> Result<Preferences, String> {
    let mut state = state.lock().map_err(|_| state_error())?;
    state.set_preferences(preferences)
}

#[tauri::command]
fn save_provider_field(
    state: State<'_, Mutex<AppState>>,
    provider: String,
    field: String,
    value: String,
) -> Result<Vec<ProviderView>, String> {
    let id = parse_provider(&provider)?;
    let spec = ProviderSpec::from_id(id);
    if field != spec.api_key_var && !spec.required_vars.contains(&field.as_str()) {
        return Err(format!("`{field}` does not belong to {id}"));
    }

    let mut state = state.lock().map_err(|_| state_error())?;
    state.prefs.set_key(&field, &value)?;
    state.rebuild_providers();
    Ok(state.provider_views())
}

#[tauri::command]
fn clear_provider(
    state: State<'_, Mutex<AppState>>,
    provider: String,
) -> Result<Vec<ProviderView>, String> {
    let id = parse_provider(&provider)?;
    let mut state = state.lock().map_err(|_| state_error())?;
    state.prefs.clear_provider(id)?;
    state.rebuild_providers();
    Ok(state.provider_views())
}

/// Sends a one-line request through a single provider and reports the latency,
/// which is all the settings screen needs to confirm a key works.
#[tauri::command]
async fn check_provider(
    state: State<'_, Mutex<AppState>>,
    provider: String,
) -> Result<CheckOutcome, String> {
    let id = parse_provider(&provider)?;

    let generator = {
        let state = state.lock().map_err(|_| state_error())?;
        state.generator.clone()
    };
    let generator = generator.ok_or_else(|| "no network is configured yet".to_string())?;

    let request = ChatRequest::user("Reply with the single word: ready.")
        .with_system("You are a connectivity probe for a desktop application.")
        .with_temperature(0.0)
        .with_max_tokens(32);

    let started = Instant::now();
    let response = generator
        .registry()
        .chat(id, &request)
        .await
        .map_err(|error| error.to_string())?;

    Ok(CheckOutcome {
        ok: true,
        detail: response.text.trim().chars().take(80).collect(),
        model: response.model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Composes one prayer for the chapel. The scheduler will reuse this command.
#[tauri::command]
async fn generate_prayer(
    state: State<'_, Mutex<AppState>>,
    religion: String,
    short: bool,
    intent: Option<String>,
) -> Result<PrayerOutcome, String> {
    // The lock is released before the network call: only owned data crosses the
    // await boundary.
    let (generator, spec, language) = {
        let state = state.lock().map_err(|_| state_error())?;
        let generator = state
            .generator
            .clone()
            .ok_or_else(|| "no network is configured yet".to_string())?;
        let profile = state
            .profiles
            .get(&religion)
            .ok_or_else(|| format!("unknown religion `{religion}`"))?;
        let mut spec = profile.prayer_spec_for_length(short);
        if let Some(intent) = intent.filter(|value| !value.trim().is_empty()) {
            spec = spec.with_intent(intent);
        }
        let language = prayer_language(&state.prefs.preferences().language);
        (generator, spec, language)
    };

    let spec = spec.with_language(language);
    let started = Instant::now();
    let response = generator
        .generate(&spec)
        .await
        .map_err(|error| error.to_string())?;

    Ok(PrayerOutcome {
        text: response.text.trim().to_string(),
        religion,
        provider: response.provider,
        model: response.model,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `.env` remains the developer workflow; the settings screen stores keys in
    // the application data directory instead.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let store = PrefsStore::open(config_dir.join(SETTINGS_FILE));
            #[cfg(debug_assertions)]
            eprintln!("cyberclergy: settings live in {}", store.path().display());
            app.manage(Mutex::new(AppState::new(store)));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_providers,
            list_religions,
            load_preferences,
            save_preferences,
            save_provider_field,
            clear_provider,
            check_provider,
            generate_prayer
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the CyberClergy window");
}
