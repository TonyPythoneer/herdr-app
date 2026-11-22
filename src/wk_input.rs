//! Native AppKit mouse events for the real embedded Wry WKWebView.
//! Synthetic DOM click() and dispatchEvent() are not substitutes for this.
//! This module is intentionally NOT wired into Agent commands until a real
//! macOS E2E verifies the website receives a trusted event.
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;
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

#[cfg(test)]
mod tests {
    use super::*;
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
