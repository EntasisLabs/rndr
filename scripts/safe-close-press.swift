// Press only the close button of the explicitly supplied synthetic probe PID.
import AppKit
import ApplicationServices
let pid = pid_t(CommandLine.arguments[1])!
guard AXIsProcessTrusted() else { fatalError("Accessibility permission missing") }
let app = AXUIElementCreateApplication(pid)
var windows: CFTypeRef?
precondition(AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &windows) == .success)
let list = windows as! [AXUIElement]
precondition(!list.isEmpty)
var button: CFTypeRef?
precondition(AXUIElementCopyAttributeValue(list[0], kAXCloseButtonAttribute as CFString, &button) == .success)
let status = AXUIElementPerformAction(button as! AXUIElement, kAXPressAction as CFString)
print("AX close-button pid=\(pid) status=\(status.rawValue)")
precondition(status == .success)
