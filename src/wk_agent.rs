//! WKWebView Agent observation and explicit page-context eval.
//! Only a native Wry callback can produce a completed result; a fire-and-
//! forget JavaScript call is never reported as success.
use serde_json::{json, Value};

const MAX_SCRIPT: usize = 65_536;
const MAX_RESULT: usize = 512 * 1024;

/// Build one bounded, pane-local JS request. Destructive/opaque page scripts
/// always require approval through Herdr's native GPUI action dialog.
pub fn script(command: &str, args: &Value, approved: bool) -> Result<String, String> {
    let body = match command {
        "browser.snapshot" => {
            let max = args.get("max_nodes").and_then(Value::as_u64).unwrap_or(100).clamp(1, 400);
            format!(r#"
const nodes=[...document.querySelectorAll('a,button,input,textarea,select,[role],[tabindex],h1,h2,h3')];
const elements=[];
for(const el of nodes){{ if(elements.length>={max})break;if(!visible(el))continue;elements.push(summary(el)); }}
return {{ok:true,result:{{snapshot_id:String(performance.timeOrigin)+'|'+location.href,
document_id:String(performance.timeOrigin)+'|'+location.href,title:document.title,
url:location.href,elements,truncated:nodes.length>elements.length}}}};
"#)
        }
        "browser.query" | "browser.inspect" => {
            let target = args.get("target").ok_or("INVALID_ARGUMENT: target required")?;
            let object = target.as_object().ok_or("INVALID_ARGUMENT: target must be an object")?;
            let choices = ["css","role","label","text","test_id","ref"];
            if choices.iter().filter(|choice| object.contains_key(**choice)).count() != 1
                || object.keys().any(|key| !choices.contains(&key.as_str()) && !["name","exact","snapshot_id"].contains(&key.as_str()))
            {
                return Err("INVALID_ARGUMENT: target requires exactly one locator type".into());
            }
            for (key, value) in object {
                if key == "exact" {
                    if !value.is_boolean() {return Err("INVALID_ARGUMENT: exact must be boolean".into());}
                } else if value.as_str().is_none_or(|s| s.len()>512) {
                    return Err(format!("INVALID_ARGUMENT: invalid target.{key}"));
                }
            }
            let limit = if command == "browser.inspect" {1} else {50};
            format!("const els=select();return {{ok:true,result:{{count:els.length,truncated:els.length>{limit},elements:els.slice(0,{limit}).map(summary)}}}};")
        }
        "browser.eval" => {
            if !approved {
                return Err("APPROVAL_REQUIRED: arbitrary JavaScript can bypass website action permissions".into());
            }
            let expression = args.get("expression").and_then(Value::as_str)
                .ok_or("INVALID_ARGUMENT: expression requires a string")?;
            if expression.len()>MAX_SCRIPT {
                return Err("INVALID_ARGUMENT: expression exceeds 64KiB".into());
            }
            // JSON-encoded string prevents the script from changing our wrapper.
            let serialized = serde_json::to_string(expression).map_err(|e| e.to_string())?;
            format!("return {{ok:true,result:(0,eval)({serialized})}};")
        }
        _ => return Err("UNSUPPORTED_ENGINE: WKWebView native input/screenshots are not yet implemented".into()),
    };
    let target = if matches!(command, "browser.query" | "browser.inspect") {
        args["target"].clone()
    } else { Value::Null };
    Ok(format!(r#"(() => {{
try {{
const t={target};
{helpers}
{body}
}} catch (e) {{
return {{ok:false,error:String(e&&e.message||e)}};
}}
}})()"#, target=target,helpers=crate::browser_api::DOM_HELPERS,body=body))
}

/// Wry serializes evaluate_script_with_callback results as JSON. Treat
/// missing or malformed callback payloads as errors, never success.
pub fn parse_callback(raw: &str) -> Result<Value, String> {
    if raw.len()>MAX_RESULT { return Err("RESULT_TOO_LARGE: WKWebView output exceeds 512KiB".into()); }
    let response: Value = serde_json::from_str(raw)
        .map_err(|e|format!("WK_CALLBACK_INVALID: {e}"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response.get("error").and_then(Value::as_str).unwrap_or("no result");
        return Err(format!("WK_EVALUATION_FAILED: {message}"));
    }
    let result = response.get("result").cloned().unwrap_or(Value::Null);
    if serde_json::to_vec(&result).map_err(|e| e.to_string())?.len()>MAX_RESULT {
        return Err("RESULT_TOO_LARGE: WKWebView result exceeds 512KiB".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wk_observation_commands_are_bounded_and_pane_local() {
        let snapshot=script("browser.snapshot", &json!({"max_nodes":4000}), false).unwrap();
        assert!(snapshot.contains("elements.length>=400"));
        let query=script("browser.query", &json!({"target":{"role":"button","name":"Search"}}),false).unwrap();
        assert!(query.contains("const t="));
        assert!(query.contains("els.slice(0,50)"));
        assert!(script("browser.query", &json!({"target":{"css":"button","text":"Search"}}),false).is_err());
        assert!(script("browser.query", &json!({"target":{"css":"button","x":12}}),false).is_err());
        assert!(script("browser.click", &json!({"target":{"css":"button"}}),true).is_err());
    }
    #[test]
    fn arbitrary_eval_requires_the_native_approval_flow() {
        let args=json!({"expression":"document.title"});
        assert!(script("browser.eval",&args,false).unwrap_err().starts_with("APPROVAL_REQUIRED:"));
        assert!(script("browser.eval",&args,true).unwrap().contains("(0,eval)"));
        assert!(script("browser.eval",&json!({"expression":"x".repeat(70000)}),true).is_err());
    }
    #[test]
    fn wk_callback_must_have_verified_json_and_bounded_result() {
        assert_eq!(parse_callback(r#"{"ok":true,"result":{"count":2}}"#).unwrap(),json!({"count":2}));
        assert!(parse_callback("null").is_err());
        assert!(parse_callback("undefined").is_err());
        assert!(parse_callback(r#"{"ok":false,"error":"bad selector"}"#).unwrap_err().contains("bad selector"));
        assert!(parse_callback(&format!(r#"{{"ok":true,"result":"{}"}}"#,"x".repeat(600000))).is_err());
    }
}
