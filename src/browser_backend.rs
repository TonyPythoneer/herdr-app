//! Host-only backend discovery and capability advertisement.
//! Pure engine selection and persistence policy come from the adapter crate.
use serde_json::{Value, json};
pub use herdr_webview_adapter::engine_policy::{API_VERSION, BrowserEngine, from_persisted, set_pane_url};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Availability {
    pub wkwebview: bool,
    pub chrome_cdp: bool,
}

impl Availability {
    /// Chrome is selectable when a supported local Chrome 136+ is installed.
    /// It is never bundled or downloaded. A launch failure is reported by the
    /// owning pane rather than silently switching to a different engine.
    pub fn current() -> Self {
        #[cfg(target_os = "macos")]
        let chrome_cdp = crate::chrome_cdp::discover().is_ok();
        #[cfg(not(target_os = "macos"))]
        let chrome_cdp = false;
        Self { wkwebview: cfg!(target_os = "macos"), chrome_cdp }
    }
}

/// Preserve the host-facing API while delegating the decision to the package.
pub fn resolve_new_tab(requested: Option<&str>, available: Availability) -> Result<BrowserEngine, String> {
    herdr_webview_adapter::engine_policy::resolve_new_tab(
        requested,
        herdr_webview_adapter::engine_policy::Availability {
            wkwebview: available.wkwebview,
            chrome_cdp: available.chrome_cdp,
        },
    )
}

/// Capability detection reflects shipped implementation, not desired scope.
/// Chrome becomes available only after its CDP rendering bridge is integrated.
pub fn capabilities() -> Value {
    let available = Availability::current();
    let mut engines = Vec::new();
    if available.wkwebview {
        engines.push("wkwebview");
    }
    if available.chrome_cdp {
        engines.push("chrome-cdp");
    }
    let default_engine = resolve_new_tab(None, available).ok().map(BrowserEngine::as_str);
    json!({
        "api_version": API_VERSION,
        "default_engine": default_engine,
        "available_engines": engines,
        "engines": {
            "wkwebview": {
                "available": available.wkwebview,
                "implemented": available.wkwebview,
                "actions": ["open", "navigate", "back", "forward", "reload"],
                "agent_automation": available.wkwebview,
                "agent_actions": ["snapshot", "query", "inspect", "click", "click_at", "fill", "type", "press", "scroll", "wait", "eval", "screenshot"],
                "limits": {"text_input": "1..128 ASCII letters, digits and spaces", "scroll": "bounded native CoreGraphics pixel wheel; visible focused pane only", "unicode_ime": "not yet implemented"},
                "notes": "WKWebView uses native click/key/text/wheel events; no universal parity until real shipped-app E2E gates pass"
            },
            "chrome-cdp": {
                "available": available.chrome_cdp,
                "implemented": cfg!(target_os = "macos"),
                "actions": ["open", "navigate", "back", "forward", "reload"],
                "agent_automation": available.chrome_cdp,
                "notes": "Chrome CDP Agent API and MCP implemented"
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: Availability = Availability { wkwebview: true, chrome_cdp: true };
    const WK_ONLY: Availability = Availability { wkwebview: true, chrome_cdp: false };

    #[test]
    fn automatic_new_tab_prefers_usable_local_chrome() {
        assert_eq!(resolve_new_tab(None, BOTH).unwrap(), BrowserEngine::ChromeCdp);
        assert_eq!(resolve_new_tab(Some("auto"), BOTH).unwrap(), BrowserEngine::ChromeCdp);
        assert_eq!(resolve_new_tab(None, WK_ONLY).unwrap(), BrowserEngine::WkWebView);
    }

    #[test]
    fn explicit_choice_never_silently_falls_back() {
        assert_eq!(resolve_new_tab(Some("wkwebview"), BOTH).unwrap(), BrowserEngine::WkWebView);
        assert!(resolve_new_tab(Some("chrome-cdp"), WK_ONLY).is_err());
        assert!(resolve_new_tab(Some("unknown"), BOTH).is_err());
        assert!(resolve_new_tab(None, Availability { wkwebview: false, chrome_cdp: false }).is_err());
    }

    #[test]
    fn old_panes_keep_webkit_regardless_of_new_default() {
        assert_eq!(from_persisted(&json!({"url":"https://example.com"})).unwrap(), BrowserEngine::WkWebView);
        assert_eq!(from_persisted(&Value::Null).unwrap(), BrowserEngine::WkWebView);
        assert_eq!(from_persisted(&json!({"engine":"chrome-cdp"})).unwrap(), BrowserEngine::ChromeCdp);
        assert!(from_persisted(&json!({"engine":"auto"})).is_err());
        assert!(from_persisted(&json!({"engine":42})).is_err());
    }

    #[test]
    fn navigation_preserves_persisted_engine_and_unknown_fields() {
        let mut props = json!({"url":"https://old.example", "engine":"chrome-cdp", "custom":"keep"});
        set_pane_url(&mut props, "https://new.example/");
        assert_eq!(props["url"], "https://new.example/");
        assert_eq!(props["engine"], "chrome-cdp");
        assert_eq!(props["custom"], "keep");
    }

    #[test]
    fn webkit_capabilities_match_implemented_native_agent_actions() {
        let caps = capabilities();
        let actions = caps["engines"]["wkwebview"]["agent_actions"].as_array().unwrap();
        for name in ["snapshot", "query", "inspect", "click", "click_at", "fill", "type", "press", "wait", "eval", "screenshot"] {
            assert!(actions.iter().any(|item| item.as_str() == Some(name)), "missing WK native action: {name}");
        }
        assert!(actions.iter().any(|item| item.as_str() == Some("scroll")),
            "WK native wheel must remain exposed after its shipped input integration");
        assert_eq!(caps["engines"]["wkwebview"]["limits"]["unicode_ime"], "not yet implemented");
    }

    #[test]
    fn chrome_capabilities_reflect_platform_and_installed_browser() {
        let caps = capabilities();
        assert_eq!(caps["api_version"], API_VERSION);
        assert_eq!(caps["engines"]["chrome-cdp"]["available"], Availability::current().chrome_cdp);
        assert_eq!(caps["engines"]["chrome-cdp"]["implemented"], cfg!(target_os = "macos"));
        assert_eq!(caps["engines"]["chrome-cdp"]["agent_automation"], Availability::current().chrome_cdp);
    }
}
