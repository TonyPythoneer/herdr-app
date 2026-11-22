// Runs as an EXTERNAL macOS process. It cannot call AppWindow::browser_decide,
// set an approved flag, use the Herdr control socket, or forge a browser result.
// Every action goes through the same native GUI event route as an actual user.
// A failed/missing approval never counts as a pass in the Python E2E harness.
import AppKit
import ApplicationServices
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 3,
      let rawPID = Int32(CommandLine.arguments[1]),
      ["approve", "deny"].contains(CommandLine.arguments[2]) else {
    fputs("usage: swift driver.swift <app-pid> <approve|deny>\n", stderr)
    exit(2)
}
let pid = pid_t(rawPID)
let requested = CommandLine.arguments[2] == "approve" ? "Approve once" : "Deny"

func asCFString(_ value: String) -> CFString {
    value.withCString { CFStringCreateWithCString(nil, $0, kCFStringEncodingUTF8)! }
}

func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var result: CFTypeRef?
    return AXUIElementCopyAttributeValue(element, asCFString(name), &result) == .success ? result : nil
}

func textOf(_ element: AXUIElement) -> String {
    [kAXTitleAttribute, kAXDescriptionAttribute, kAXValueAttribute].compactMap {
        attribute(element, $0) as? String
    }.joined(separator: " ")
}

func locate(_ root: AXUIElement, _ label: String) -> AXUIElement? {
    var pending = [root]
    var seen = 0
    while !pending.isEmpty && seen < 800 {
        let element = pending.removeFirst()
        seen += 1
        let text = textOf(element)
        if text.localizedCaseInsensitiveContains(label) && text.count < label.count + 50 {
            return element
        }
        if let children = attribute(element, kAXChildrenAttribute) as? [AXUIElement] {
            pending.append(contentsOf: children)
        }
    }
    return nil
}

func click(at point: CGPoint) -> Bool {
    guard let down = CGEvent(mouseEventSource: nil, mouseType: .leftMouseDown,
                             mouseCursorPosition: point, mouseButton: .left),
          let up = CGEvent(mouseEventSource: nil, mouseType: .leftMouseUp,
                           mouseCursorPosition: point, mouseButton: .left) else {
        return false
    }
    down.post(tap: .cghidEventTap)
    usleep(90_000)
    up.post(tap: .cghidEventTap)
    return true
}

// Prefer an actual accessibility button. The custom GPUI div's AXPress action,
// when exposed, exercises exactly the live on_click native GPUI callback.
let app = AXUIElementCreateApplication(pid)
if let element = locate(app, requested) {
    if AXUIElementPerformAction(element, asCFString(kAXPressAction)) == .success {
        print("GPUI_NATIVE_APPROVAL_INPUT: AXPress \(requested)")
        exit(0)
    }
    if let position = attribute(element, kAXPositionAttribute),
       let size = attribute(element, kAXSizeAttribute) {
        var origin = CGPoint.zero
        var dimensions = CGSize.zero
        if AXValueGetValue(position as! AXValue, .cgPoint, &origin),
           AXValueGetValue(size as! AXValue, .cgSize, &dimensions),
           click(at: CGPoint(x: origin.x + dimensions.width / 2,
                             y: origin.y + dimensions.height / 2)) {
            print("GPUI_NATIVE_APPROVAL_INPUT: Quartz click on AX element \(requested)")
            exit(0)
        }
    }
}

// GPUI may not expose a separate AX node for its div. Fall back to genuine
// Quartz mouse input at the button's location in the centered approval card.
// The following is NOT a success assertion; Python must see a resulting
// completed/denied request and verify the page's observable postcondition.
guard let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements],
                                                kCGNullWindowID) as? [[String: Any]] else {
    fputs("Cannot read macOS app window list\n", stderr)
    exit(1)
}
let ours = windows.filter {
    ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == rawPID &&
    ($0[kCGWindowLayer as String] as? NSNumber)?.intValue == 0
}
guard let window = ours.max(by: {
    let a = ($0[kCGWindowBounds as String] as? NSDictionary)?["Width"] as? NSNumber
    let b = ($1[kCGWindowBounds as String] as? NSDictionary)?["Width"] as? NSNumber
    return (a?.doubleValue ?? 0) < (b?.doubleValue ?? 0)
}), let bounds = window[kCGWindowBounds as String] as? NSDictionary,
   let bx = bounds["X"] as? NSNumber, let by = bounds["Y"] as? NSNumber,
   let bw = bounds["Width"] as? NSNumber, let bh = bounds["Height"] as? NSNumber else {
    fputs("Cannot locate the actual running Herdr window; no approval input sent\n", stderr)
    exit(1)
}
let centerX = bx.doubleValue + bw.doubleValue / 2
let centerY = by.doubleValue + bh.doubleValue / 2
// Card is 520px wide, centered; two buttons are left aligned below the text.
let point = CGPoint(x: centerX + (requested == "Approve once" ? -128 : -215),
                    y: centerY + 65)
guard click(at: point) else {
    fputs("Failed to send a real Quartz pointer event\n", stderr)
    exit(1)
}
print("GPUI_NATIVE_APPROVAL_INPUT: Quartz click \(requested) in packaged app PID \(pid)")
