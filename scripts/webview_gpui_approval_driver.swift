// Runs as an EXTERNAL macOS process. It cannot call AppWindow::browser_decide,
// set an approved flag, use the Herdr control socket, or forge a browser result.
// Every action goes through the same native GUI event route as an actual user.
// A failed/missing approval never counts as a pass in the Python E2E harness.
import AppKit
import ApplicationServices
import CoreGraphics
import CoreFoundation
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
    value.withCString { CFStringCreateWithCString(nil, $0, CFStringBuiltInEncodings.UTF8.rawValue)! }
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
    // GPUI receives regular macOS HID events in its NSWindow event loop.
    // CGEvent.postToPid can enqueue a Quartz event without passing through
    // the active AppKit window's normal mouse routing; the previous harness
    // claimed a successful click while the real approval stayed pending.
    // The caller first verifies that the target app is the actual foreground
    // process, and AX/TCC authorization is already granted.
    let source = CGEventSource(stateID: .hidSystemState)
    guard let moved = CGEvent(mouseEventSource: source, mouseType: .mouseMoved,
                              mouseCursorPosition: point, mouseButton: .left),
          let down = CGEvent(mouseEventSource: source, mouseType: .leftMouseDown,
                             mouseCursorPosition: point, mouseButton: .left),
          let up = CGEvent(mouseEventSource: source, mouseType: .leftMouseUp,
                           mouseCursorPosition: point, mouseButton: .left) else {
        return false
    }
    moved.post(tap: .cghidEventTap)
    usleep(140_000)
    down.post(tap: .cghidEventTap)
    usleep(140_000)
    up.post(tap: .cghidEventTap)
    usleep(140_000)
    return true
}

// A background CI process may not have the foreground session when it runs.
// Bring the exact packaged Herdr app to the front before sending OS events.
let trusted = AXIsProcessTrusted()
let target = NSRunningApplication(processIdentifier: pid)
let activated = target?.activate(options: []) ?? false
usleep(350_000)
let frontmost = NSWorkspace.shared.frontmostApplication?.processIdentifier
print("GPUI_NATIVE_INPUT_DIAGNOSTIC: accessibility_trusted=\(trusted) activated=\(activated) frontmost=\(frontmost ?? -1) target=\(pid)")
guard trusted, activated, frontmost == pid else {
    fputs("No trusted foreground session for native GPUI input; refusing to click another app\n", stderr)
    exit(1)
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
let point = CGPoint(x: centerX + (requested == "Approve once" ? -102 : -214),
                    y: centerY + 65)
print("GPUI_NATIVE_INPUT_DIAGNOSTIC: window_origin=(\(bx),\(by)) size=(\(bw),\(bh)) click=(\(point.x),\(point.y)) label=\(requested)")
guard click(at: point) else {
    fputs("Failed to send a real Quartz pointer event\n", stderr)
    exit(1)
}
print("GPUI_NATIVE_APPROVAL_INPUT: Quartz HID click \(requested) in packaged app PID \(pid)")
