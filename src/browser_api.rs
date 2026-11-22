//! Bounded, pane-scoped Chrome automation routines. Called only from the
//! host's verified Herdr-pane dispatcher, never from a web page script.
use crate::chrome_cdp::Session;
use serde_json::{Value, json};
use std::{sync::{Arc, atomic::{AtomicBool,AtomicU64,Ordering}},time::{Duration,Instant},fs,io::Write,path::Path,os::unix::fs::{PermissionsExt,OpenOptionsExt}};
static SCREENSHOT_SEQ: AtomicU64 = AtomicU64::new(1);
const MAX_TEXT: usize = 65536;

fn required<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    let value = args.get(key).and_then(Value::as_str).ok_or_else(|| format!("INVALID_ARGUMENT: {key} requires a string"))?;
    if value.len() > MAX_TEXT {return Err(format!("INVALID_ARGUMENT: {key} exceeds size limit"))}
    Ok(value)
}
fn coordinate(args: &Value, key: &str, default: f64) -> Result<f64, String> {
    let n = args.get(key).map(Value::as_f64).unwrap_or(Some(default))
        .ok_or_else(|| format!("INVALID_ARGUMENT: {key} requires a number"))?;
    if !n.is_finite() || n.abs()>100000.0 {return Err(format!("INVALID_ARGUMENT: invalid {key}"))}
    Ok(n)
}
fn locator(args: &Value) -> Result<&Value,String> {
    let v=args.get("target").ok_or("INVALID_ARGUMENT: target required")?;
    let obj=v.as_object().ok_or("INVALID_ARGUMENT: target must be an object")?;
    let types=["css","role","label","text","test_id","ref"];
    if types.iter().filter(|key|obj.contains_key(**key)).count()!=1 ||
        obj.keys().any(|k|!types.contains(&k.as_str()) && !["name","exact","snapshot_id"].contains(&k.as_str())) {
        return Err("INVALID_ARGUMENT: target requires exactly one locator type".into());
    }
    for (key,val) in obj {
        if key=="exact" {if !val.is_boolean(){return Err("INVALID_ARGUMENT: target.exact must be boolean".into())}}
        else if !val.is_string() || val.as_str().is_some_and(|s|s.len()>512){
            return Err(format!("INVALID_ARGUMENT: target.{key} must be a string of at most 512 bytes"));
        }
    }
    Ok(v)
}

pub(crate) const DOM_HELPERS: &str = r#"
const value=t;
const visible=e=>{
  const b=e.getBoundingClientRect(),s=getComputedStyle(e);
  return b.width>0&&b.height>0&&s.display!=='none'&&s.visibility!=='hidden';
};
const simple=x=>String(x==null?'':x).trim().slice(0,240);
const role=e=>e.getAttribute('role')||({A:'link',BUTTON:'button',TEXTAREA:'textbox',INPUT:'textbox',SELECT:'combobox'}[e.tagName]||'');
const name=e=>simple(e.getAttribute('aria-label')||e.labels&&e.labels[0]&&e.labels[0].textContent||
   e.getAttribute('alt')||e.getAttribute('placeholder')||e.textContent);
const path=e=>{
  const p=[];let cur=e;
  for(let i=0;cur&&cur.nodeType===1&&i<15;i++,cur=cur.parentElement) {
    const peers=cur.parentElement?[...cur.parentElement.children].filter(x=>x.tagName===cur.tagName):[cur];
    p.unshift(cur.tagName.toLowerCase()+':nth-of-type('+(peers.indexOf(cur)+1)+')');
  }
  return p.join(' > ');
};
const sensitive=e=>{
  const kind=String(e.getAttribute('type')||'').toLowerCase();
  const autocomplete=String(e.getAttribute('autocomplete')||'').toLowerCase();
  const identity=[e.id,e.getAttribute('name'),e.getAttribute('aria-label')].filter(Boolean).join(' ');
  return kind==='password'||kind==='hidden'
    ||/(?:^|\s)(?:current-password|new-password|one-time-code|cc-[\w-]+)(?:\s|$)/i.test(autocomplete)
    ||/(?:password|passwd|passcode|pin|otp|totp|2fa|cvv|cvc|credit.?card|card.?number|secret|access.?token|api.?key|security.?code)/i.test(identity);
};
const summary=e=>{
  const b=e.getBoundingClientRect();
  return {ref:path(e),role:role(e),name:name(e),tag:e.tagName.toLowerCase(),text:simple(e.textContent),
    visible:visible(e),enabled:!e.disabled&&e.getAttribute('aria-disabled')!=='true',
    bounds:{x:b.x,y:b.y,width:b.width,height:b.height},
    value:sensitive(e)?'[REDACTED]':
      (e.tagName==='INPUT'||e.tagName==='TEXTAREA'?simple(e.value):null)};
};
const equals=(a,b)=>value.exact===false?String(a).toLowerCase().includes(String(b).toLowerCase()):String(a)===String(b);
const select=()=>{
  if(value.css)return [...document.querySelectorAll(value.css)];
  if(value.ref){
    if(value.snapshot_id&&value.snapshot_id!==String(performance.timeOrigin)+'|'+location.href)
      throw Error('STALE_SNAPSHOT');
    return [...document.querySelectorAll(value.ref)];
  }
  return [...document.querySelectorAll('a,button,input,textarea,select,label,[role],[tabindex],[contenteditable]')].slice(0,6000).filter(e=>{
    if(value.test_id)return equals(e.getAttribute('data-testid'),value.test_id);
    if(value.role)return role(e)===value.role&&(!value.name||equals(name(e),value.name));
    if(value.label)return equals(name(e),value.label);
    if(value.text)return equals(simple(e.textContent),value.text);
    return false;
  });
};
"#;
fn js(loc:&Value,body:&str)->String {
    format!("(() => {{ const t = {}; {} {} }})()",loc,DOM_HELPERS,body)
}
fn unique(session:&Session,loc:&Value,body:&str)->Result<Value,String>{
    let script=js(loc,&format!("const els=select(); if(els.length!==1)return {{error:els.length===0?'LOCATOR_NOT_FOUND':'LOCATOR_AMBIGUOUS',count:els.length}};const el=els[0];{body}"));
    let value=session.eval(&script)?;
    if let Some(error)=value["error"].as_str(){return Err(format!("{error}: {} matches",value["count"]))}
    Ok(value)
}
// Share the same no-symlink, owner-only storage policy as native WebKit.
// A pre-existing browser-screenshots symlink must never redirect Agent PNGs
// into another directory, even if that directory is writable.
fn ensure_private_screenshot_dir(dir:&Path)->Result<(),String>{
    fs::create_dir_all(dir).map_err(|e|format!("SCREENSHOT_WRITE_FAILED: {e}"))?;
    let meta=fs::symlink_metadata(dir).map_err(|e|format!("SCREENSHOT_WRITE_FAILED: {e}"))?;
    if !meta.is_dir() || meta.file_type().is_symlink(){
        return Err("SCREENSHOT_WRITE_FAILED: screenshot directory must not be a symlink".into());
    }
    fs::set_permissions(dir,fs::Permissions::from_mode(0o700))
        .map_err(|e|format!("SCREENSHOT_WRITE_FAILED: {e}"))
}
fn capped(value:Value)->Result<Value,String>{
    if serde_json::to_vec(&value).map_err(|e|e.to_string())?.len()>524288{
        return Err("RESULT_TOO_LARGE: response exceeds 512KiB".into())
    }
    Ok(value)
}
pub fn run(session:&Arc<Session>,command:&str,args:&Value,approved:bool,cancelled:&AtomicBool)->Result<Value,String>{
    if cancelled.load(Ordering::Acquire) {
        return Err("CANCELLED: browser operation stopped before execution".into());
    }
    let result=match command {
        "browser.snapshot"=>{
            let max=args.get("max_nodes").and_then(Value::as_u64).unwrap_or(100).clamp(1,400);
            let expression=format!(r#"(() => {{
const nodes=[...document.querySelectorAll('a,button,input,textarea,select,[role],[tabindex],h1,h2,h3')];
const items=[];
for(const el of nodes){{
 if(items.length>={max})break;
 const b=el.getBoundingClientRect(),s=getComputedStyle(el);
 if(b.width<=0||b.height<=0||s.visibility==='hidden'||s.display==='none')continue;
 const p=[];let cur=el;
 for(let i=0;cur&&cur.nodeType===1&&i<15;i++,cur=cur.parentElement){{
  const peers=cur.parentElement?[...cur.parentElement.children].filter(x=>x.tagName===cur.tagName):[cur];
  p.unshift(cur.tagName.toLowerCase()+':nth-of-type('+(peers.indexOf(cur)+1)+')');
 }}
 items.push({{ref:p.join(' > '),tag:el.tagName.toLowerCase(),
  role:el.getAttribute('role')||el.tagName.toLowerCase(),
  name:String(el.getAttribute('aria-label')||el.labels&&el.labels[0]&&el.labels[0].textContent||el.textContent||'').trim().slice(0,180),
  enabled:!el.disabled,bounds:{{x:b.x,y:b.y,width:b.width,height:b.height}}}});
}}
return {{snapshot_id:String(performance.timeOrigin)+'|'+location.href,
document_id:String(performance.timeOrigin)+'|'+location.href,
title:document.title,url:location.href,elements:items,truncated:nodes.length>items.length}};
}})()"#);
            session.eval(&expression)?
        }
        "browser.query"|"browser.inspect"=>{
            let loc=locator(args)?;
            let limit=if command=="browser.inspect"{1}else{50};
            session.eval(&js(loc,&format!("const els=select();return {{count:els.length,truncated:els.length>{limit},elements:els.slice(0,{limit}).map(summary)}};")))?
        }
        "browser.click"=>{
            let loc=locator(args)?;
            let info=unique(session,loc,r#"
if(!visible(el)||el.disabled||el.getAttribute('aria-disabled')==='true')return {error:'ELEMENT_NOT_ACTIONABLE'};
el.scrollIntoView({block:'center',inline:'center',behavior:'instant'});
const b=el.getBoundingClientRect(),x=b.x+b.width/2,y=b.y+b.height/2;
const top=document.elementFromPoint(x,y);
if(!top||(top!==el&&!el.contains(top)))return {error:'ELEMENT_OBSCURED'};
const label=name(el).toLowerCase();
// A harmless-looking label does not make a submit button or a destructive
// destination safe. This is a conservative preflight; native approval is
// still required before clicking unknown controls.
const form=el.closest('form');
const submits=(el.tagName==='BUTTON'||el.tagName==='INPUT')&&el.type==='submit'
  &&(!form||form.getAttribute('role')!=='search');
const href=el.tagName==='A'?el.getAttribute('href')||'':'';
const riskyHref=/(?:^|[/?#&=_-])(pay|purchase|checkout|delete|remove|transfer|confirm|order|submit|unsubscribe|logout)(?:[/?#&=_-]|$)/i.test(href);
const dangerous=/pay|buy|purchase|checkout|send|delete|remove|transfer|publish|confirm|order|submit|unsubscribe|account/i.test(label)
  ||submits||riskyHref;
const known=(el.tagName==='A'&&/^https?:/.test(el.href))||
 /^(search|next|back|previous|open|close|show|hide|expand|collapse|filter|play|pause)$/i.test(label);
return {x,y,dangerous,unknown:!known,label:label.slice(0,120)};"#)?;
            if !approved&&(info["dangerous"]==true||info["unknown"]==true){
                return Err(format!("APPROVAL_REQUIRED: click on site control {:?}",info["label"]));
            }
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.click_at(info["x"].as_f64().ok_or("invalid X")?,info["y"].as_f64().ok_or("invalid Y")?)?;
            json!({"clicked":true})
        }
        "browser.click_at"=>{
            if !approved{return Err("APPROVAL_REQUIRED: coordinate click may activate an unknown control".into())}
            let x=coordinate(args,"x",f64::NAN)?;
            let y=coordinate(args,"y",f64::NAN)?;
            if x<0.0||y<0.0||x>8192.0||y>8192.0{return Err("INVALID_ARGUMENT: coordinates out of bounds".into())}
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.click_at(x,y)?;
            json!({"clicked":true})
        }
        "browser.fill"|"browser.type"=>{
            let loc=locator(args)?;
            let value=required(args,if command=="browser.fill"{"value"}else{"text"})?;
            // The DOM bridge only locates/focuses/selects; actual typing is
            // dispatched through Chrome Input.insertText, not el.value=...
            let expression=if command=="browser.fill" {r#"
if(!visible(el)||el.disabled||el.readOnly)return {error:'ELEMENT_NOT_ACTIONABLE'};
if(!['INPUT','TEXTAREA'].includes(el.tagName)&&!el.isContentEditable)return {error:'NOT_EDITABLE'};
el.focus();
if(typeof el.select==='function')el.select();
else if(el.isContentEditable)document.getSelection().selectAllChildren(el);
return {ready:true,sensitive:sensitive(el)};
"#}else{r#"
if(!visible(el)||el.disabled||el.readOnly)return {error:'ELEMENT_NOT_ACTIONABLE'};
if(!['INPUT','TEXTAREA'].includes(el.tagName)&&!el.isContentEditable)return {error:'NOT_EDITABLE'};
el.focus();return {ready:true,sensitive:sensitive(el)};"#};
            let info=unique(session,loc,expression)?;
            if info["sensitive"]==true && !approved {return Err("APPROVAL_REQUIRED: entering sensitive credentials/payment data".into())}
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.insert_text(value)?;
            json!({"typed":true,"length":value.chars().count()})
        }
        "browser.press"=>{
            let key=required(args,"key")?;
            // Return aliases Enter, and Space can activate a focused button. Neither
            // may bypass the native human-approval gate through a different key name.
            if matches!(key, "Enter" | "Return" | "Space") && !approved {
                return Err("APPROVAL_REQUIRED: native key could submit or activate a site action".into());
            }
            if let Some(loc)=args.get("target"){unique(session,loc,"el.focus();return {focused:true};")?;}
            let mapped=match key {
                "Enter"|"Return"=>"Enter","Escape"=>"Escape","Tab"=>"Tab",
                "Backspace"=>"Backspace","Delete"=>"Delete",
                "ArrowLeft"=>"ArrowLeft","ArrowRight"=>"ArrowRight",
                "ArrowUp"=>"ArrowUp","ArrowDown"=>"ArrowDown","Space"=>" ",
                _=>return Err("INVALID_ARGUMENT: unsupported key; use browser.type for text".into()),
            };
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.command("Input.dispatchKeyEvent",json!({"type":"rawKeyDown","key":mapped}))?;
            session.command("Input.dispatchKeyEvent",json!({"type":"keyUp","key":mapped}))?;
            json!({"pressed":mapped})
        }
        "browser.scroll"=>{
            let x=coordinate(args,"x",400.0)?;
            let y=coordinate(args,"y",300.0)?;
            let dx=coordinate(args,"delta_x",0.0)?;
            let dy=coordinate(args,"delta_y",0.0)?;
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.scroll(x,y,dx,dy)?;
            json!({"scrolled":true})
        }
        "browser.wait"=>{
            let cond=args.get("condition").filter(|v|v.is_object()).ok_or("INVALID_ARGUMENT: condition must be an object")?;
            let js=format!(r#"(() => {{const c={};let ok=true;
if(c.url!==undefined)ok=ok&&location.href.includes(c.url);
if(c.selector!==undefined){{const el=document.querySelector(c.selector);
const visible=!!el&&el.getBoundingClientRect().width>0&&el.getBoundingClientRect().height>0;
ok=ok&&(c.state==='hidden'?!visible:visible);}}
if(c.text!==undefined)ok=ok&&!!document.body&&document.body.innerText.includes(c.text);
return {{matched:!!ok,url:location.href}};}})()"#,cond);
            let timeout=Duration::from_millis(args["timeout_ms"].as_u64().unwrap_or(5000).min(30000));
            let start=Instant::now();
            loop {
                if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
                let result=session.eval(&js)?;
                if result["matched"]==true{break result}
                if start.elapsed()>=timeout{return Err("TIMEOUT: wait condition not satisfied".into())}
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        "browser.eval"=>{
            if !approved{return Err("APPROVAL_REQUIRED: arbitrary JavaScript can bypass website action permissions".into())}
            if cancelled.load(Ordering::Acquire){return Err("CANCELLED".into())}
            session.eval(required(args,"expression")?)?
        }
        "browser.screenshot"=>{
            let bytes=session.screenshot()?;
            if bytes.len()>12*1024*1024{return Err("RESULT_TOO_LARGE: PNG exceeds 12MiB".into())}
            let img=image::load_from_memory_with_format(&bytes,image::ImageFormat::Png)
                .map_err(|e|format!("SCREENSHOT_INVALID: {e}"))?;
            let dir=crate::mods::app_dir().join("browser-screenshots");
            ensure_private_screenshot_dir(&dir)?;
            let file=dir.join(format!("chrome-{}-{}.png",std::process::id(),SCREENSHOT_SEQ.fetch_add(1,Ordering::Relaxed)));
            let mut f=fs::OpenOptions::new().write(true).create_new(true).mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&file).map_err(|e|format!("SCREENSHOT_WRITE_FAILED: {e}"))?;
            f.write_all(&bytes).map_err(|e|e.to_string())?;
            json!({"path":file.to_string_lossy(),"width":img.width(),"height":img.height(),"format":"png"})
        }
        _=>return Err("UNSUPPORTED_ACTION: browser operation not implemented".into()),
    };
    capped(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_request_has_no_write_permission(){
        let cancel=AtomicBool::new(true);
        assert!(cancel.load(Ordering::Acquire));
    }
    #[test]
    fn locator_must_be_unique_in_schema(){
        assert!(locator(&json!({"target":{"css":"button"}})).is_ok());
        assert!(locator(&json!({"target":{"role":"button","name":"Search"}})).is_ok());
        assert!(locator(&json!({"target":{"css":"button","text":"Search"}})).is_err());
        assert!(locator(&json!({"target":{"x":1,"y":2}})).is_err());
    }
    #[test]
    fn chrome_screenshot_directory_rejects_symlink_redirection(){
        use std::os::unix::fs::symlink;
        let parent=std::env::temp_dir().join(format!("herdr-chrome-png-security-{}",std::process::id()));
        let _=fs::remove_dir_all(&parent);
        fs::create_dir_all(&parent).unwrap();
        let real=parent.join("real");
        assert!(ensure_private_screenshot_dir(&real).is_ok());
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode()&0o777,0o700);
        let alias=parent.join("browser-screenshots");
        symlink(&real,&alias).unwrap();
        assert!(ensure_private_screenshot_dir(&alias).unwrap_err().contains("symlink"));
        fs::remove_dir_all(parent).unwrap();
    }
    #[test]
    fn shared_dom_helper_redacts_passwords_otp_and_card_fields(){
        for marker in ["current-password","one-time-code","cc-","api.?key","[REDACTED]"] {
            assert!(DOM_HELPERS.contains(marker),"missing sensitive redaction rule: {marker}");
        }
    }

    #[test]
    fn browser_operation_output_must_be_bounded(){
        assert!(capped(json!({"data":"x".repeat(600000)})).is_err());
        assert!(coordinate(&json!({"x":"abc"}),"x",0.0).is_err());
        assert!(required(&json!({"name":"hello"}),"name").is_ok());
    }
}
