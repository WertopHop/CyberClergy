use std::collections::BTreeMap;
use std::time::Duration;

use cyberclergy_llm::{ChatRequest, LlmError, ProviderId, Registry, Settings};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const SUCCESS_BODY: &str = r#"{
  "model": "llama-3.3-70b-versatile",
  "choices": [{ "message": { "role": "assistant", "content": "  Online and praying.  " } }],
  "usage": { "prompt_tokens": 12, "completion_tokens": 6, "total_tokens": 18 }
}"#;

fn settings_from(pairs: &[(&str, &str)]) -> Settings {
    let map: BTreeMap<String, String> = pairs
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect();
    Settings::from_vars(&map)
}

fn groq_registry(server: &MockServer, extra: &[(&str, &str)]) -> Registry {
    let base_url = server.uri();
    let mut pairs: Vec<(&str, &str)> = vec![
        ("GROQ_API_KEY", "gsk-test"),
        ("GROQ_BASE_URL", base_url.as_str()),
    ];
    pairs.extend_from_slice(extra);
    Registry::new(&settings_from(&pairs)).expect("registry should build")
}

#[tokio::test]
async fn openai_compatible_success_is_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer gsk-test"))
        .and(body_json(serde_json::json!({
            "model": "llama-3.3-70b-versatile",
            "messages": [
                { "role": "system", "content": "be brief" },
                { "role": "user", "content": "ping" }
            ],
            "temperature": 0.5,
            "max_tokens": 64,
            "stream": false
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS_BODY))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let request = ChatRequest::user("ping")
        .with_system("be brief")
        .with_temperature(0.5)
        .with_max_tokens(64);

    let response = registry.chat(ProviderId::Groq, &request).await.unwrap();

    assert_eq!(response.provider, ProviderId::Groq);
    assert_eq!(response.text, "Online and praying.");
    assert_eq!(response.model, "llama-3.3-70b-versatile");
    let usage = response.usage.expect("usage should be reported");
    assert_eq!(usage.total_tokens, Some(18));
}

#[tokio::test]
async fn shared_defaults_are_applied_to_bare_requests() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(body_json(serde_json::json!({
            "model": "llama-3.3-70b-versatile",
            "messages": [{ "role": "user", "content": "ping" }],
            "temperature": 0.5,
            "max_tokens": 64,
            "stream": false
        })))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS_BODY))
        .mount(&server)
        .await;

    let registry = groq_registry(
        &server,
        &[
            ("CC_DEFAULT_TEMPERATURE", "0.5"),
            ("CC_DEFAULT_MAX_TOKENS", "64"),
            ("CC_MAX_RETRIES", "0"),
        ],
    );

    let response = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap();

    assert_eq!(response.text, "Online and praying.");
}

#[tokio::test]
async fn authentication_failure_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string(
            r#"{"error":{"message":"Invalid API Key","type":"invalid_request_error"}}"#,
        ))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let error = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::Auth {
            provider,
            status,
            detail,
        } => {
            assert_eq!(provider, ProviderId::Groq);
            assert_eq!(status, 401);
            assert!(detail.contains("Invalid API Key"));
        }
        other => panic!("expected an auth error, got {other:?}"),
    }
}

#[tokio::test]
async fn rate_limit_exposes_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "2")
                .set_body_string(r#"{"error":{"message":"Rate limit reached"}}"#),
        )
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let error = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::RateLimited {
            provider,
            retry_after,
            ..
        } => {
            assert_eq!(provider, ProviderId::Groq);
            assert_eq!(retry_after, Some(Duration::from_secs(2)));
        }
        other => panic!("expected a rate limit error, got {other:?}"),
    }
}

#[tokio::test]
async fn transient_server_error_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("upstream boom"))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS_BODY))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "1")]);
    let response = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap();

    assert_eq!(response.text, "Online and praying.");
}

#[tokio::test]
async fn fallback_moves_to_the_next_provider() {
    let groq = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(403).set_body_string(r#"{"error":{"message":"no"}}"#))
        .mount(&groq)
        .await;

    let openrouter = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(SUCCESS_BODY))
        .mount(&openrouter)
        .await;

    let groq_url = groq.uri();
    let openrouter_url = openrouter.uri();
    let registry = Registry::new(&settings_from(&[
        ("GROQ_API_KEY", "gsk-test"),
        ("GROQ_BASE_URL", groq_url.as_str()),
        ("OPENROUTER_API_KEY", "sk-or-test"),
        ("OPENROUTER_BASE_URL", openrouter_url.as_str()),
        ("CC_PROVIDER_ORDER", "groq,openrouter"),
        ("CC_MAX_RETRIES", "0"),
    ]))
    .expect("registry should build");

    let response = registry
        .chat_with_fallback(&ChatRequest::user("ping"))
        .await
        .unwrap();

    assert_eq!(response.provider, ProviderId::OpenRouter);
}

#[tokio::test]
async fn every_provider_failing_returns_an_aggregated_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string(r#"{"error":{"message":"no"}}"#))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let error = registry
        .chat_with_fallback(&ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::AllProvidersFailed { attempts } => {
            assert!(attempts.contains("groq"));
            assert!(attempts.contains("authentication failed"));
        }
        other => panic!("expected an aggregated failure, got {other:?}"),
    }
}

#[tokio::test]
async fn unconfigured_provider_reports_not_configured() {
    let settings = settings_from(&[("GROQ_API_KEY", "gsk-test")]);
    let registry = Registry::new(&settings).expect("registry should build");

    assert!(!registry.is_available(ProviderId::OpenRouter));
    assert_eq!(registry.available(), vec![ProviderId::Groq]);
    assert!(matches!(
        registry.chat(ProviderId::OpenRouter, &ChatRequest::user("ping")).await,
        Err(LlmError::NotConfigured(ProviderId::OpenRouter))
    ));
}

#[test]
fn a_local_runtime_is_enabled_by_its_model_not_a_key() {
    let settings = settings_from(&[("OLLAMA_MODEL", "llama3.2")]);
    let ollama = settings.provider(ProviderId::Ollama).expect("ollama is known");
    assert!(ollama.is_configured(), "no key is needed for a local model");
    assert_eq!(ollama.model, "llama3.2");
    assert_eq!(ollama.base_url, "http://localhost:11434/v1");

    // Without a model the runtime stays off, so a fresh install never stands a
    // robot on the cloud that cannot answer.
    let empty = settings_from(&[]);
    assert!(!empty
        .provider(ProviderId::Ollama)
        .expect("ollama is known")
        .is_configured());
}

#[test]
fn a_custom_gateway_needs_a_base_url_and_a_model() {
    let half = settings_from(&[("CUSTOM_BASE_URL", "http://127.0.0.1:8080/v1")]);
    assert!(!half
        .provider(ProviderId::Custom)
        .expect("custom is known")
        .is_configured());
    assert_eq!(
        half.provider(ProviderId::Custom)
            .expect("custom is known")
            .missing_vars(),
        vec!["CUSTOM_MODEL"]
    );

    let full = settings_from(&[
        ("CUSTOM_BASE_URL", "http://127.0.0.1:8080/v1"),
        ("CUSTOM_MODEL", "qwen2.5"),
    ]);
    let custom = full.provider(ProviderId::Custom).expect("custom is known");
    assert!(custom.is_configured(), "a keyless gateway is allowed");
    assert_eq!(custom.base_url, "http://127.0.0.1:8080/v1");
    assert_eq!(custom.model, "qwen2.5");
}

#[tokio::test]
async fn registry_without_any_key_reports_no_providers() {
    let registry = Registry::new(&Settings::from_vars(&BTreeMap::new())).expect("registry builds");

    assert!(registry.available().is_empty());
    assert!(matches!(
        registry.chat_with_fallback(&ChatRequest::user("ping")).await,
        Err(LlmError::NoProvidersConfigured)
    ));
}

#[tokio::test]
async fn gemini_success_is_parsed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.5-flash:generateContent"))
        .and(header("x-goog-api-key", "gm-test"))
        .and(body_json(serde_json::json!({
            "contents": [{ "role": "user", "parts": [{ "text": "ping" }] }],
            "systemInstruction": { "parts": [{ "text": "be brief" }] },
            "generationConfig": { "temperature": 0.5, "maxOutputTokens": 64 }
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "candidates": [{
                "content": { "parts": [{ "text": "Amen. " }, { "text": "Always." }] },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 3,
                "totalTokenCount": 8
            },
            "modelVersion": "gemini-2.5-flash-001"
        })))
        .mount(&server)
        .await;

    let base_url = server.uri();
    let registry = Registry::new(&settings_from(&[
        ("GEMINI_API_KEY", "gm-test"),
        ("GEMINI_BASE_URL", base_url.as_str()),
        ("CC_MAX_RETRIES", "0"),
    ]))
    .expect("registry should build");

    let request = ChatRequest::user("ping")
        .with_system("be brief")
        .with_temperature(0.5)
        .with_max_tokens(64);
    let response = registry.chat(ProviderId::Gemini, &request).await.unwrap();

    assert_eq!(response.text, "Amen. Always.");
    assert_eq!(response.model, "gemini-2.5-flash-001");
    assert_eq!(
        response.usage.unwrap().total_tokens,
        Some(8)
    );
}

#[tokio::test]
async fn gemini_blocked_prompt_is_a_bad_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.5-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "promptFeedback": { "blockReason": "SAFETY" },
            "candidates": []
        })))
        .mount(&server)
        .await;

    let base_url = server.uri();
    let registry = Registry::new(&settings_from(&[
        ("GEMINI_API_KEY", "gm-test"),
        ("GEMINI_BASE_URL", base_url.as_str()),
        ("CC_MAX_RETRIES", "0"),
    ]))
    .expect("registry should build");

    let error = registry
        .chat(ProviderId::Gemini, &ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::BadRequest { detail, .. } => assert!(detail.contains("SAFETY")),
        other => panic!("expected a bad request error, got {other:?}"),
    }
}

#[tokio::test]
async fn cloudflare_success_reads_the_result_field() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(
            "/accounts/acc-123/ai/run/@cf/meta/llama-3.1-8b-instruct",
        ))
        .and(header("authorization", "Bearer cf-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "success": true,
            "result": {
                "response": "  Praying for you.  ",
                "usage": {
                    "prompt_tokens": 7,
                    "completion_tokens": 4,
                    "total_tokens": 11
                }
            },
            "errors": []
        })))
        .mount(&server)
        .await;

    let base_url = server.uri();
    let registry = Registry::new(&settings_from(&[
        ("CLOUDFLARE_ACCOUNT_ID", "acc-123"),
        ("CLOUDFLARE_API_TOKEN", "cf-token"),
        ("CLOUDFLARE_BASE_URL", base_url.as_str()),
        ("CC_MAX_RETRIES", "0"),
    ]))
    .expect("registry should build");

    let response = registry
        .chat(ProviderId::Cloudflare, &ChatRequest::user("ping"))
        .await
        .unwrap();

    assert_eq!(response.text, "Praying for you.");
    assert_eq!(response.usage.unwrap().total_tokens, Some(11));
}

#[tokio::test]
async fn cloudflare_without_account_is_not_registered() {
    let registry = Registry::new(&settings_from(&[("CLOUDFLARE_API_TOKEN", "cf-token")]))
        .expect("registry should build");

    assert!(!registry.is_available(ProviderId::Cloudflare));
    assert!(matches!(
        registry
            .chat(ProviderId::Cloudflare, &ChatRequest::user("ping"))
            .await,
        Err(LlmError::NotConfigured(ProviderId::Cloudflare))
    ));
}

#[tokio::test]
async fn structured_content_parts_are_joined() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "llama-3.3-70b-versatile",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": [
                        { "type": "text", "text": "Amen. " },
                        { "type": "text", "text": "So be it." }
                    ]
                },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let response = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap();

    assert_eq!(response.text, "Amen. So be it.");
    assert!(response.usage.is_none());
}

#[tokio::test]
async fn exhausted_budget_reports_an_actionable_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "some-reasoning-model",
            "choices": [{
                "message": { "role": "assistant", "content": null },
                "finish_reason": "length"
            }]
        })))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let error = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::Parse { detail, .. } => {
            assert!(detail.contains("token budget"));
            assert!(detail.contains("length"));
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
}

#[tokio::test]
async fn reasoning_only_response_is_explained() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "model": "some-reasoning-model",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "",
                    "reasoning_content": "The user wants a short confirmation."
                },
                "finish_reason": "stop"
            }]
        })))
        .mount(&server)
        .await;

    let registry = groq_registry(&server, &[("CC_MAX_RETRIES", "0")]);
    let error = registry
        .chat(ProviderId::Groq, &ChatRequest::user("ping"))
        .await
        .unwrap_err();

    match error {
        LlmError::Parse { detail, .. } => assert!(detail.contains("only reasoning content")),
        other => panic!("expected a parse error, got {other:?}"),
    }
}
