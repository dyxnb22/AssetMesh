//! Text invariants shared by every module.
//!
//! Modules own their typed details (ADR 0003) and must not reach into each
//! other's domain types. Free-text handling is a kernel-level concern, so it
//! lives here: every module's optional text field is trimmed the same way and
//! control characters are rejected the same way.

use crate::{AppError, AppResult};

/// Trims an optional free-text field; `None`/whitespace-only collapse to
/// `None` so empty strings never become meaningful state.
pub fn optional_text(value: &Option<String>, field: &str) -> AppResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            if trimmed.chars().any(|c| c.is_control()) {
                return Err(AppError::validation(format!(
                    "{field} must not contain control characters"
                )));
            }
            Ok(Some(trimmed.to_string()))
        }
    }
}

/// Rejects a value longer than `max` bytes. Shared so bound checks read
/// identically across modules.
pub fn bounded(text: &str, max: usize, field: &str) -> AppResult<()> {
    if text.len() > max {
        return Err(AppError::validation(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(())
}

/// Requires a syntactically valid absolute http/https URL with no credentials.
///
/// Credential-bearing URLs must not enter canonical data (ADR 0010): userinfo
/// segments (`user:pass@host`), credential query parameters (`?api_key=...`),
/// and OAuth-style fragments (`#access_token=...`) are all rejected.
pub fn url_shape(raw_url: &str) -> AppResult<()> {
    if raw_url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::validation(
            "URL must not contain whitespace or control characters",
        ));
    }

    let parsed = url::Url::parse(raw_url)
        .map_err(|e| AppError::validation(format!("URL is not a valid absolute URL: {e}")))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::validation(
            "URL must be an absolute http:// or https:// URL",
        ));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::validation(
            "URL must not contain credentials; remove the user info segment",
        ));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| AppError::validation("URL must have a host"))?;
    if host.is_empty() {
        return Err(AppError::validation("URL must have a host"));
    }

    // Hostname label validation
    if let Some(url::Host::Domain(domain)) = parsed.host() {
        for label in domain.split('.') {
            if label.is_empty()
                || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                || label.starts_with('-')
                || label.ends_with('-')
            {
                return Err(AppError::validation(format!(
                    "URL host {domain:?} is not a valid host name"
                )));
            }
        }
    }

    // Query parameters: check percent-decoded parameter names
    for (name, _) in parsed.query_pairs() {
        if looks_like_credential_param(&name) {
            return Err(AppError::validation(format!(
                "URL must not contain credentials; remove the query parameter {name:?}"
            )));
        }
    }

    // Fragment: check OAuth-style parameters (#access_token=... or #/callback?access_token=...)
    if let Some(frag) = parsed.fragment() {
        if let Some((_, query_part)) = frag.split_once('?') {
            for (name, _) in url::form_urlencoded::parse(query_part.as_bytes()) {
                if looks_like_credential_param(&name) {
                    return Err(AppError::validation(format!(
                        "URL must not contain credentials; remove the fragment parameter {name:?}"
                    )));
                }
            }
        }
        if frag.contains('=') {
            for (name, _) in url::form_urlencoded::parse(frag.as_bytes()) {
                if looks_like_credential_param(&name) {
                    return Err(AppError::validation(format!(
                        "URL must not contain credentials; remove the fragment parameter {name:?}"
                    )));
                }
            }
        }
    }

    Ok(())
}

/// True for parameter names that conventionally carry a secret. Deliberately
/// a deny list, not an allow list: an unknown parameter is assumed innocent.
fn looks_like_credential_param(name: &str) -> bool {
    let normalized: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    const CREDENTIAL_PARAMS: [&str; 16] = [
        "apikey",
        "key",
        "accesstoken",
        "refreshtoken",
        "idtoken",
        "token",
        "secret",
        "clientsecret",
        "password",
        "passwd",
        "pwd",
        "privatekey",
        "signature",
        "sig",
        "hmac",
        "authorization",
    ];
    CREDENTIAL_PARAMS.contains(&normalized.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_text_trims_and_rejects_controls() {
        assert_eq!(optional_text(&None, "f").unwrap(), None);
        assert_eq!(optional_text(&Some("".to_string()), "f").unwrap(), None);
        assert_eq!(optional_text(&Some("   ".to_string()), "f").unwrap(), None);
        assert_eq!(
            optional_text(&Some("  hello ".to_string()), "f").unwrap(),
            Some("hello".to_string())
        );
        assert!(optional_text(&Some("a\u{0000}b".to_string()), "f").is_err());
    }

    #[test]
    fn bounded_checks_length() {
        assert!(bounded("abc", 3, "f").is_ok());
        assert!(bounded("abcd", 3, "f").is_err());
    }

    #[test]
    fn url_shape_accepts_reasonable_urls() {
        assert!(url_shape("https://example.com").is_ok());
        assert!(url_shape("https://example.com/path/to/thing").is_ok());
        assert!(url_shape("https://example.com:8443/a?b=c&d=e").is_ok());
        assert!(url_shape("http://localhost:3000/").is_ok());
        assert!(url_shape("https://sub.domain.example.co.uk/a#frag").is_ok());
        assert!(url_shape("https://[::1]:8080/a").is_ok());
        assert!(url_shape("https://example.com/?page=2&q=hello").is_ok());
        assert!(url_shape("https://example.com/callback?code=xyz&state=1").is_ok());
        assert!(url_shape("https://example.com/?id=42").is_ok());
    }

    #[test]
    fn url_shape_rejects_malformed_urls() {
        assert!(url_shape("ftp://example.com/").is_err());
        assert!(url_shape("example.com").is_err());
        assert!(url_shape("https://").is_err());
        assert!(url_shape("https://exa mple.com/").is_err());
        assert!(url_shape("https://exa\nmple.com/").is_err());
        assert!(url_shape("https://-bad.com/").is_err());
        assert!(url_shape("https://example..com/").is_err());
        assert!(url_shape("https://example.com:notaport/").is_err());
        assert!(url_shape("https://[::g]/").is_err());
        assert!(url_shape("https://[1:::2/").is_err());
    }

    #[test]
    fn url_shape_rejects_userinfo_credentials() {
        assert!(url_shape("https://user:pass@example.com").is_err());
        assert!(url_shape("https://user:pass@example.com/").is_err());
        assert!(url_shape("https://token@example.com/").is_err());
    }

    #[test]
    fn url_shape_rejects_credentials_in_query_and_percent_encoded() {
        assert!(url_shape("https://example.com/refresh?api_key=abc123").is_err());
        assert!(url_shape("https://example.com/?API_KEY=abc123").is_err());
        assert!(url_shape("https://example.com/?api-key=abc123").is_err());
        assert!(url_shape("https://example.com/?a=b&access_token=x").is_err());
        assert!(url_shape("https://example.com/?secret=x#section").is_err());
        assert!(url_shape("https://example.com/?refresh_token=x").is_err());
        assert!(url_shape("https://example.com/?key=x").is_err());
        assert!(url_shape("https://example.com/?sig=x").is_err());
        // Percent-encoded credential parameter names must be decoded and rejected:
        assert!(url_shape("https://example.com/?api%5Fkey=x").is_err());
        assert!(url_shape("https://example.com/?%61ccess_token=x").is_err());
        assert!(url_shape("https://example.com/?%61pi_key=x").is_err());
    }

    #[test]
    fn url_shape_rejects_credentials_in_fragment() {
        assert!(url_shape("https://example.com/#access_token=x").is_err());
        assert!(url_shape("https://example.com/#/callback?access_token=x").is_err());
        assert!(url_shape("https://example.com/#api_key=secret123").is_err());
        assert!(url_shape("https://example.com/#id_token=jwt_xyz").is_err());
        assert!(url_shape("https://example.com/#token=abc&state=1").is_err());
    }
}
