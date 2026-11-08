//! Tauri entry point for CyberClergy.
//!
//! The shell owns the shared application state:
//!
//! * credentials from [`secrets::SecretStore`] (OS keychain, with `settings.json`
//!   as the fallback) and the provider snapshot derived from them;
//! * the prayer generator and the embedded catalog of religion profiles;
//! * the confessional queue persisted by [`confessions::ConfessionStore`];
//! * the SQLite journal in [`history`];
//! * the rhythm state ([`cyberclergy_llm::Health`]) and the per-cleric schedule.
//!
//! A single background task, [`scheduler`], turns that state into prayer and
//! pushes events to the interface. Network calls are kept out of the state lock:
//! commands copy what they need (an `Arc` to the generator, an owned spec) before
//! awaiting.

mod confessions;
mod history;
mod prefs;
mod scheduler;
mod secrets;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use cyberclergy_llm::{
    Cadence, ChatRequest, Health, Intercession, IntercessionRequest, Policy, PrayerGenerator,
    ProfileCatalog, ProviderId, ProviderSpec, Registry, ReligionProfile, Settings, PROVIDER_SPECS,
};
use serde::Serialize;
use tauri::{Manager, State};
use tokio::sync::Notify;

use confessions::{ConfessionStore, CONFESSIONS_FILE};
use history::{now_millis, now_secs, History, PrayerRecord, PrayerStats, HISTORY_FILE};
use prefs::{Preferences, PrefsStore, SETTINGS_FILE};
use secrets::SecretStore;

/// Every command and the scheduler share this handle.
type Shared = Arc<Mutex<AppState>>;

/// State managed by Tauri and handed to commands.
pub struct AppState {
    secrets: SecretStore,
    /// Snapshot rebuilt whenever a credential changes.
    settings: Settings,
    /// `None` when no provider is configured or the HTTP client could not be
    /// built; the UI still needs provider statuses in that case.
    generator: Option<Arc<PrayerGenerator>>,
    /// Empty only if the embedded profiles failed validation, which would be a
    /// build-time bug rather than a user error.
    profiles: ProfileCatalog,
    confessions: ConfessionStore,
    history: History,
    /// Failures, cooldowns and the hourly budget.
    health: Health,
    /// Wakes the scheduler after a change instead of waiting for the next tick.
    wake: Arc<Notify>,
    /// Next turn of each cleric, Unix seconds.
    schedule: Vec<i64>,
    /// Roster the current schedule was built for, slot by slot.
    schedule_roster: Vec<ProviderId>,
    /// Rotation through the tradition's offline canon.
    quiet_rotation: u64,
    /// Last failure, cleared by the next successful prayer.
    last_error: Option<String>,
}

impl AppState {
    fn new(
        secrets: SecretStore,
        confessions: ConfessionStore,
        history: History,
        policy: Policy,
    ) -> Self {
        let profiles = match ProfileCatalog::embedded() {
            Ok(catalog) => catalog,
            Err(error) => {
                eprintln!("embedded religion profiles are invalid: {error}");
                ProfileCatalog::default()
            }
        };

        let mut state = Self {
            secrets,
            settings: Settings::from_vars(&BTreeMap::new()),
            generator: None,
            profiles,
            confessions,
            history,
            health: Health::new(policy, now_secs()),
            wake: Arc::new(Notify::new()),
            schedule: Vec::new(),
            schedule_roster: Vec::new(),
            quiet_rotation: 0,
            last_error: None,
        };
        state.rebuild_providers();
        state.ensure_schedule(now_secs());
        state
    }

    fn preferences(&self) -> Preferences {
        self.secrets.prefs().preferences()
    }

    /// The prayer language follows the interface language, so a Russian user
    /// gets Russian prayers without touching the prompt.
    fn prayer_language(&self) -> &'static str {
        match self.preferences().language.as_str() {
            "ru" => "Russian",
            _ => "English",
        }
    }

    /// Recomposes provider settings from `.env` plus the credentials held by the
    /// secret store. Stored values win, so the settings screen always has the
    /// last word over whatever a developer left in `.env`.
    fn rebuild_providers(&mut self) {
        let mut vars: BTreeMap<String, String> = std::env::vars().collect();
        vars.extend(self.secrets.keys());
        self.settings = Settings::from_vars(&vars);
        self.generator = Registry::new(&self.settings)
            .ok()
            .map(|registry| Arc::new(PrayerGenerator::new(registry)));
    }

    /// Providers that currently hold every credential they need, in the spec
    /// order. This is what the roster can be drawn from.
    fn configured_ids(&self) -> Vec<ProviderId> {
        PROVIDER_SPECS
            .iter()
            .map(|spec| spec.id)
            .filter(|id| {
                self.settings
                    .provider(*id)
                    .map(|config| config.is_configured())
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Networks standing on the cloud, slot by slot. Slot `i` prays through
    /// `roster[i]`, so the composition and the schedule always agree.
    fn roster(&self) -> Vec<ProviderId> {
        self.preferences().cloud(&self.configured_ids())
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

    /// Keeps preferences pointing at a religion the catalog actually knows and
    /// propagates a changed roster to the queue and the schedule.
    fn set_preferences(&mut self, preferences: Preferences) -> Result<Preferences, String> {
        let mut preferences = preferences.sanitized();
        if self.profiles.get(&preferences.religion).is_none() {
            if let Some(profile) = self.profiles.list().first() {
                preferences.religion = profile.id.clone();
            }
        }

        let previous = self.preferences();
        let stored = self.secrets.prefs_mut().set_preferences(preferences)?;

        // Only the rhythm-shaping settings reset the schedule; switching a theme
        // or a language must not scatter the clerics' turns.
        let rhythm_changed = stored.religion != previous.religion
            || stored.pace != previous.pace
            || self.roster() != self.schedule_roster;
        if rhythm_changed {
            self.schedule.clear();
            self.ensure_schedule(now_secs());
        }
        self.wake.notify_one();
        Ok(stored)
    }

    /// Lazily rebuilds the per-cleric turn list whenever its shape is stale and
    /// hands the same roster to the confessional queue, so slot `i` of both
    /// sides is the same robot.
    fn ensure_schedule(&mut self, now: i64) {
        let roster = self.roster();
        let roster_changed = roster != self.schedule_roster;
        if roster_changed {
            self.schedule_roster = roster.clone();
            self.confessions.queue_mut().set_roster(&roster);
            if let Err(error) = self.confessions.persist() {
                eprintln!("cannot persist the confessional queue: {error}");
            }
        }
        if !roster_changed && !self.schedule.is_empty() {
            return;
        }
        let preferences = self.preferences();
        let Some(profile) = self.profiles.get(&preferences.religion) else {
            self.schedule.clear();
            return;
        };
        let cadence = Cadence::new(
            preferences.pace(),
            profile.schedule.interval_seconds,
            profile.schedule.min_interval_seconds,
        );
        let count = roster.len();
        self.schedule = (0..count)
            .map(|slot| cadence.first_due(slot, count, now))
            .collect();
    }
}

/// One credential field of a provider, as the settings screen sees it.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderField {
    pub name: &'static str,
    /// Whether the value is currently known (from `.env` or the secret store).
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

/// Result of writing an entry in the confessional.
#[derive(Debug, Clone, Serialize)]
pub struct ConfessionOutcome {
    pub entry: Intercession,
    /// The same text is already waiting; nothing new was queued.
    pub duplicate: bool,
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
fn list_providers(state: State<'_, Shared>) -> Result<Vec<ProviderView>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.provider_views())
}

#[tauri::command]
fn list_religions(state: State<'_, Shared>) -> Result<Vec<ReligionProfile>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.profiles.list().into_iter().cloned().collect())
}

#[tauri::command]
fn load_preferences(state: State<'_, Shared>) -> Result<Preferences, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.preferences())
}

#[tauri::command]
fn save_preferences(
    state: State<'_, Shared>,
    preferences: Preferences,
) -> Result<Preferences, String> {
    let mut state = state.lock().map_err(|_| state_error())?;
    state.set_preferences(preferences)
}

#[tauri::command]
fn save_provider_field(
    state: State<'_, Shared>,
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
    state.secrets.set_key(&field, &value)?;
    state.rebuild_providers();
    state.wake.notify_one();
    Ok(state.provider_views())
}

#[tauri::command]
fn clear_provider(state: State<'_, Shared>, provider: String) -> Result<Vec<ProviderView>, String> {
    let id = parse_provider(&provider)?;
    let mut state = state.lock().map_err(|_| state_error())?;
    state.secrets.clear_provider(id)?;
    state.rebuild_providers();
    state.wake.notify_one();
    Ok(state.provider_views())
}

/// Sends a one-line request through a single provider and reports the latency,
/// which is all the settings screen needs to confirm a key works.
#[tauri::command]
async fn check_provider(
    state: State<'_, Shared>,
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

/// Composes one prayer on demand. The scheduler has its own path through the
/// same generator; this command stays for manual checks and previews.
#[tauri::command]
async fn generate_prayer(
    state: State<'_, Shared>,
    religion: String,
    short: bool,
    intent: Option<String>,
) -> Result<PrayerOutcome, String> {
    // The lock is released before the network call: only owned data crosses the
    // await boundary.
    let (generator, spec) = {
        let state = state.lock().map_err(|_| state_error())?;
        let generator = state
            .generator
            .clone()
            .ok_or_else(|| "no network is configured yet".to_string())?;
        let profile = state
            .profiles
            .get(&religion)
            .ok_or_else(|| format!("unknown religion `{religion}`"))?;
        let mut spec = profile
            .prayer_spec_for_length(short)
            .with_language(state.prayer_language().to_string());
        if let Some(intent) = intent.filter(|value| !value.trim().is_empty()) {
            spec = spec.with_intent(intent);
        }
        (generator, spec)
    };

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

#[tauri::command]
fn list_confessions(state: State<'_, Shared>) -> Result<Vec<Intercession>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(state.confessions.queue().entries().to_vec())
}

#[tauri::command]
fn submit_confession(
    state: State<'_, Shared>,
    text: String,
    repeat: Option<u32>,
    endless: Option<bool>,
    provider: Option<String>,
) -> Result<ConfessionOutcome, String> {
    let provider = provider.as_deref().map(parse_provider).transpose()?;
    let request = IntercessionRequest {
        repeat,
        endless: endless.unwrap_or(false),
        provider,
    };
    let mut state = state.lock().map_err(|_| state_error())?;
    let outcome = state
        .confessions
        .queue_mut()
        .submit_with(&text, now_millis(), request)
        .map_err(|error| error.to_string())?;
    state.confessions.persist()?;
    state.wake.notify_one();
    Ok(ConfessionOutcome {
        entry: outcome.entry,
        duplicate: outcome.duplicate,
    })
}

#[tauri::command]
fn cancel_confession(state: State<'_, Shared>, id: String) -> Result<Vec<Intercession>, String> {
    let mut state = state.lock().map_err(|_| state_error())?;
    if !state.confessions.queue_mut().cancel(&id) {
        return Err("this entry is no longer waiting".to_string());
    }
    state.confessions.persist()?;
    state.wake.notify_one();
    Ok(state.confessions.queue().entries().to_vec())
}

/// Deletes finished entries, text included. An explicit "forget" action, so a
/// prayed-over confession does not linger on disk.
#[tauri::command]
fn forget_finished_confessions(state: State<'_, Shared>) -> Result<Vec<Intercession>, String> {
    let mut state = state.lock().map_err(|_| state_error())?;
    state.confessions.queue_mut().forget_finished();
    state.confessions.persist()?;
    Ok(state.confessions.queue().entries().to_vec())
}

#[tauri::command]
fn scheduler_status(
    state: State<'_, Shared>,
) -> Result<scheduler::ChapelStatus, String> {
    let state = state.lock().map_err(|_| state_error())?;
    Ok(scheduler::status_of(&state, now_secs()))
}

#[tauri::command]
fn prayer_stats(state: State<'_, Shared>) -> Result<PrayerStats, String> {
    let state = state.lock().map_err(|_| state_error())?;
    // A rolling day rather than a calendar day: no timezone library, and "the
    // last 24 hours" is what a continuously running chapel actually means.
    state.history.stats(now_millis() - 86_400_000)
}

#[tauri::command]
fn prayer_journal(
    state: State<'_, Shared>,
    limit: Option<u32>,
) -> Result<Vec<PrayerRecord>, String> {
    let state = state.lock().map_err(|_| state_error())?;
    state.history.recent(limit.unwrap_or(20))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // `.env` remains the developer workflow; the settings screen stores keys in
    // the OS keychain, falling back to the application config directory.
    let _ = dotenvy::dotenv();

    tauri::Builder::default()
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;

            let secrets = SecretStore::open(PrefsStore::open(config_dir.join(SETTINGS_FILE)));
            // Slots settle once the roster is known; the widest count keeps the
            // stored entries spread out until then, and `ensure_schedule` modulo
            // folds them onto the real roster.
            let confessions =
                ConfessionStore::open(config_dir.join(CONFESSIONS_FILE), prefs::MAX_ROBOTS);
            #[cfg(debug_assertions)]
            eprintln!(
                "cyberclergy: preferences in {}, credentials in the `{}` store, confessional in {}",
                secrets.prefs().path().display(),
                secrets.backend().as_str(),
                confessions.path().display()
            );
            let history = History::open(&data_dir.join(HISTORY_FILE)).unwrap_or_else(|error| {
                eprintln!("cannot open the prayer journal: {error}; keeping it in memory");
                History::in_memory().expect("the in-memory journal always opens")
            });

            let state: Shared = Arc::new(Mutex::new(AppState::new(
                secrets,
                confessions,
                history,
                scheduler::policy_from_env(),
            )));
            app.manage(state.clone());
            scheduler::spawn(app.handle().clone(), state);
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
            generate_prayer,
            list_confessions,
            submit_confession,
            cancel_confession,
            forget_finished_confessions,
            scheduler_status,
            prayer_stats,
            prayer_journal
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the CyberClergy window");
}
