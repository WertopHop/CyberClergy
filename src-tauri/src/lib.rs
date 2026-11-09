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
    custom_slot_of, custom_vars, ChatRequest, Health, Intercession, IntercessionRequest, Policy,
    PrayerGenerator, ProfileCatalog, ProviderId, ProviderSpec, Registry, ReligionProfile, Settings,
    MAX_CUSTOM_GATEWAYS, PROVIDER_SPECS,
};
use serde::Serialize;
use tauri::{Manager, State};
use tokio::sync::Notify;

use confessions::{ConfessionStore, CONFESSIONS_FILE};
use history::{now_millis, now_secs, History, PrayerRecord, PrayerStats, HISTORY_FILE};
use prefs::{Preferences, PrefsStore, SETTINGS_FILE};
use scheduler::PrayerBuffers;
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
    /// Prayers a batch already paid for, waiting for their slot's turn.
    buffers: PrayerBuffers,
    /// `CC_BATCH` override; religion profiles carry their own default.
    batch_override: Option<u32>,
}

/// Experimental batch size from the environment, clamped to what the prompt
/// contract allows.
fn batch_from_env() -> Option<u32> {
    std::env::var("CC_BATCH")
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok())
        .map(|value| value.clamp(1, cyberclergy_llm::MAX_BATCH))
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
            buffers: PrayerBuffers::default(),
            batch_override: batch_from_env(),
        };
        state.rebuild_providers();
        state.ensure_schedule(now_secs());
        state
    }

    fn preferences(&self) -> Preferences {
        self.secrets.prefs().preferences()
    }

    /// The prayer language follows the interface language, so a Russian user
    /// gets Russian prayers without touching the prompt. The prompt names the
    /// language in full, which models follow far more reliably than a code.
    fn prayer_language(&self) -> &'static str {
        match self.preferences().language.as_str() {
            "ru" => "Russian",
            "uk" => "Ukrainian",
            "he" => "Hebrew",
            "de" => "German",
            "es" => "Spanish",
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
                if *id == ProviderId::Custom {
                    // One ready gateway is enough to put the custom robot up.
                    return !self.settings.custom().is_empty();
                }
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

    /// Prayers per request for this turn: the experiment override, otherwise
    /// the religion's own batch.
    fn batch(&self, fallback: u32) -> u32 {
        self.batch_override.unwrap_or(fallback)
    }

    fn provider_views(&self) -> Vec<ProviderView> {
        PROVIDER_SPECS
            .iter()
            .map(|spec| {
                if spec.id == ProviderId::Custom {
                    return self.custom_view(spec);
                }
                let config = self.settings.provider(spec.id);
                let mut fields = vec![ProviderField {
                    name: spec.api_key_var.to_string(),
                    secret: is_secret_var(spec.api_key_var),
                    set: config.map(|config| config.api_key.is_some()).unwrap_or(false),
                }];
                for variable in spec.required_vars {
                    // The model has its own editor, so it is listed once.
                    if *variable == spec.model_var {
                        continue;
                    }
                    fields.push(ProviderField {
                        name: variable.to_string(),
                        secret: is_secret_var(variable),
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
                    default_model: spec.default_model.to_string(),
                    model_var: spec.model_var,
                    configured: config.map(|config| config.is_configured()).unwrap_or(false),
                    fields,
                }
            })
            .collect()
    }

    /// The custom network is a single row holding as many gateways as the
    /// person filled in, plus one blank gateway to grow into. Saving an endpoint
    /// therefore reveals the next empty one, so any number of
    /// OpenAI-compatible servers can be added without a separate row each.
    fn custom_view(&self, spec: &'static ProviderSpec) -> ProviderView {
        let last_used = (1..=MAX_CUSTOM_GATEWAYS)
            .filter(|slot| {
                let [_, base, model] = custom_vars(*slot);
                self.settings.has_var(&base) || self.settings.has_var(&model)
            })
            .max()
            .unwrap_or(0);
        let shown = (last_used + 1).clamp(1, MAX_CUSTOM_GATEWAYS);
        let mut fields = Vec::new();
        for slot in 1..=shown {
            let [key, base, model] = custom_vars(slot);
            // Slot one's model has its own editor at the top of the row.
            if slot > 1 {
                fields.push(ProviderField {
                    name: model.clone(),
                    secret: false,
                    set: self.settings.has_var(&model),
                });
            }
            // Where the gateway lives comes first: it defines the endpoint.
            fields.push(ProviderField {
                name: base.clone(),
                secret: false,
                set: self.settings.has_var(&base),
            });
            fields.push(ProviderField {
                name: key.clone(),
                secret: true,
                set: self.settings.has_var(&key),
            });
        }
        let first = self.settings.provider(ProviderId::Custom);
        ProviderView {
            id: ProviderId::Custom,
            model: first.map(|config| config.model.clone()).unwrap_or_default(),
            default_model: spec.default_model.to_string(),
            model_var: spec.model_var,
            configured: !self.settings.custom().is_empty(),
            fields,
        }
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

        let roster_changed = self.roster() != self.schedule_roster;
        if stored.religion != previous.religion
            || stored.language != previous.language
            || roster_changed
        {
            // Prayers already composed for the old religion, language or
            // network no longer fit and must not be spent.
            self.buffers.clear();
        }

        // Only the rhythm-shaping settings reset the schedule; switching a theme
        // must not scatter the clerics' turns.
        let rhythm_changed = stored.religion != previous.religion
            || stored.pace != previous.pace
            || stored.interval_secs != previous.interval_secs
            || roster_changed;
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
        let count = roster.len();
        let cadence = scheduler::cadence_for(&preferences, profile, count);
        self.schedule = (0..count)
            .map(|slot| cadence.first_due(slot, count, now))
            .collect();
    }
}

/// One credential field of a provider, as the settings screen sees it.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderField {
    pub name: String,
    /// Whether the value is currently known (from `.env` or the secret store).
    pub set: bool,
    /// True for keys and tokens, which the interface masks while typing.
    pub secret: bool,
}

/// Provider entry rendered by the settings screen. Only names are exposed;
/// secret values never travel back to the interface.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderView {
    pub id: ProviderId,
    /// Model in effect: the person's choice, or the built-in default.
    pub model: String,
    /// The built-in default, shown under the model editor.
    pub default_model: String,
    /// Variable the model editor writes to, for example `GROQ_MODEL`.
    pub model_var: &'static str,
    pub configured: bool,
    pub fields: Vec<ProviderField>,
}

/// Keys and tokens are typed into masked inputs; base URLs, account ids and
/// model names stay readable, because hiding them only makes them uncheckable.
fn is_secret_var(name: &str) -> bool {
    name.ends_with("_API_KEY") || name.ends_with("_API_TOKEN") || name.ends_with("_TOKEN")
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
    let custom_slot = custom_slot_of(&field);
    if field != spec.api_key_var
        && field != spec.model_var
        && !spec.required_vars.contains(&field.as_str())
        && custom_slot.is_none()
    {
        return Err(format!("`{field}` does not belong to {id}"));
    }
    if custom_slot.is_some() && id != ProviderId::Custom {
        return Err(format!("`{field}` belongs to the custom network, not {id}"));
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
