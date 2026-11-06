// Read-only metadata for one owned smoke process. No screenshots or permission requests.
// Usage: swift scripts/desktop-window-smoke.swift PID
import CoreGraphics
import Foundation

guard CommandLine.arguments.count == 2, let pid = Int(CommandLine.arguments[1]) else {
    fputs("usage: desktop-window-smoke.swift PID\n", stderr)
    exit(2)
}
print("Screen capture already permitted: \(CGPreflightScreenCaptureAccess()) (not requested)")
let windows = CGWindowListCopyWindowInfo(.optionOnScreenOnly, kCGNullWindowID) as? [[String: Any]] ?? []
let owned = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid }
for window in owned {
    let bounds = window[kCGWindowBounds as String] ?? "unknown"
    print("Owned pid \(pid), on-screen bounds: \(bounds)")
}
guard !owned.isEmpty else {
    fputs("No owned on-screen window found; this is not visual signoff.\n", stderr)
    exit(1)
}
print("Native window metadata only. No pixel inspection, input, IME or accessibility acceptance.")
