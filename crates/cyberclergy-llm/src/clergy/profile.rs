//! Profile model, validation and catalog loading.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::prompts::PrayerSpec;

pub const PROFILE_FILE: &str = "profile.json";
pub const ADDENDUM_FILE: &str = "prompt_addendum.md";
/// Prayers used while the networks are silent. Kept in the profile folder so a
/// new tradition brings its own canon without touching code.
pub const OFFLINE_FILE: &str = "offline.json";

/// Longest entry accepted in the offline canon; the chapel shows it in a small
/// bubble, so a paragraph is already too much.
const MAX_OFFLINE_PRAYER_CHARS: usize = 400;

/// Compile-time table of bundled profiles. Adding a religion means creating a
/// folder under `resources/clergy/` and one row here; nothing is read from disk
/// at runtime, so packaged binaries always carry their clergy with them.
struct BuiltinProfile {
    id: &'static str,
    json: &'static str,
    addendum: &'static str,
    offline: &'static str,
}

const BUILTIN_PROFILES: &[BuiltinProfile] = &[
    BuiltinProfile {
        id: "judaism",
        json: include_str!("../../resources/clergy/judaism/profile.json"),
        addendum: include_str!("../../resources/clergy/judaism/prompt_addendum.md"),
        offline: include_str!("../../resources/clergy/judaism/offline.json"),
    },
    BuiltinProfile {
        id: "christianity",
        json: include_str!("../../resources/clergy/christianity/profile.json"),
        addendum: include_str!("../../resources/clergy/christianity/prompt_addendum.md"),
        offline: include_str!("../../resources/clergy/christianity/offline.json"),
    },
    BuiltinProfile {
        id: "islam",
        json: include_str!("../../resources/clergy/islam/profile.json"),
        addendum: include_str!("../../resources/clergy/islam/prompt_addendum.md"),
        offline: include_str!("../../resources/clergy/islam/offline.json"),
    },
];

/// Everything the application knows about one religion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReligionProfile {
    pub id: String,
    pub version: u32,
    pub identity: Identity,
    pub style: Style,
    /// Prayers one request should yield. Free tiers bill per request, so the
    /// scheduler asks for a batch and spends it at the ordinary rhythm; an
    /// interactive caller keeps the prompt default of one.
    #[serde(default = "default_profile_batch")]
    pub batch: u32,
    pub schedule: Schedule,
    /// Loaded from `prompt_addendum.md` next to the profile, never from JSON.
    #[serde(skip)]
    pub addendum: String,
    /// Loaded from `offline.json` next to the profile, never from JSON.
    #[serde(skip)]
    pub offline: OfflineCanon,
}

/// Short prayers of a tradition, used when no network can be reached.
///
/// They are intentionally generic: while the chapel is offline it does not know
/// what the person is carrying, so it prays the words the tradition already
/// agrees on instead of inventing anything.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OfflineCanon {
    /// Format version of `offline.json`, mirroring `profile.json`.
    pub version: u32,
    /// Language tag (`en`, `ru`, ...) mapped to its prayers.
    pub prayers: BTreeMap<String, Vec<String>>,
}

impl OfflineCanon {
    pub fn is_empty(&self) -> bool {
        self.prayers.values().all(Vec::is_empty)
    }

    pub fn len(&self, language: &str) -> usize {
        self.prayers.get(language).map(Vec::len).unwrap_or(0)
    }

    /// Picks a prayer by rotation, falling back to English and finally to the
    /// first non-empty list, so an unknown interface language still prays.
    pub fn pick(&self, language: &str, rotation: u64) -> Option<&str> {
        let list = self
            .prayers
            .get(language)
            .filter(|prayers| !prayers.is_empty())
            .or_else(|| {
                self.prayers
                    .get("en")
                    .filter(|prayers| !prayers.is_empty())
            })
            .or_else(|| self.prayers.values().find(|prayers| !prayers.is_empty()))?;
        let index = (rotation % list.len() as u64) as usize;
        list.get(index).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Identity {
    /// Localised display names, keyed by language tag (`en`, `ru`, ...).
    pub names: BTreeMap<String, String>,
    /// Description injected into the `{{tradition}}` placeholder.
    pub tradition: String,
    /// How this tradition calls the act of prayer (`tefillah`, `du'a`, ...).
    pub prayer_term: String,
    #[serde(default)]
    pub closing_formula: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    pub tone: String,
    /// Default length for interactive use.
    pub length: String,
    /// Compact length for the continuous scheduler, so it can save tokens
    /// without needing a second prompt file.
    pub short_length: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Schedule {
    pub interval_seconds: u64,
    pub min_interval_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    InvalidJson {
        id: String,
        detail: String,
    },
    MissingFile {
        path: PathBuf,
        detail: String,
    },
    Validation {
        id: String,
        detail: String,
    },
    DuplicateId {
        id: String,
    },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProfileError::InvalidJson { id, detail } => {
                write!(f, "profile `{id}` is not valid JSON: {detail}")
            }
            ProfileError::MissingFile { path, detail } => {
                write!(f, "{}: {detail}", path.display())
            }
            ProfileError::Validation { id, detail } => {
                write!(f, "profile `{id}` is invalid: {detail}")
            }
            ProfileError::DuplicateId { id } => {
                write!(f, "profile `{id}` is already registered")
            }
        }
    }
}

impl std::error::Error for ProfileError {}

impl ReligionProfile {
    /// Display name for the requested language, falling back to English and
    /// finally to the raw id.
    pub fn localized_name(&self, language: &str) -> &str {
        self.identity
            .names
            .get(language)
            .map(String::as_str)
            .or_else(|| self.identity.names.get("en").map(String::as_str))
            .unwrap_or(self.id.as_str())
    }

    /// Prayer spec with the profile's default length.
    pub fn prayer_spec(&self) -> PrayerSpec {
        self.prayer_spec_for_length(false)
    }

    /// Prayer spec with the compact length intended for the scheduler.
    pub fn short_prayer_spec(&self) -> PrayerSpec {
        self.prayer_spec_for_length(true)
    }

    /// A prayer from the tradition's own canon, for the quiet hours when the
    /// networks cannot be reached. `rotation` walks the list so the chapel does
    /// not repeat the same words forever.
    pub fn quiet_prayer(&self, language: &str, rotation: u64) -> Option<&str> {
        self.offline.pick(language, rotation)
    }

    pub fn prayer_spec_for_length(&self, short: bool) -> PrayerSpec {
        let length = if short {
            &self.style.short_length
        } else {
            &self.style.length
        };
        PrayerSpec::new()
            .with_tradition(self.identity.tradition.clone())
            .with_tone(self.style.tone.clone())
            .with_length(length.clone())
            .with_tradition_notes(self.addendum.clone())
    }

    /// Fails when a hand-edited profile would produce a broken prompt.
    pub fn validate(&self) -> Result<(), ProfileError> {
        let invalid = |detail: String| ProfileError::Validation {
            id: self.id.clone(),
            detail,
        };

        let id_is_valid = !self.id.is_empty()
            && self.id.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            });
        if !id_is_valid {
            return Err(invalid(
                "id must contain only lowercase latin letters, digits or hyphens".to_string(),
            ));
        }
        if self.version == 0 {
            return Err(invalid("version must be at least 1".to_string()));
        }
        if self
            .identity
            .names
            .get("en")
            .map(|name| name.trim().is_empty())
            .unwrap_or(true)
        {
            return Err(invalid(
                "identity.names.en must not be blank".to_string(),
            ));
        }
        let required_fields = [
            ("identity.tradition", &self.identity.tradition),
            ("identity.prayer_term", &self.identity.prayer_term),
            ("style.tone", &self.style.tone),
            ("style.length", &self.style.length),
            ("style.short_length", &self.style.short_length),
        ];
        for (field, value) in required_fields {
            if value.trim().is_empty() {
                return Err(invalid(format!("{field} must not be blank")));
            }
        }
        if self.offline.prayers.get("en").map(Vec::is_empty).unwrap_or(true) {
            return Err(invalid(
                "offline.json must contain at least one English prayer".to_string(),
            ));
        }
        for (language, prayers) in &self.offline.prayers {
            for (index, prayer) in prayers.iter().enumerate() {
                let trimmed = prayer.trim();
                if trimmed.is_empty() {
                    return Err(invalid(format!(
                        "offline.json has an empty {language} prayer at position {}",
                        index + 1
                    )));
                }
                if trimmed.chars().count() > MAX_OFFLINE_PRAYER_CHARS {
                    return Err(invalid(format!(
                        "offline.json {language} prayer {} is longer than {MAX_OFFLINE_PRAYER_CHARS} characters",
                        index + 1
                    )));
                }
            }
        }
        if self.batch == 0 || self.batch > crate::batch::MAX_BATCH {
            return Err(invalid(format!(
                "batch must be between 1 and {}",
                crate::batch::MAX_BATCH
            )));
        }
        if self.schedule.min_interval_seconds < 60 {
            return Err(invalid(
                "schedule.min_interval_seconds must be at least 60".to_string(),
            ));
        }
        if self.schedule.interval_seconds < self.schedule.min_interval_seconds {
            return Err(invalid(
                "schedule.interval_seconds must not be below schedule.min_interval_seconds"
                    .to_string(),
            ));
        }
        if self.addendum.trim().is_empty() {
            return Err(invalid(
                "the prompt addendum must not be empty".to_string(),
            ));
        }
        Ok(())
    }
}

/// In-memory set of profiles, keyed by id.
#[derive(Debug, Clone, Default)]
pub struct ProfileCatalog {
    profiles: BTreeMap<String, ReligionProfile>,
}

impl ProfileCatalog {
    /// Loads the profiles compiled into the binary.
    pub fn embedded() -> Result<Self, ProfileError> {
        let mut catalog = Self::default();
        for builtin in BUILTIN_PROFILES {
            let profile = parse_profile(
                builtin.json,
                builtin.addendum,
                builtin.offline,
                builtin.id,
            )?;
            if profile.id != builtin.id {
                return Err(ProfileError::Validation {
                    id: profile.id,
                    detail: format!("profile id does not match its builtin key `{}`", builtin.id),
                });
            }
            catalog.insert(profile)?;
        }
        Ok(catalog)
    }

    /// Reads `profile.json`, `prompt_addendum.md` and `offline.json` from every
    /// subdirectory.
    pub fn load_dir(directory: &Path) -> Result<Vec<ReligionProfile>, ProfileError> {
        let entries = fs::read_dir(directory).map_err(|error| ProfileError::MissingFile {
            path: directory.to_path_buf(),
            detail: format!("cannot read the profiles directory: {error}"),
        })?;

        let mut profiles = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| ProfileError::MissingFile {
                path: directory.to_path_buf(),
                detail: format!("cannot read a directory entry: {error}"),
            })?;
            let folder = entry.path();
            if !folder.is_dir() {
                continue;
            }
            let folder_name = folder
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();
            let profile = parse_profile(
                &read_text(&folder.join(PROFILE_FILE))?,
                &read_text(&folder.join(ADDENDUM_FILE))?,
                &read_text(&folder.join(OFFLINE_FILE))?,
                &folder_name,
            )?;
            if profile.id != folder_name {
                return Err(ProfileError::Validation {
                    id: profile.id,
                    detail: format!("profile id does not match its folder name `{folder_name}`"),
                });
            }
            profiles.push(profile);
        }
        Ok(profiles)
    }

    /// Adds user profiles from `directory`, replacing built-ins with the same id.
    /// A missing directory is not an error: most users never create one.
    pub fn with_overrides(mut self, directory: &Path) -> Result<Self, ProfileError> {
        if !directory.exists() {
            return Ok(self);
        }
        for profile in Self::load_dir(directory)? {
            self.replace(profile);
        }
        Ok(self)
    }

    /// Registers a profile, refusing to shadow an existing id.
    pub fn insert(&mut self, profile: ReligionProfile) -> Result<(), ProfileError> {
        if self.profiles.contains_key(&profile.id) {
            return Err(ProfileError::DuplicateId { id: profile.id });
        }
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }

    /// Registers a profile, replacing any profile with the same id.
    pub fn replace(&mut self, profile: ReligionProfile) -> Option<ReligionProfile> {
        self.profiles.insert(profile.id.clone(), profile)
    }

    pub fn get(&self, id: &str) -> Option<&ReligionProfile> {
        self.profiles.get(id)
    }

    /// Ids in alphabetical order.
    pub fn ids(&self) -> Vec<&str> {
        self.profiles.keys().map(String::as_str).collect()
    }

    pub fn list(&self) -> Vec<&ReligionProfile> {
        self.profiles.values().collect()
    }

    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }
}

fn default_profile_batch() -> u32 {
    crate::batch::DEFAULT_PROFILE_BATCH
}

fn parse_profile(
    json: &str,
    addendum: &str,
    offline: &str,
    expected_id: &str,
) -> Result<ReligionProfile, ProfileError> {
    let mut profile: ReligionProfile =
        serde_json::from_str(json).map_err(|error| ProfileError::InvalidJson {
            id: expected_id.to_string(),
            detail: error.to_string(),
        })?;
    profile.addendum = addendum.trim().to_string();
    profile.offline = parse_offline(offline, expected_id)?;
    profile.validate()?;
    Ok(profile)
}

fn parse_offline(json: &str, expected_id: &str) -> Result<OfflineCanon, ProfileError> {
    let canon: OfflineCanon =
        serde_json::from_str(json).map_err(|error| ProfileError::InvalidJson {
            id: expected_id.to_string(),
            detail: format!("offline.json: {error}"),
        })?;
    Ok(canon)
}

fn read_text(path: &Path) -> Result<String, ProfileError> {
    fs::read_to_string(path).map_err(|error| ProfileError::MissingFile {
        path: path.to_path_buf(),
        detail: format!("cannot read the file: {error}"),
    })
}
