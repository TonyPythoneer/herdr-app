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

#[cfg(test)]
mod tests {
    use super::*;
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
