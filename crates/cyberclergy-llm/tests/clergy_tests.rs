use std::fs;
use std::path::{Path, PathBuf};

use cyberclergy_llm::clergy::{
    ProfileCatalog, ProfileError, ADDENDUM_FILE, OFFLINE_FILE, PROFILE_FILE,
};
use cyberclergy_llm::{build_prayer_request, PromptTemplate};

fn embedded() -> ProfileCatalog {
    ProfileCatalog::embedded().expect("built-in profiles must be valid")
}

fn temp_dir(name: &str) -> PathBuf {
    let directory =
        std::env::temp_dir().join(format!("cyberclergy-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("temp directory is created");
    directory
}

const OFFLINE_CANON: &str = r#"{
  "version": 1,
  "prayers": { "en": ["A short quiet prayer."] }
}"#;

fn write_profile(directory: &Path, folder: &str, json: &str, addendum: &str) -> PathBuf {
    let path = directory.join(folder);
    fs::create_dir_all(&path).expect("profile folder is created");
    fs::write(path.join(PROFILE_FILE), json).expect("profile json is written");
    fs::write(path.join(ADDENDUM_FILE), addendum).expect("addendum is written");
    fs::write(path.join(OFFLINE_FILE), OFFLINE_CANON).expect("offline canon is written");
    path
}

const CUSTOM_PROFILE: &str = r#"{
  "id": "judaism",
  "version": 2,
  "identity": {
    "names": { "en": "Custom Judaism", "ru": "Особый иудаизм" },
    "tradition": "a custom tradition description",
    "prayer_term": "tefillah",
    "closing_formula": null
  },
  "style": {
    "tone": "calm and quiet",
    "length": "one sentence",
    "short_length": "five words"
  },
  "schedule": {
    "interval_seconds": 600,
    "min_interval_seconds": 60
  }
}"#;

#[test]
fn embedded_catalog_contains_the_three_initial_religions() {
    let catalog = embedded();

    assert_eq!(
        catalog.ids(),
        vec!["christianity", "islam", "judaism"],
        "the initial set is Judaism, Christianity and Islam"
    );
    assert_eq!(catalog.len(), 3);
}

#[test]
fn builtin_profiles_are_localised_and_valid() {
    let catalog = embedded();

    let judaism = catalog.get("judaism").expect("judaism is embedded");
    assert_eq!(judaism.localized_name("en"), "Judaism");
    assert_eq!(judaism.localized_name("ru"), "Иудаизм");
    assert_eq!(judaism.localized_name("de"), "Judaism");
    assert_eq!(judaism.identity.prayer_term, "tefillah");
    assert!(judaism.addendum.contains("HaShem"));

    let christianity = catalog.get("christianity").expect("christianity is embedded");
    assert_eq!(christianity.identity.prayer_term, "prayer");
    assert_eq!(christianity.identity.closing_formula.as_deref(), Some("Amen"));

    let islam = catalog.get("islam").expect("islam is embedded");
    assert_eq!(islam.identity.prayer_term, "du'a");
    assert!(islam.addendum.contains("du'a"));

    for profile in catalog.list() {
        profile
            .validate()
            .unwrap_or_else(|error| panic!("profile {} is invalid: {error}", profile.id));
        assert!(!profile.addendum.trim().is_empty());
    }
}

#[test]
fn quiet_canon_follows_the_language_and_rotates() {
    let catalog = embedded();
    let profile = catalog.get("christianity").expect("christianity is embedded");

    let first = profile.quiet_prayer("ru", 0).expect("russian canon exists");
    assert!(first.contains("Господи"), "the canon is translated");
    assert_ne!(first, profile.quiet_prayer("ru", 1).expect("a second prayer exists"));

    // An unknown interface language still prays, in English.
    assert_eq!(
        profile.quiet_prayer("de", 0),
        profile.quiet_prayer("en", 0)
    );
    assert!(profile.offline.len("ru") >= 3);
}

#[test]
fn profile_specs_shape_the_shared_system_prompt() {
    let catalog = embedded();
    let template = PromptTemplate::system_prompt();

    let cases = [
        ("judaism", "tefillah"),
        ("christianity", "mainstream denominations"),
        ("islam", "ritual salah"),
    ];

    for (id, marker) in cases {
        let profile = catalog.get(id).expect("profile exists");
        let request = build_prayer_request(&template, &profile.prayer_spec())
            .unwrap_or_else(|error| panic!("profile {id} does not render: {error}"));
        let system = request.system.expect("system prompt is present");

        assert!(system.contains(&profile.identity.tradition));
        assert!(system.contains(marker), "profile {id} addendum is missing");
        assert!(system.contains(&profile.style.tone));
        assert!(!system.contains("{{"));
    }
}

#[test]
fn short_specs_use_the_compact_length() {
    let catalog = embedded();
    let profile = catalog.get("islam").expect("islam is embedded");

    let normal = profile.prayer_spec();
    let short = profile.short_prayer_spec();

    assert_eq!(normal.length.as_deref(), Some(profile.style.length.as_str()));
    assert_eq!(
        short.length.as_deref(),
        Some(profile.style.short_length.as_str())
    );
    assert_ne!(normal.length, short.length);
    assert_eq!(short.tone, normal.tone);
    assert_eq!(short.tradition_notes, normal.tradition_notes);
}

#[test]
fn caller_overrides_win_over_profile_defaults() {
    let catalog = embedded();
    let profile = catalog.get("judaism").expect("judaism is embedded");

    let spec = profile
        .prayer_spec()
        .with_language("Russian")
        .with_intent("health for a parent")
        .with_user_name("Leah");

    assert_eq!(spec.language.as_deref(), Some("Russian"));
    assert_eq!(spec.intent.as_deref(), Some("health for a parent"));
    assert_eq!(spec.user_name.as_deref(), Some("Leah"));
    assert_eq!(spec.tone.as_deref(), Some(profile.style.tone.as_str()));
}

#[test]
fn user_directory_replaces_and_adds_profiles() {
    let directory = temp_dir("profiles-override");
    write_profile(&directory, "judaism", CUSTOM_PROFILE, "- a custom note");

    let new_faith = r#"{
  "id": "new-faith",
  "version": 1,
  "identity": {
    "names": { "en": "New Faith" },
    "tradition": "a freshly added tradition",
    "prayer_term": "invocation",
    "closing_formula": null
  },
  "style": {
    "tone": "steady",
    "length": "one paragraph",
    "short_length": "one line"
  },
  "schedule": { "interval_seconds": 900, "min_interval_seconds": 300 }
}"#;
    write_profile(&directory, "new-faith", new_faith, "- be gentle");

    let catalog = embedded()
        .with_overrides(&directory)
        .expect("overrides load");

    assert_eq!(catalog.len(), 4);
    let judaism = catalog.get("judaism").expect("judaism still exists");
    assert_eq!(judaism.localized_name("en"), "Custom Judaism");
    assert_eq!(judaism.version, 2);
    assert_eq!(judaism.addendum, "- a custom note");
    assert!(catalog.get("new-faith").is_some());

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn unknown_override_directory_is_ignored() {
    let catalog = embedded();

    let result = catalog.with_overrides(Path::new("this-directory-does-not-exist-cyberclergy"));

    assert!(result.is_ok());
}

#[test]
fn invalid_json_is_reported() {
    let directory = temp_dir("profiles-invalid-json");
    write_profile(&directory, "broken", "{ not json", "- note");

    let error = ProfileCatalog::load_dir(&directory).expect_err("invalid json must fail");

    assert!(matches!(error, ProfileError::InvalidJson { id, .. } if id == "broken"));

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn missing_addendum_is_reported() {
    let directory = temp_dir("profiles-missing-addendum");
    let folder = directory.join("judaism");
    fs::create_dir_all(&folder).expect("folder is created");
    fs::write(folder.join(PROFILE_FILE), CUSTOM_PROFILE).expect("profile json is written");

    let error = ProfileCatalog::load_dir(&directory).expect_err("missing addendum must fail");

    match error {
        ProfileError::MissingFile { path, .. } => {
            assert!(path.ends_with(ADDENDUM_FILE));
        }
        other => panic!("expected a missing file error, got {other:?}"),
    }

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn blank_tradition_is_rejected() {
    let directory = temp_dir("profiles-blank-tradition");
    let json = CUSTOM_PROFILE.replace("a custom tradition description", "   ");
    write_profile(&directory, "judaism", &json, "- note");

    let error = ProfileCatalog::load_dir(&directory).expect_err("blank tradition must fail");

    assert!(matches!(error, ProfileError::Validation { detail, .. } if detail.contains("tradition")));

    let _ = fs::remove_dir_all(&directory);
}

#[test]
fn catalog_refuses_duplicate_ids() {
    let mut catalog = embedded();
    let duplicate = catalog.get("islam").expect("islam is embedded").clone();

    let error = catalog.insert(duplicate).expect_err("duplicates must fail");

    assert!(matches!(error, ProfileError::DuplicateId { id } if id == "islam"));
}
