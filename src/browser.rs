//! Thin host-facing export of the separately maintained browser Adapter.
//! Herdr Tab/Pane compatibility tests remain here; browser behavior is
//! implemented in TonyPythoneer/herdr-webview-adapter.
pub use herdr_webview_adapter::browser::{MOD_NAME,is_browser,normalize_url,url_from_props};
#[cfg(target_os="macos")]
pub use herdr_webview_adapter::browser::create;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Content, Node};
    use crate::model::Tab;
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
    fn browser_pane_roundtrips_in_upstream_tab_model() {
        let tab = Tab::with_pane("tab1", "Browser", Node::block(
            "pane1",
            Content::Mod { name: MOD_NAME.into(), props: json!({"url": "https://example.com"}) },
        ));
        let serialized = serde_json::to_value(&tab.root).unwrap();
        let restored: Node = serde_json::from_value(serialized).unwrap();
        let item = restored.find("pane1").unwrap();
        assert!(item.is_pane());
        match item.content().unwrap() {
            Content::Mod { name, props } => {
                assert!(is_browser(name));
                assert_eq!(url_from_props(props).unwrap(), "https://example.com/");
            }
            _ => panic!("browser pane became a different content type"),
        }
    }

    #[test]
    fn browser_is_reserved_not_a_user_mod() {
        assert!(is_browser(MOD_NAME));
        assert!(!is_browser("browser"));
        assert!(url_from_props(&json!({})).is_err());
    }
}
