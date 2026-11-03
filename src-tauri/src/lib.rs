//! Tauri entry point for CyberClergy.
//!
//! The shell owns the shared application state: the provider registry built from
//! the environment, the configuration snapshot the UI needs, and the catalog of
//! religion profiles. Profiles are embedded into the binary, so packaged builds
//! always carry their clergy without shipping extra resource files.

use std::sync::Mutex;

use cyberclergy_llm::{ProfileCatalog, ProviderStatus, Registry, ReligionProfile, Settings};

/// Shared state managed by Tauri and handed to commands.
pub struct AppState {
    pub settings: Settings,
    /// `None` when the HTTP client could not be created; the UI still needs the
    /// provider statuses in that case, so the failure must not abort the app.
    pub registry: Option<Registry>,
    /// `None` only if the embedded profiles failed validation, which would be a
    /// build-time bug rather than a user error.
    pub profiles: Option<ProfileCatalog>,
}

#[tauri::command]
fn list_providers(state: tauri::State<'_, Mutex<AppState>>) -> Result<Vec<ProviderStatus>, String> {
    let state = state
        .lock()
        .map_err(|_| "application state is unavailable".to_string())?;
    Ok(state.settings.statuses())
}

#[tauri::command]
fn registry_ready(state: tauri::State<'_, Mutex<AppState>>) -> Result<bool, String> {
    let state = state
        .lock()
        .map_err(|_| "application state is unavailable".to_string())?;
    Ok(state
        .registry
        .as_ref()
        .map(|registry| !registry.available().is_empty())
        .unwrap_or(false))
}

#[tauri::command]
fn list_religions(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<ReligionProfile>, String> {
    let state = state
        .lock()
        .map_err(|_| "application state is unavailable".to_string())?;
    Ok(state
        .profiles
        .as_ref()
        .map(|catalog| catalog.list().into_iter().cloned().collect())
        .unwrap_or_default())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // In development the keys live in `.env`; packaged builds will read the OS
    // keychain instead, which is wired up together with the settings screen.
    let _ = dotenvy::dotenv();

    let settings = Settings::from_env();
    let registry = Registry::new(&settings).ok();
    let profiles = ProfileCatalog::embedded().ok();

    tauri::Builder::default()
        .manage(Mutex::new(AppState {
            settings,
            registry,
            profiles,
        }))
        .invoke_handler(tauri::generate_handler![
            list_providers,
            registry_ready,
            list_religions
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the CyberClergy window");
}
