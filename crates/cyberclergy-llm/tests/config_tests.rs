use std::collections::BTreeMap;
use std::str::FromStr;

use cyberclergy_llm::{ProviderId, Settings};

fn vars(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect()
}

#[test]
fn nothing_is_configured_without_environment() {
    let settings = Settings::from_vars(&BTreeMap::new());
    let statuses = settings.statuses();

    assert_eq!(statuses.len(), ProviderId::ALL.len());
    assert!(statuses.iter().all(|status| !status.configured));
    assert_eq!(settings.shared.max_retries, 2);
    assert_eq!(settings.shared.provider_order, ProviderId::ALL.to_vec());
}

#[test]
fn single_key_enables_only_that_provider() {
    let settings = Settings::from_vars(&vars(&[
        ("GROQ_API_KEY", "  gsk_test  "),
        ("GROQ_MODEL", "custom-model"),
    ]));

    let configured: Vec<ProviderId> = settings
        .statuses()
        .into_iter()
        .filter(|status| status.configured)
        .map(|status| status.id)
        .collect();

    assert_eq!(configured, vec![ProviderId::Groq]);
    assert_eq!(settings.groq.model, "custom-model");
    assert_eq!(settings.groq.api_key.as_deref(), Some("gsk_test"));
}

#[test]
fn blank_values_are_treated_as_missing() {
    let settings = Settings::from_vars(&vars(&[("OPENROUTER_API_KEY", "   ")]));

    assert!(!settings.openrouter.is_configured());
    assert_eq!(settings.openrouter.missing_vars(), vec!["OPENROUTER_API_KEY"]);
}

#[test]
fn cloudflare_requires_both_account_and_token() {
    let settings = Settings::from_vars(&vars(&[("CLOUDFLARE_API_TOKEN", "cf-token")]));

    assert!(!settings.cloudflare.is_configured());
    assert_eq!(settings.cloudflare.missing_vars(), vec!["CLOUDFLARE_ACCOUNT_ID"]);
}

#[test]
fn provider_order_ignores_unknown_entries() {
    let settings = Settings::from_vars(&vars(&[("CC_PROVIDER_ORDER", "groq, nope, gemini")]));

    assert_eq!(
        settings.shared.provider_order,
        vec![ProviderId::Groq, ProviderId::Gemini]
    );
}

#[test]
fn invalid_numbers_fall_back_to_defaults() {
    let settings = Settings::from_vars(&vars(&[
        ("CC_MAX_RETRIES", "abc"),
        ("CC_DEFAULT_TEMPERATURE", "9"),
        ("CC_HTTP_TIMEOUT_SECS", "0"),
        ("CC_DEFAULT_MAX_TOKENS", "1"),
    ]));

    assert_eq!(settings.shared.max_retries, 2);
    assert!((settings.shared.temperature - 0.8).abs() < f32::EPSILON);
    assert_eq!(settings.shared.timeout.as_secs(), 60);
    assert_eq!(settings.shared.max_tokens, 512);
}

#[test]
fn valid_numbers_are_applied() {
    let settings = Settings::from_vars(&vars(&[
        ("CC_MAX_RETRIES", "0"),
        ("CC_DEFAULT_TEMPERATURE", "0.5"),
        ("CC_HTTP_TIMEOUT_SECS", "15"),
        ("CC_DEFAULT_MAX_TOKENS", "64"),
    ]));

    assert_eq!(settings.shared.max_retries, 0);
    assert!((settings.shared.temperature - 0.5).abs() < f32::EPSILON);
    assert_eq!(settings.shared.timeout.as_secs(), 15);
    assert_eq!(settings.shared.max_tokens, 64);
}

#[test]
fn debug_output_never_contains_the_raw_key() {
    let settings = Settings::from_vars(&vars(&[("GROQ_API_KEY", "gsk_super_secret_value")]));
    let rendered = format!("{settings:?}");

    assert!(!rendered.contains("gsk_super_secret_value"));
    assert!(rendered.contains("gsk_\u{2026}alue"));
}

#[test]
fn provider_ids_accept_aliases() {
    assert_eq!(ProviderId::from_str("NIM").unwrap(), ProviderId::Nvidia);
    assert_eq!(
        ProviderId::from_str("google-ai-studio").unwrap(),
        ProviderId::Gemini
    );
    assert_eq!(
        ProviderId::from_str(" workers-ai ").unwrap(),
        ProviderId::Cloudflare
    );
    assert!(ProviderId::from_str("unknown").is_err());
}
