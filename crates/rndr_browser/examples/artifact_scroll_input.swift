// Run after building artifact_scroll_harness with its required features:
// swift crates/rndr_browser/examples/artifact_scroll_input.swift [binary] [capture-directory]
// Uses real AppKit-dispatched CGEvents, never JS scrollTo. Requires macOS Aqua
// and permission to synthesize input (and Screen Recording for optional captures).
import AppKit
import CoreGraphics
import Foundation

func require(_ condition: @autoclosure () -> Bool, _ message: String) {
    if !condition() { fatalError(message) }
}
let binary = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "target/debug/examples/artifact_scroll_harness"
let captures = CommandLine.arguments.count > 2 ? CommandLine.arguments[2] : nil
func run(_ mode: String) throws {
    let log = "/tmp/rndr-scroll-\(mode).log"
    FileManager.default.createFile(atPath: log, contents: nil)
    let file = try FileHandle(forWritingTo: URL(fileURLWithPath: log))
    let child = Process()
    child.executableURL = URL(fileURLWithPath: binary)
    child.arguments = mode == "plain" ? [] : ["--\(mode)-host-style"]
    child.standardOutput = file; child.standardError = file
    try child.run()
    defer { if child.isRunning { child.terminate(); child.waitUntilExit() }; try? file.close() }
    func output() -> String { (try? String(contentsOfFile: log, encoding: .utf8)) ?? "" }
    let deadline = Date().addingTimeInterval(20)
    while !output().contains("scroll probe: READY") {
        require(child.isRunning && Date() < deadline, "preview never became ready: \(output())")
        Thread.sleep(forTimeInterval: 0.05)
    }
    NSRunningApplication(processIdentifier: child.processIdentifier)?.activate(options: [])
    Thread.sleep(forTimeInterval: 0.3)
    let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as! [[String:Any]]
    let window = windows.first { ($0[kCGWindowOwnerPID as String] as? Int32) == child.processIdentifier && ($0[kCGWindowLayer as String] as? Int) == 0 }!
    let bounds = window[kCGWindowBounds as String] as! [String:Any]
    func wheel(_ x: Double, _ y: Double, _ pixel: Bool, _ horizontal: Bool = false, _ gesture: String? = nil) {
        let point = CGPoint(x: (bounds["X"] as! Double) + x, y: (bounds["Y"] as! Double) + y)
        CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)!.post(tap: .cghidEventTap)
        Thread.sleep(forTimeInterval: 0.15)
        for index in 0..<12 {
            let event = CGEvent(scrollWheelEvent2Source: nil, units: pixel ? .pixel : .line, wheelCount: 2, wheel1: horizontal ? 0 : -30, wheel2: horizontal ? -30 : 0, wheel3: 0)!
            event.location = point
            if let gesture = gesture {
                event.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
                if gesture == "finger" {
                    let phase: CGScrollPhase = index == 0 ? .began : (index == 11 ? .ended : .changed)
                    event.setIntegerValueField(.scrollWheelEventScrollPhase, value: Int64(phase.rawValue))
                } else {
                    // CoreGraphics momentum phases are begin=1, continue=2, end=3.
                    event.setIntegerValueField(.scrollWheelEventMomentumPhase, value: index == 0 ? 1 : (index == 11 ? 3 : 2))
                }
            }
            event.post(tap: .cghidEventTap)
            Thread.sleep(forTimeInterval: 0.04)
        }
        Thread.sleep(forTimeInterval: 0.4)
    }
    func root() -> (Int, Int) {
        let lines = output().components(separatedBy: "\n").filter { $0.hasPrefix("scroll probe: ROOT ") }
        guard let line = lines.last else { return (0, 0) }
        let fields = line.components(separatedBy: " ")
        return (Int(fields[3])!, Int(fields[4])!)
    }
    // The child occupies logical x=100,y=70,w=640,h=460 in an 800x600 client.
    // This point lies inside the guest's independently scrollable panel.
    wheel(170, 300, true)
    require(output().contains("scroll probe: NESTED "), "\(mode): nested pixel scrolling did not move")
    require(root().1 == 0, "\(mode): nested scrolling unexpectedly moved root")
    wheel(700, 480, true)
    let pixelY = root().1
    if mode == "legacy" {
        require(pixelY == 0, "legacy overflow:hidden should reproduce blocked root scrolling")
    } else {
        require(pixelY > 0, "\(mode): pixel wheel failed to scroll root")
    }
    require(!output().contains("HOST WHEEL"), "\(mode): guest wheel incorrectly reached host")
    wheel(700, 480, true, true)
    let pixelX = root().0
    require(mode == "legacy" ? pixelX == 0 : pixelX > 0, "\(mode): horizontal scroll result incorrect")
    wheel(700, 480, true, false, "finger")
    let fingerY = root().1
    require(mode == "legacy" ? fingerY == 0 : fingerY > pixelY, "\(mode): phased trackpad scroll result incorrect")
    wheel(700, 480, true, false, "momentum")
    let momentumY = root().1
    require(mode == "legacy" ? momentumY == 0 : momentumY > fingerY, "\(mode): momentum scroll result incorrect")
    require(!output().contains("HOST WHEEL"), "\(mode): guest gestures incorrectly reached host")
    wheel(700, 480, false)
    let lineY = root().1
    require(mode == "legacy" ? lineY == 0 : lineY > pixelY, "\(mode): line wheel result incorrect")
    // Native parent's sidebar-like strip must still receive ordinary wheel events.
    let beforeHost = root()
    wheel(40, 400, true)
    wheel(40, 400, true, false, "finger")
    wheel(40, 400, true, false, "momentum")
    require(output().contains("source: Finger") && output().contains("source: Momentum"), "gesture events did not retain native phases")
    require(output().contains("HOST WHEEL"), "\(mode): host-owned strip lost wheel input")
    require(root() == beforeHost, "\(mode): host wheel leaked into guest")
    if let directory = captures {
        try FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        let capture = Process(); capture.executableURL = URL(fileURLWithPath: "/usr/sbin/screencapture")
        capture.arguments = ["-x", "-l", String(window[kCGWindowNumber as String] as! Int), "\(directory)/native-scroll-\(mode).png"]
        try capture.run(); capture.waitUntilExit(); require(capture.terminationStatus == 0, "capture failed")
    }
    print("PASS \(mode): nested pixel, root pixel Y=\(pixelY) X=\(pixelX), finger Y=\(fingerY), momentum Y=\(momentumY), line Y=\(lineY), guest/host wheel separation; \(log)")
}
try run("plain")
try run("legacy")
try run("stripped-legacy")
