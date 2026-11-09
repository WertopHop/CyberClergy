//! Traditions the person created, kept next to their settings.
//!
//! A user tradition is the same three files a bundled one has, written to the
//! application config directory. Loading merges them over the embedded catalog,
//! and a folder that no longer parses is reported without taking the chapel
//! down with it.

use std::fs;
use std::path::{Path, PathBuf};

use cyberclergy_llm::{
    ProfileCatalog, ReligionProfile, ADDENDUM_FILE, OFFLINE_FILE, PROFILE_FILE,
};

pub const CLERGY_DIR: &str = "clergy";

/// Where user traditions live, inside the application config directory.
pub fn directory(config_dir: &Path) -> PathBuf {
    config_dir.join(CLERGY_DIR)
}

/// Embedded traditions plus every user tradition that still loads.
pub fn load(config_dir: &Path) -> ProfileCatalog {
    let embedded = match ProfileCatalog::embedded() {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("embedded religion profiles are invalid: {error}");
            return ProfileCatalog::default();
        }
    };
    match embedded.clone().with_overrides(&directory(config_dir)) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("cannot load the user traditions: {error}");
            embedded
        }
    }
}

/// Writes the three files of one tradition, replacing any earlier version.
pub fn save(config_dir: &Path, profile: &ReligionProfile) -> Result<(), String> {
    let folder = directory(config_dir).join(&profile.id);
    fs::create_dir_all(&folder).map_err(|error| {
        format!("cannot create the tradition folder {}: {error}", folder.display())
    })?;

    // `addendum` and `offline` are skipped by the profile's serializer, so the
    // document and the two side files never duplicate each other.
    let json = serde_json::to_string_pretty(profile)
        .map_err(|error| format!("cannot encode the tradition: {error}"))?;
    write(&folder.join(PROFILE_FILE), &json)?;
    write(
        &folder.join(ADDENDUM_FILE),
        &format!("{}\n", profile.addendum.trim()),
    )?;
    let canon = serde_json::to_string_pretty(&profile.offline)
        .map_err(|error| format!("cannot encode the tradition's prayers: {error}"))?;
    write(&folder.join(OFFLINE_FILE), &canon)
}

fn write(path: &Path, contents: &str) -> Result<(), String> {
    fs::write(path, contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}
