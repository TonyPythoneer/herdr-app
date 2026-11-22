//! Native AppKit mouse events for the real embedded Wry WKWebView.
//! Synthetic DOM click() and dispatchEvent() are not substitutes for this.
//! This module is intentionally NOT wired into Agent commands until a real
//! macOS E2E verifies the website receives a trusted event.
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::{NSPoint, NSString};
use wry::WebViewExtMacOS;

#[derive(Clone, Copy, Debug)]
struct ViewPoint {
    x: f64,
    y: f64,
}

fn to_native_point(
    css_x: f64, css_y: f64, width: f64, height: f64, flipped: bool,
) -> Result<ViewPoint, String> {
    if !css_x.is_finite() || !css_y.is_finite()
        || !width.is_finite() || !height.is_finite()
        || width <= 0.0 || height <= 0.0
        || css_x < 0.0 || css_x >= width
        || css_y < 0.0 || css_y >= height
    {
        return Err("INVALID_ARGUMENT: native pointer is outside the WKWebView viewport".into());
    }
    Ok(ViewPoint {
        x: css_x,
        y: if flipped { css_y } else { height-css_y },
    })
}

/// Dispatch AppKit-native mouse down/up into the WKWebView's existing window.
/// A real WebKit click listener must independently verify isTrusted=true.
pub fn click_at(view: &gpui_wry::WebView, css_x: f64, css_y: f64) -> Result<(), String> {
    if !view.visible() {
        return Err("BROWSER_NOT_VISIBLE: native pointer requires the active browser pane".into());
    }
    let wk = view.raw().webview();
    let bounds = wk.bounds();
    let pt = to_native_point(
        css_x, css_y, bounds.size.width, bounds.size.height, wk.isFlipped(),
    )?;
    let window = wk.window().ok_or("BROWSER_NOT_READY: WKWebView is not attached to a native window")?;
    if !window.isKeyWindow() {
        return Err("BROWSER_NOT_FOCUSED: activate the Herdr window before native input".into());
    }
    let location = wk.convertPoint_toView(NSPoint::new(pt.x, pt.y), None);
    let number = window.windowNumber();
    let down = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseDown, location, NSEventModifierFlags::empty(),
        0.0, number, None, 0, 1, 1.0,
    ).ok_or("NATIVE_INPUT_FAILED: could not construct mouse-down")?;
    let up = NSEvent::mouseEventWithType_location_modifierFlags_timestamp_windowNumber_context_eventNumber_clickCount_pressure(
        NSEventType::LeftMouseUp, location, NSEventModifierFlags::empty(),
        0.0, number, None, 0, 1, 0.0,
    ).ok_or("NATIVE_INPUT_FAILED: could not construct mouse-up")?;
    window.sendEvent(&down);
    window.sendEvent(&up);
    Ok(())
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativeKey {
    code: u16,
    chars: &'static str,
}

/// Allowlisted native WebKit navigation keys. Arbitrary text is deliberately
/// NOT accepted here; it needs a separate tested native text-entry operation.
fn key_info(key: &str) -> Result<NativeKey, String> {
    let (code, chars) = match key {
        "Enter" | "Return" => (36, "\r"),
        "Escape" => (53, "\u{001b}"),
        "Tab" => (48, "\t"),
        "Backspace" => (51, "\u{0008}"),
        "Delete" => (117, "\u{007f}"),
        "Space" => (49, " "),
        "ArrowLeft" => (123, "\u{f702}"),
        "ArrowRight" => (124, "\u{f703}"),
        "ArrowDown" => (125, "\u{f701}"),
        "ArrowUp" => (126, "\u{f700}"),
        _ => return Err("INVALID_ARGUMENT: unsupported WKWebView native key".into()),
    };
    Ok(NativeKey { code, chars })
}

/// Send an actual AppKit keyDown/keyUp to the active WKWebView responder.
/// Caller must focus the correct field and get native approval for keys that
/// can submit an irreversible form before entering this function.
pub fn press_key(view: &gpui_wry::WebView, key: &str) -> Result<(), String> {
    let key = key_info(key)?;
    if !view.visible() {
        return Err("BROWSER_NOT_VISIBLE: native keyboard requires active WKWebView pane".into());
    }
    let wk = view.raw().webview();
    let window = wk.window().ok_or("BROWSER_NOT_READY: WKWebView has no native window")?;
    if !window.isKeyWindow() {
        return Err("BROWSER_NOT_FOCUSED: activate the Herdr window before native keyboard input".into());
    }
    let chars = NSString::from_str(key.chars);
    let loc = NSPoint::new(0.0, 0.0);
    let number = window.windowNumber();
    let down = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyDown, loc, NSEventModifierFlags::empty(), 0.0,
        number, None, &chars, &chars, false, key.code,
    ).ok_or("NATIVE_INPUT_FAILED: could not construct native keyDown")?;
    let up = NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
        NSEventType::KeyUp, loc, NSEventModifierFlags::empty(), 0.0,
        number, None, &chars, &chars, false, key.code,
    ).ok_or("NATIVE_INPUT_FAILED: could not construct native keyUp")?;
    window.sendEvent(&down);
    window.sendEvent(&up);
    Ok(())
}


fn ascii_keycode(character: char) -> Result<(u16, bool), String> {
    let lower=character.to_ascii_lowercase();
    let code=match lower {
        'a'=>0,'s'=>1,'d'=>2,'f'=>3,'h'=>4,'g'=>5,'z'=>6,
        'x'=>7,'c'=>8,'v'=>9,'b'=>11,'q'=>12,'w'=>13,'e'=>14,
        'r'=>15,'y'=>16,'t'=>17,
        '1'=>18,'2'=>19,'3'=>20,'4'=>21,'6'=>22,'5'=>23,
        '9'=>25,'7'=>26,'8'=>28,'0'=>29,
        'o'=>31,'u'=>32,'i'=>34,'p'=>35,'l'=>37,'j'=>38,
        'k'=>40,'n'=>45,'m'=>46,' '=>49,
        _=>return Err("UNSUPPORTED_TEXT: WK native typing currently supports ASCII letters, digits and spaces only".into()),
    };
    Ok((code,character.is_ascii_uppercase()))
}

/// Native AppKit text entry into the already focused WKWebView input element.
/// Strictly bounded to avoid hanging GPUI's main thread with huge events.
/// The page, not this API, decides whether the user's typing changes its value.
pub fn type_text(view: &gpui_wry::WebView, value: &str) -> Result<usize, String> {
    if value.is_empty() || value.chars().count() > 128 {
        return Err("INVALID_ARGUMENT: native WK text input must be 1..128 characters".into());
    }
    // Validate all characters before dispatching anything. Partial unsupported
    // input must not be accidentally submitted to a production page.
    let events = value.chars().map(|c| ascii_keycode(c).map(|(code,shift)|(c,code,shift)))
        .collect::<Result<Vec<_>,_>>()?;
    if !view.visible() { return Err("BROWSER_NOT_VISIBLE: native WK typing needs a visible pane".into()); }
    let window=view.raw().webview().window().ok_or("BROWSER_NOT_READY: no WK native window")?;
    if !window.isKeyWindow() { return Err("BROWSER_NOT_FOCUSED: WK typing requires active Herdr window".into()); }
    let number=window.windowNumber();
    let loc=NSPoint::new(0.0,0.0);
    for (c,code,shift) in &events {
        let character=c.to_string();
        let unmodified=c.to_ascii_lowercase().to_string();
        let chars=NSString::from_str(&character);
        let ignored=NSString::from_str(&unmodified);
        let flags=if *shift { NSEventModifierFlags::Shift } else { NSEventModifierFlags::empty() };
        let make = |event_type| NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            event_type,loc,flags,0.0,number,None,&chars,&ignored,false,*code
        ).ok_or("NATIVE_INPUT_FAILED: cannot construct text keyboard event");
        let down=make(NSEventType::KeyDown)?;
        let up=make(NSEventType::KeyUp)?;
        window.sendEvent(&down);
        window.sendEvent(&up);
    }
    Ok(events.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_ascii_text_mapping_is_strict_and_atomic() {
        assert_eq!(ascii_keycode('h').unwrap(),(4,false));
        assert_eq!(ascii_keycode('H').unwrap(),(4,true));
        assert_eq!(ascii_keycode('1').unwrap(),(18,false));
        for ch in ['\n','\t','中','!','\0'] {
            assert!(ascii_keycode(ch).is_err());
        }
    }
    #[test]
    fn only_known_native_webkit_keys_are_dispatched() {
        assert_eq!(key_info("Tab").unwrap(), NativeKey { code:48, chars:"\t" });
        assert_eq!(key_info("Enter").unwrap().code, 36);
        for invalid in ["a","Command+Q","Meta+A","","Shift+Enter"] {
            assert!(key_info(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn native_input_checks_viewport_and_flipped_coordinates() {
        let p = to_native_point(60.0, 20.0, 100.0, 100.0, false).unwrap();
        assert_eq!((p.x,p.y), (60.0,80.0));
        let p = to_native_point(60.0, 20.0, 100.0, 100.0, true).unwrap();
        assert_eq!((p.x,p.y), (60.0,20.0));
        for (x,y) in [(f64::NAN,0.0),(0.0,f64::INFINITY),(-1.0,0.0),(100.0,0.0),(0.0,100.0)] {
            assert!(to_native_point(x,y,100.0,100.0,false).is_err());
        }
    }
}
