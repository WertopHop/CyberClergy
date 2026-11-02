//! Tauri entry point for CyberClergy.
//!
//! The shell only owns shared application state for now: the provider registry
//! built from the environment, plus the configuration snapshot the UI needs.
//! Chapel screens, key storage and the prayer scheduler arrive in later stages.

use std::sync::Mutex;

use cyberclergy_llm::{ProviderStatus, Registry, Settings};

/// Shared state managed by Tauri and handed to commands.
pub struct AppState {
    pub settings: Settings,
    /// `None` when the HTTP client could not be created; the UI still needs the
    /// provider statuses in that case, so the failure must not abort the app.
    pub registry: Option<Registry>,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // In development the keys live in `.env`; packaged builds will read the OS
    // keychain instead, which is wired up together with the settings screen.
    let _ = dotenvy::dotenv();

    let settings = Settings::from_env();
    let registry = Registry::new(&settings).ok();

    tauri::Builder::default()
        .manage(Mutex::new(AppState { settings, registry }))
        .invoke_handler(tauri::generate_handler![list_providers, registry_ready])
        .run(tauri::generate_context!())
        .expect("failed to start the CyberClergy window");
}
