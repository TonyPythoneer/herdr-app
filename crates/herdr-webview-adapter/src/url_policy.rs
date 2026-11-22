//! Portable browser URL admission and legacy Herdr pane metadata helpers.
//! This module deliberately does not depend on GPUI, native WebKit or app state.

use serde_json::Value;

pub const MOD_NAME: &str = "__herdr_native_browser";

pub fn is_browser(name: &str) -> bool {
    name == MOD_NAME
}

/// Accept web navigation only. Never pass file://, javascript:, data:, or
/// credential-bearing URLs to a privileged WKWebView from a command.
pub fn normalize_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.len() > 4096 || raw.chars().any(char::is_control) {
        return Err("browser URL must be non-empty, at most 4096 characters, and contain no control characters".into());
    }
    let input = if raw.contains("://") || raw.starts_with("javascript:") || raw.starts_with("data:")
        || raw.starts_with("file:") || raw.starts_with("about:")
    {
        raw.to_owned()
    } else if raw == "localhost" || raw.starts_with("localhost:")
        || raw == "127.0.0.1" || raw.starts_with("127.0.0.1:")
        || raw == "[::1]" || raw.starts_with("[::1]:")
    {
        format!("http://{raw}")
    } else {
        format!("https://{raw}")
    };
    let url = url::Url::parse(&input).map_err(|e| format!("invalid browser URL: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("only http:// and https:// URLs with a host are supported".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("embedded URL credentials are not allowed".into());
    }
    Ok(url.to_string())
}

pub fn url_from_props(props: &Value) -> Result<String, String> {
    let raw = props.get("url").and_then(Value::as_str).ok_or("browser pane has no URL")?;
    normalize_url(raw)
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn accepts_web_urls_and_local_services() {
        assert_eq!(normalize_url(" example.com ").unwrap(), "https://example.com/");
        assert_eq!(normalize_url("http://localhost:3000").unwrap(), "http://localhost:3000/");
        assert_eq!(normalize_url("127.0.0.1:8080/path").unwrap(), "http://127.0.0.1:8080/path");
        assert_eq!(normalize_url("https://example.com/a?q=1").unwrap(), "https://example.com/a?q=1");
    }

    #[test]
    fn rejects_unsafe_schemes_credentials_and_controls() {
        for input in [
            "", "javascript:alert(1)", "data:text/html,hello", "file:///etc/passwd",
            "about:blank", "ftp://example.com", "https://user:pass@example.com",
            "https://example.com/\nHeader: injected",
        ] {
            assert!(normalize_url(input).is_err(), "should reject {input:?}");
        }
    }

    #[test]
    fn browser_is_reserved_not_a_user_mod() {
        assert!(is_browser(MOD_NAME));
        assert!(!is_browser("browser"));
        assert!(url_from_props(&json!({})).is_err());
        assert!(url_from_props(&json!({"url": 123})).is_err());
        assert_eq!(url_from_props(&json!({"url": "example.com"})).unwrap(), "https://example.com/");
    }
}
