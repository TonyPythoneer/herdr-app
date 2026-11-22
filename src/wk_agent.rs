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
        "browser.click" => {
            let target=args.get("target").and_then(Value::as_object)
                .ok_or("INVALID_ARGUMENT: click target must be an object")?;
            let choices=["css","role","label","text","test_id","ref"];
            if choices.iter().filter(|choice| target.contains_key(**choice)).count()!=1
                || target.keys().any(|key| !choices.contains(&key.as_str())
                    && !["name","exact","snapshot_id"].contains(&key.as_str()))
            {
                return Err("INVALID_ARGUMENT: click requires exactly one supported locator".into());
            }
            for (key,value) in target {
                if key=="exact" {
                    if !value.is_boolean(){return Err("INVALID_ARGUMENT: exact must be boolean".into())}
                } else if value.as_str().is_none_or(|s| s.is_empty()||s.len()>512) {
                    return Err(format!("INVALID_ARGUMENT: invalid click target.{key}"));
                }
            }
            let expected=args.get("document_id").unwrap_or(&Value::Null);
            if !expected.is_null() && expected.as_str().is_none_or(|s| s.len()>2048){
                return Err("INVALID_ARGUMENT: document_id must be a string".into());
            }
            let expected_json=expected.to_string();
            format!(r#"
const current=String(performance.timeOrigin)+'|'+location.href;
const expected={expected_json};
if(expected!==null&&expected!==current)return {{ok:false,error:'STALE_DOCUMENT: target document changed'}};
const els=select();
if(els.length!==1)return {{ok:false,error:els.length===0?'LOCATOR_NOT_FOUND':'LOCATOR_AMBIGUOUS'}};
const el=els[0];
if(!visible(el)||el.disabled||el.getAttribute('aria-disabled')==='true')
 return {{ok:false,error:'ELEMENT_NOT_ACTIONABLE'}};
el.scrollIntoView({{block:'center',inline:'center',behavior:'instant'}});
const box=el.getBoundingClientRect();
const x=box.x+box.width/2,y=box.y+box.height/2;
const top=document.elementFromPoint(x,y);
if(!top||(top!==el&&!el.contains(top)))return {{ok:false,error:'ELEMENT_OBSCURED'}};
const label=name(el).toLowerCase().slice(0,100);
const dangerous=/pay|buy|purchase|checkout|send|delete|remove|transfer|publish|confirm|order|submit|unsubscribe|account|save|security/i.test(label)
    ||(el.tagName==='BUTTON'&&el.type==='submit'&&!el.closest('form[role=search]'));
const known=(el.tagName==='A'&&/^https?:/.test(el.href))||
 /^(search|next|back|previous|open|close|show|hide|expand|collapse|filter|play|pause)$/i.test(label);
if(!{approved}&&(dangerous||!known))
 return {{ok:false,error:'APPROVAL_REQUIRED: native click on uncertain or consequential site control '+label}};
return {{ok:true,result:{{x,y,native:true,document_id:current,label}}}};
"#)
        }
        "browser.click_at" => {
            if !approved {
                return Err("APPROVAL_REQUIRED: native coordinate click may activate an unknown site control".into());
            }
            let x=args.get("x").and_then(Value::as_f64)
                .ok_or("INVALID_ARGUMENT: click_at x must be numeric")?;
            let y=args.get("y").and_then(Value::as_f64)
                .ok_or("INVALID_ARGUMENT: click_at y must be numeric")?;
            if !x.is_finite()||!y.is_finite()||x<0.0||y<0.0||x>8192.0||y>8192.0 {
                return Err("INVALID_ARGUMENT: click_at is outside supported viewport bounds".into());
            }
            format!("return {{ok:true,result:{{x:{x},y:{y},native:true}}}};")
        }
        "browser.wait" => {
            let condition=args.get("condition").and_then(Value::as_object)
                .ok_or("INVALID_ARGUMENT: wait condition must be an object")?;
            if condition.is_empty() || condition.len()>4
                || condition.keys().any(|key| !["url","selector","state","text"].contains(&key.as_str())) {
                return Err("INVALID_ARGUMENT: unsupported or empty wait condition".into());
            }
            for (key, value) in condition {
                let text=value.as_str().ok_or("INVALID_ARGUMENT: wait fields must be strings")?;
                let limit=if key=="url"{2048}else{512};
                if text.is_empty()||text.len()>limit {
                    return Err(format!("INVALID_ARGUMENT: wait condition.{key} is too long or empty"));
                }
            }
            if condition.contains_key("state") &&
                (!condition.contains_key("selector") ||
                 !["visible","hidden"].contains(&condition["state"].as_str().unwrap_or("")))
            {
                return Err("INVALID_ARGUMENT: state requires selector and visible|hidden".into());
            }
            if !condition.contains_key("url") && !condition.contains_key("selector") && !condition.contains_key("text") {
                return Err("INVALID_ARGUMENT: wait needs a URL, selector or text condition".into());
            }
            if args.get("timeout_ms").is_some_and(|v| v.as_u64().is_none_or(|t| t==0 || t>30000)) {
                return Err("INVALID_ARGUMENT: timeout_ms must be 1..30000".into());
            }
            let cond_json=Value::Object(condition.clone()).to_string();
            format!(r#"
const c={cond_json};let matched=true;
if(c.url!==undefined)matched=matched&&location.href.includes(c.url);
if(c.selector!==undefined){{
 const el=document.querySelector(c.selector);
 const shown=!!el&&visible(el);
 matched=matched&&(c.state==='hidden'?!shown:shown);
}}
if(c.text!==undefined)matched=matched&&!!document.body&&document.body.innerText.includes(c.text);
return {{ok:true,result:{{matched:!!matched,url:location.href}}}};
"#)
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
    let target = if matches!(command, "browser.query" | "browser.inspect" | "browser.click") {
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
        if message.starts_with("APPROVAL_REQUIRED:") {
            return Err(message.to_owned());
        }
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
    fn wk_wait_supports_typed_conditions_without_injecting_js_literals() {
        let generated=script("browser.wait",&json!({
            "condition":{"selector":"#submit","state":"visible","text":"Done","url":"/next"},
            "timeout_ms":1500
        }),false).unwrap();
        assert!(generated.contains("document.querySelector(c.selector)"));
        assert!(generated.contains("matched:!!matched"));
        assert!(generated.contains("\"selector\":\"#submit\""));
        assert!(script("browser.wait",&json!({"condition":{"state":"hidden"}}),false).is_err());
        assert!(script("browser.wait",&json!({"condition":{"selector":"input","state":"bogus"}}),false).is_err());
        assert!(script("browser.wait",&json!({"condition":{"selector":"input"},"timeout_ms":30001}),false).is_err());
        assert!(script("browser.wait",&json!({"condition":{"selector":"input","exec":"alert(1)"}}),false).is_err());
    }
    #[test]
    fn wk_native_clicks_require_locators_and_native_approval() {
        let target=json!({"target":{"css":"button"}});
        let source=script("browser.click",&target,false).unwrap();
        assert!(source.contains("elementFromPoint"));
        assert!(source.contains("APPROVAL_REQUIRED: native click"));
        assert!(source.contains("STALE_DOCUMENT"));
        assert!(script("browser.click",&json!({"target":{"css":"button","label":"x"}}),false).is_err());
        assert!(script("browser.click",&json!({"target":{"css":"button"},"document_id":17}),false).is_err());
        assert!(script("browser.click_at",&json!({"x":20.0,"y":30.0}),false)
            .unwrap_err().starts_with("APPROVAL_REQUIRED:"));
        assert!(script("browser.click_at",&json!({"x":20.0,"y":30.0}),true)
            .unwrap().contains("native:true"));
        assert!(script("browser.click_at",&json!({"x":-1,"y":30}),true).is_err());
        assert!(parse_callback(r#"{"ok":false,"error":"APPROVAL_REQUIRED: confirm deletion"}"#)
            .unwrap_err().starts_with("APPROVAL_REQUIRED:"));
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
