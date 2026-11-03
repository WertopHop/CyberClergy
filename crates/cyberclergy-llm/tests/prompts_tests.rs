use cyberclergy_llm::prompts::{
    DEFAULT_INTENT, DEFAULT_LANGUAGE, DEFAULT_PRAYER_LENGTH, DEFAULT_TONE, DEFAULT_TRADITION,
    DEFAULT_USER_NAME, KEY_INTENT, KEY_LANGUAGE, KEY_PRAYER_LENGTH, KEY_TONE, KEY_TRADITION,
    KEY_USER_NAME,
};
use cyberclergy_llm::{
    build_prayer_request, PrayerSpec, PromptContext, PromptError, PromptTemplate, Role,
    SYSTEM_PROMPT_TEMPLATE,
};

#[test]
fn system_prompt_starts_with_its_instructions() {
    let first_line = SYSTEM_PROMPT_TEMPLATE.lines().next().unwrap_or_default();

    assert!(first_line.starts_with("You are a member of the CyberClergy"));
}

#[test]
fn system_prompt_declares_the_output_contract() {
    assert!(SYSTEM_PROMPT_TEMPLATE.contains("Output ONLY the text of the prayer"));
    assert!(SYSTEM_PROMPT_TEMPLATE.contains("Write in {{language}}"));
    assert!(SYSTEM_PROMPT_TEMPLATE.contains("{{tradition}}"));
}

#[test]
fn every_placeholder_has_a_default() {
    let template = PromptTemplate::system_prompt();
    let context = PrayerSpec::new().context();

    let placeholders = template.placeholders().expect("placeholders are well formed");
    assert!(!placeholders.is_empty());
    for key in placeholders {
        assert!(
            context.get(&key).is_some(),
            "placeholder `{key}` has no default value"
        );
    }
}

#[test]
fn defaults_are_rendered_without_leftover_braces() {
    let template = PromptTemplate::system_prompt();
    let rendered = template
        .render(&PrayerSpec::new().context())
        .expect("rendering succeeds");

    assert!(rendered.contains(DEFAULT_TRADITION));
    assert!(rendered.contains(DEFAULT_LANGUAGE));
    assert!(rendered.contains(DEFAULT_TONE));
    assert!(rendered.contains(DEFAULT_PRAYER_LENGTH));
    assert!(rendered.contains(DEFAULT_INTENT));
    assert!(rendered.contains(DEFAULT_USER_NAME));
    assert!(!rendered.contains("{{"));
    assert!(!rendered.contains("}}"));
}

#[test]
fn spec_overrides_replace_defaults() {
    let template = PromptTemplate::system_prompt();
    let spec = PrayerSpec::new()
        .with_tradition("Sunni Islam")
        .with_language("Arabic")
        .with_intent("a quiet heart")
        .with_user_name("Amina");
    let rendered = template.render(&spec.context()).expect("rendering succeeds");

    assert!(rendered.contains("cleric of Sunni Islam"));
    assert!(rendered.contains("Write in Arabic."));
    assert!(rendered.contains("this intention: a quiet heart"));
    assert!(rendered.contains("as Amina"));
    assert!(!rendered.contains(DEFAULT_TRADITION));
}

#[test]
fn blank_overrides_fall_back_to_defaults() {
    let rendered = PrayerSpec::new()
        .with_tradition("   ")
        .with_language("")
        .context();

    assert_eq!(rendered.get(KEY_TRADITION), Some(DEFAULT_TRADITION));
    assert_eq!(rendered.get(KEY_LANGUAGE), Some(DEFAULT_LANGUAGE));
}

#[test]
fn default_context_does_not_leak_empty_values() {
    let context = PrayerSpec::new().context();

    for key in [
        KEY_TRADITION,
        KEY_LANGUAGE,
        KEY_TONE,
        KEY_PRAYER_LENGTH,
        KEY_INTENT,
        KEY_USER_NAME,
    ] {
        let value = context.get(key).expect("key has a default");
        assert!(!value.trim().is_empty(), "default for `{key}` is blank");
    }
}

#[test]
fn unknown_placeholder_is_reported() {
    let template = PromptTemplate::new("broken", "Pray for {{nobody_knows_me}}.");

    let error = template
        .render(&PromptContext::new())
        .expect_err("unknown placeholder must fail");

    assert_eq!(
        error,
        PromptError::UnknownPlaceholder {
            template: "broken".to_string(),
            key: "nobody_knows_me".to_string(),
        }
    );
}

#[test]
fn unterminated_placeholder_is_reported() {
    let template = PromptTemplate::new("broken", "Pray for {{tradition");

    assert!(matches!(
        template.render(&PromptContext::new()),
        Err(PromptError::UnterminatedPlaceholder { .. })
    ));
    assert!(template.placeholders().is_err());
}

#[test]
fn empty_template_is_reported() {
    let template = PromptTemplate::new("blank", "   \n  ");

    assert_eq!(
        template.render(&PromptContext::new()),
        Err(PromptError::EmptyTemplate {
            template: "blank".to_string(),
        })
    );
}

#[test]
fn prayer_request_contains_rendered_system_prompt_and_user_turn() {
    let template = PromptTemplate::system_prompt();
    let spec = PrayerSpec::new().with_tradition("Roman Catholic");
    let request = build_prayer_request(&template, &spec).expect("request builds");

    let system = request.system.expect("system prompt is present");
    assert!(system.contains("cleric of Roman Catholic"));
    assert!(!system.contains("{{"));

    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].role, Role::User);
    assert_eq!(
        request.messages[0].content,
        cyberclergy_llm::USER_INSTRUCTION
    );
    assert!(request.model.is_none());
    assert!(request.temperature.is_none());
    assert!(request.max_tokens.is_none());
}
