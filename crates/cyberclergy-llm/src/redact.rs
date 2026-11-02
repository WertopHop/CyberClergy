//! Helpers that keep secrets out of logs, errors and UI payloads.

/// Masks a secret so it can be safely logged.
///
/// Short values are fully masked: showing four leading and four trailing
/// characters of a short token would leak most of it.
pub fn redact_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    match chars.len() {
        0 => "<empty>".to_string(),
        1..=12 => "***".to_string(),
        len => {
            let prefix: String = chars[..4].iter().collect();
            let suffix: String = chars[len - 4..].iter().collect();
            format!("{prefix}\u{2026}{suffix}")
        }
    }
}

/// Drops the query string of a URL, which is where some APIs put credentials.
pub fn redact_url(url: &str) -> String {
    url.split(['?', '#']).next().unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::{redact_secret, redact_url};

    #[test]
    fn long_secrets_keep_only_the_edges() {
        assert_eq!(redact_secret("sk-or-v1-0123456789abcdef"), "sk-o\u{2026}cdef");
    }

    #[test]
    fn short_secrets_are_fully_masked() {
        assert_eq!(redact_secret("abc"), "***");
        assert_eq!(redact_secret("123456789012"), "***");
    }

    #[test]
    fn empty_secret_is_marked() {
        assert_eq!(redact_secret(""), "<empty>");
    }

    #[test]
    fn url_query_is_stripped() {
        assert_eq!(
            redact_url("https://generativelanguage.googleapis.com/v1beta/models/x:generateContent?key=secret"),
            "https://generativelanguage.googleapis.com/v1beta/models/x:generateContent"
        );
    }
}
