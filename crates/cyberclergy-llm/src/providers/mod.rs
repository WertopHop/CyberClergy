//! Protocol-specific adapter implementations.

pub mod cloudflare;
pub mod gemini;
pub mod openai_compat;

use std::time::Duration;

use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;

use crate::errors::LlmError;
use crate::types::ProviderId;

/// Reads `Retry-After` when the provider sends it as a plain number of seconds.
///
/// HTTP-date values are ignored on purpose: all supported free tiers send
/// seconds, and a wrong delay is worse than falling back to local backoff.
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get(RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .map(|seconds| Duration::from_secs_f64(seconds.min(3_600.0)))
}

/// Maps an unsuccessful HTTP response onto the typed error taxonomy.
pub(crate) fn map_status(
    provider: ProviderId,
    status: StatusCode,
    retry_after: Option<Duration>,
    body: &str,
) -> LlmError {
    let detail = extract_error_message(body);
    let code = status.as_u16();
    match code {
        401 | 403 => LlmError::Auth {
            provider,
            status: code,
            detail,
        },
        402 => LlmError::QuotaExceeded { provider, detail },
        429 => LlmError::RateLimited {
            provider,
            retry_after,
            detail,
        },
        400 | 404 | 405 | 409 | 413 | 415 | 422 => LlmError::BadRequest {
            provider,
            status: code,
            detail,
        },
        _ if status.is_server_error() => LlmError::Server {
            provider,
            status: code,
            detail,
        },
        _ => LlmError::BadRequest {
            provider,
            status: code,
            detail,
        },
    }
}

/// Turns a transport-level failure into a typed error, keeping timeouts distinct.
pub(crate) fn transport_error(
    provider: ProviderId,
    error: reqwest::Error,
    timeout: Duration,
) -> LlmError {
    if error.is_timeout() {
        LlmError::Timeout { provider, timeout }
    } else if error.is_decode() {
        LlmError::Parse {
            provider,
            detail: truncate(&error.to_string(), 400),
        }
    } else {
        LlmError::Network {
            provider,
            detail: truncate(&error.to_string(), 400),
        }
    }
}

/// Extracts a human-readable message from the three error shapes we meet:
/// `{"error":{"message":...}}`, `{"message":...}` and
/// `{"errors":[{"message":...}]}`.
pub(crate) fn extract_error_message(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return "empty response body".to_string();
    }
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(message) = value.pointer("/error/message").and_then(|v| v.as_str()) {
            return truncate(message, 400);
        }
        if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
            return truncate(message, 400);
        }
        if let Some(errors) = value.get("errors").and_then(|v| v.as_array()) {
            let joined = errors
                .iter()
                .filter_map(|error| error.get("message").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
                .join("; ");
            if !joined.is_empty() {
                return truncate(&joined, 400);
            }
        }
        if let Some(message) = value.get("detail").and_then(|v| v.as_str()) {
            return truncate(message, 400);
        }
    }
    truncate(trimmed, 400)
}

pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    let mut result: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        result.push('\u{2026}');
    }
    result
}

#[cfg(test)]
mod tests {
    use super::extract_error_message;

    #[test]
    fn reads_openai_error_shape() {
        let body = r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#;
        assert_eq!(extract_error_message(body), "Incorrect API key provided");
    }

    #[test]
    fn reads_cloudflare_error_shape() {
        let body = r#"{"errors":[{"code":1000,"message":"Rate limit exceeded"}]}"#;
        assert_eq!(extract_error_message(body), "Rate limit exceeded");
    }

    #[test]
    fn falls_back_to_raw_body_for_html_pages() {
        assert_eq!(
            extract_error_message("<html>Bad gateway</html>"),
            "<html>Bad gateway</html>"
        );
    }
}
