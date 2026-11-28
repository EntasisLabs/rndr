// macOS diagnostic only. Owns exactly one native-artifact-host child; never
// attaches to the principal's app. Requires Accessibility + Screen Recording.
// Build: swiftc examples/native-artifact-host/wheel_probe.swift -o /tmp/wheel-probe
// Usage: wheel-probe BINARY HTML OUTPUT line|pixel|rapid-line|rapid-pixel|extreme-line|extreme-pixel|manual|replay [TRACE]
// Default client is 800x600. Set RNDR_PROBE_WINDOW=1200,850 for an AX window size.
// manual: ten seconds of explicitly requested hardware input in the owned window.
// replay: preserves recorded wheel fields/timing; current own-window destination.
import AppKit
import ScreenCaptureKit
import CoreMedia
import CoreVideo

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
func require(_ value: @autoclosure () -> Bool, _ message: String) {
    if !value() { fputs("\(message)\n", stderr); exit(1) }
}
require(CommandLine.arguments.count >= 5, "usage: wheel-probe BINARY HTML OUTPUT MODE [TRACE]")
let binary = CommandLine.arguments[1], html = CommandLine.arguments[2]
let output = URL(fileURLWithPath: CommandLine.arguments[3], isDirectory: true)
let mode = CommandLine.arguments[4]
require(["line", "pixel", "rapid-line", "rapid-pixel", "extreme-line", "extreme-pixel", "manual", "replay", "replay-native"].contains(mode), "invalid mode")
require(!FileManager.default.fileExists(atPath: output.path), "output already exists; choose a fresh directory")
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
let log = output.appendingPathComponent("host.log")
require(FileManager.default.createFile(atPath: log.path, contents: nil), "create host log failed")
let logHandle = try FileHandle(forWritingTo: log)
let child = Process()
child.executableURL = URL(fileURLWithPath: binary)
child.arguments = ["--html", html]
child.standardOutput = logHandle; child.standardError = logHandle
try child.run()
// A crash, permission error, or Ctrl-C must not leave our test window running.
atexit { if child.isRunning { child.terminate() } }
signal(SIGINT, SIG_IGN)
let interrupt = DispatchSource.makeSignalSource(signal: SIGINT, queue: .main)
interrupt.setEventHandler { exit(130) }; interrupt.resume()

let fields: [CGEventField] = [
    .scrollWheelEventDeltaAxis1, .scrollWheelEventDeltaAxis2, .scrollWheelEventDeltaAxis3,
    .scrollWheelEventFixedPtDeltaAxis1, .scrollWheelEventFixedPtDeltaAxis2, .scrollWheelEventFixedPtDeltaAxis3,
    .scrollWheelEventPointDeltaAxis1, .scrollWheelEventPointDeltaAxis2, .scrollWheelEventPointDeltaAxis3,
    .scrollWheelEventIsContinuous, .scrollWheelEventScrollPhase, .scrollWheelEventMomentumPhase,
    .scrollWheelEventScrollCount
]
struct Wheel: Codable {
    var seconds: Double
    var values: [Int64]
}
var wheels: [Wheel] = []
var injected: [Wheel] = []
var traceStart = ProcessInfo.processInfo.systemUptime
// Process-specific, passive, wheel-only tap: no keys, content, other windows,
// interception, or change to normal AppKit delivery. Bounded to this child/run.
let tap = CGEvent.tapCreateForPid(pid: child.processIdentifier, place: .tailAppendEventTap,
    options: .listenOnly, eventsOfInterest: CGEventMask(1) << CGEventType.scrollWheel.rawValue,
    callback: { _, type, event, _ in
        if type == .scrollWheel && wheels.count < 10000 {
            wheels.append(Wheel(seconds: ProcessInfo.processInfo.systemUptime - traceStart,
                                values: fields.map { event.getIntegerValueField($0) }))
        }
        return Unmanaged.passUnretained(event)
    }, userInfo: nil)
if let tap = tap {
    let source = CFMachPortCreateRunLoopSource(nil, tap, 0)!
    CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
} else {
    fputs("Process-scoped wheel trace unavailable. Automatic captures may proceed, but delivered-event telemetry is unverified.\n", stderr)
    // Manual mode still captures video; trace.json will be empty and cannot replay.
}

final class Frames: NSObject, SCStreamOutput {
    var timestamps: [Double] = []
    var failure: Error?
    let context = CIContext()
    func stream(_ stream: SCStream, didOutputSampleBuffer sample: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, let buffer = sample.imageBuffer, timestamps.count < 2400 else { return }
        guard let attachments = CMSampleBufferGetSampleAttachmentsArray(sample, createIfNecessary: false) as? [[SCStreamFrameInfo: Any]],
              let rawStatus = attachments.first?[.status] as? Int,
              SCFrameStatus(rawValue: rawStatus) == .complete else { return }
        let image = CIImage(cvPixelBuffer: buffer)
        guard let cg = context.createCGImage(image, from: image.extent) else { return }
        let rep = NSBitmapImageRep(cgImage: cg)
        do {
            guard let png = rep.representation(using: .png, properties: [:]) else { return }
            try png.write(to: output.appendingPathComponent(String(format: "frame-%05d.png", timestamps.count)))
            timestamps.append(CMSampleBufferGetPresentationTimeStamp(sample).seconds)
        } catch { failure = error }
    }
}
Task { @MainActor in
    do {
        for _ in 0..<200 {
            if (try? String(contentsOf: log, encoding: .utf8).contains("host artifact: Ready")) == true { break }
            require(child.isRunning, "child exited before Ready")
            try await Task.sleep(for: .milliseconds(50))
        }
        let readyLog = try String(contentsOf: log, encoding: .utf8)
        require(readyLog.contains("host artifact: Ready"), "Ready timed out")
        NSRunningApplication(processIdentifier: child.processIdentifier)?.activate(options: [])
        if let size = ProcessInfo.processInfo.environment["RNDR_PROBE_WINDOW"] {
            let dims = size.split(separator: ",").compactMap { Int($0) }
            require(dims.count == 2 && dims.allSatisfy { (200...4000).contains($0) }, "invalid window size")
            let resize = Process(); resize.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
            resize.arguments = ["-e", "tell application \"System Events\" to tell (first process whose unix id is \(child.processIdentifier)) to set size of window 1 to {\(dims[0]), \(dims[1])}"]
            try resize.run(); resize.waitUntilExit()
            require(resize.terminationStatus == 0, "AX resize failed")
            try await Task.sleep(for: .milliseconds(200))
        }
        let content = try await SCShareableContent.excludingDesktopWindows(true, onScreenWindowsOnly: true)
        guard let window = content.windows.first(where: { $0.owningApplication?.processID == child.processIdentifier && $0.windowLayer == 0 }) else {
            throw NSError(domain: "wheel-probe: no owned window", code: 1)
        }
        let config = SCStreamConfiguration()
        config.width = Int(window.frame.width); config.height = Int(window.frame.height)
        config.minimumFrameInterval = CMTime(value: 1, timescale: 120)
        config.queueDepth = 8; config.showsCursor = false
        let frames = Frames(), queue = DispatchQueue(label: "wheel-capture")
        let filter: SCContentFilter
        if ProcessInfo.processInfo.environment["RNDR_PROBE_DISPLAY_CROP"] == "1" {
            guard let display = content.displays.first(where: { $0.frame.contains(window.frame) }) else {
                throw NSError(domain: "wheel-probe: owned window must fit one display", code: 2)
            }
            // Capture ONLY the owned foreground window's screen rectangle, not
            // the rest of the display. Corroborates remote-layer window captures.
            config.sourceRect = window.frame.offsetBy(dx: -display.frame.minX, dy: -display.frame.minY)
            filter = SCContentFilter(display: display, excludingWindows: [])
        } else {
            filter = SCContentFilter(desktopIndependentWindow: window)
        }
        let stream = SCStream(filter: filter, configuration: config, delegate: nil)
        try stream.addStreamOutput(frames, type: .screen, sampleHandlerQueue: queue)
        try await stream.startCapture()
        let streamStarted = ProcessInfo.processInfo.systemUptime
        let point = CGPoint(x: window.frame.midX, y: window.frame.midY)
        CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: point, mouseButton: .left)?.post(tap: .cghidEventTap)
        // Navigation Ready is not paint. Record a rendered quiet baseline before motion.
        let quietMilliseconds = Double(ProcessInfo.processInfo.environment["RNDR_PROBE_QUIET_MS"] ?? "2000") ?? 2000
        require(quietMilliseconds.isFinite && quietMilliseconds >= 250 && quietMilliseconds <= 5000, "quiet baseline must be250..5000ms")
        try await Task.sleep(for: .milliseconds(quietMilliseconds))
        traceStart = ProcessInfo.processInfo.systemUptime
        if mode == "manual" {
            print("Scroll G503 rapidly in the OWNED test window now; capture stops in 10 seconds.")
            try await Task.sleep(for: .seconds(10))
        } else if mode == "replay-native" {
            require(CommandLine.arguments.count == 6, "replay-native requires artifact-PID.jsonl")
            let data = try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[5]))
            require(data.count <= 8 * 1024 * 1024, "native trace too large")
            let preview = Int(ProcessInfo.processInfo.environment["RNDR_PROBE_PREVIEW"] ?? "2") ?? 2
            let rows = try String(decoding: data, as: UTF8.self).split(separator: "\n").map {
                try JSONSerialization.jsonObject(with: Data($0.utf8)) as! [String: Any]
            }.filter { ($0["kind"] as? String) == "wheel" && ($0["preview"] as? Int) == preview }
            require(!rows.isEmpty && rows.count <= 10000, "no bounded native wheel records")
            let first = rows[0]["event_uptime"] as! Double
            var previous = 0.0
            for row in rows {
                let seconds = (row["event_uptime"] as! Double) - first
                let delta = row["delta"] as! [Double]
                require(seconds.isFinite && seconds >= previous && seconds <= 120 && delta.count == 2 && delta.allSatisfy { $0.isFinite && abs($0) < 32000 }, "invalid native wheel record")
                previous = seconds
                try await Task.sleep(for: .seconds(max(0, seconds - (ProcessInfo.processInfo.systemUptime - traceStart))))
                let precise = row["precise"] as! Bool
                let event = CGEvent(scrollWheelEvent2Source: nil, units: precise ? .pixel : .line, wheelCount: 2, wheel1: 0, wheel2: 0, wheel3: 0)!
                for (axis, value) in zip([CGEventField.scrollWheelEventDeltaAxis1, .scrollWheelEventDeltaAxis2], [delta[1], delta[0]]) { event.setIntegerValueField(axis, value: Int64(value)) }
                for (axis, value) in zip([CGEventField.scrollWheelEventPointDeltaAxis1, .scrollWheelEventPointDeltaAxis2], [delta[1], delta[0]]) { event.setIntegerValueField(axis, value: Int64((value * (precise ? 1 : 10)).rounded())) }
                // Set fractional fixed-point LAST: setting integer/point fields
                // can rewrite it. The double setter takes actual line units.
                for (axis, value) in zip([CGEventField.scrollWheelEventFixedPtDeltaAxis1, .scrollWheelEventFixedPtDeltaAxis2], [delta[1], delta[0]]) { event.setDoubleValueField(axis, value: value) }
                event.setIntegerValueField(.scrollWheelEventIsContinuous, value: precise ? 1 : 0)
                event.setIntegerValueField(.scrollWheelEventScrollPhase, value: Int64(row["phase"] as! Int))
                event.setIntegerValueField(.scrollWheelEventMomentumPhase, value: Int64(row["momentum"] as! Int))
                event.location = point
                injected.append(Wheel(seconds: ProcessInfo.processInfo.systemUptime - traceStart, values: fields.map { event.getIntegerValueField($0) }))
                event.post(tap: .cghidEventTap)
            }
        } else if mode == "replay" {
            require(CommandLine.arguments.count == 6, "replay requires trace.json")
            let events = try JSONDecoder().decode([Wheel].self, from: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[5])))
            require(!events.isEmpty && events.count <= 10000, "invalid replay size")
            var previous = 0.0
            for wheel in events {
                require(wheel.values.count == fields.count && wheel.seconds.isFinite && wheel.seconds >= previous && wheel.seconds <= 15, "invalid replay fields/time")
                try await Task.sleep(for: .seconds(max(0, wheel.seconds - (ProcessInfo.processInfo.systemUptime - traceStart))))
                previous = wheel.seconds
                let event = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 3, wheel1: 0, wheel2: 0, wheel3: 0)!
                for (field, value) in zip(fields, wheel.values) { event.setIntegerValueField(field, value: value) }
                event.location = point
                injected.append(Wheel(seconds: ProcessInfo.processInfo.systemUptime - traceStart,
                                      values: fields.map { event.getIntegerValueField($0) }))
                event.post(tap: .cghidEventTap)
            }
        } else {
            let line = mode.hasSuffix("line"), extreme = mode.hasPrefix("extreme")
            let rapid = mode.hasPrefix("rapid")
            for traversal in 0..<(rapid ? 30 : 6) {
                for _ in 0..<(extreme ? 1 : (rapid ? 8 : 12)) {
                    let delta: Int32 = extreme ? 30000 : (line ? (rapid ? 160 : 100) : 1600)
                    let event = CGEvent(scrollWheelEvent2Source: nil, units: line ? .line : .pixel,
                        wheelCount: 1, wheel1: traversal % 2 == 0 ? -delta : delta, wheel2: 0, wheel3: 0)!
                    event.location = point
                    event.setIntegerValueField(.scrollWheelEventIsContinuous, value: line ? 0 : 1)
                    event.setIntegerValueField(.scrollWheelEventScrollPhase, value: 0)
                    event.setIntegerValueField(.scrollWheelEventMomentumPhase, value: 0)
                    injected.append(Wheel(seconds: ProcessInfo.processInfo.systemUptime - traceStart,
                                          values: fields.map { event.getIntegerValueField($0) }))
                    event.post(tap: .cghidEventTap)
                    try await Task.sleep(for: .milliseconds(4))
                }
                if !rapid { try await Task.sleep(for: .milliseconds(500)) }
            }
        }
        let injectionEnd = ProcessInfo.processInfo.systemUptime
        try await Task.sleep(for: .milliseconds(500))
        try await stream.stopCapture()
        queue.sync {} // Drain outstanding frame writes before summary/exit.
        if let root = ProcessInfo.processInfo.environment["RNDR_ARTIFACT_DIAGNOSTICS"] {
            let directory = URL(fileURLWithPath: root)
            // Flush only this owned child's explicitly enabled diagnostics.
            if FileManager.default.fileExists(atPath: directory.appendingPathComponent("artifact-\(child.processIdentifier).jsonl").path) {
                try Data().write(to: directory.appendingPathComponent("diagnostic.stop"))
                try await Task.sleep(for: .seconds(0.8))
            }
        }
        if let error = frames.failure { throw error }
        try JSONEncoder().encode(wheels).write(to: output.appendingPathComponent("trace.json"))
        try JSONEncoder().encode(injected).write(to: output.appendingPathComponent("injected.json"))
        try JSONEncoder().encode(["stream_started":streamStarted,"injection_start":traceStart,"injection_end":injectionEnd]).write(to: output.appendingPathComponent("timeline.json"))
        try JSONEncoder().encode(frames.timestamps).write(to: output.appendingPathComponent("frame-times.json"))
        print("Captured \(frames.timestamps.count) delivered frames (requested120Hz, not guaranteed), \(wheels.count) delivered wheel events, size \(config.width)x\(config.height); \(output.path)")
        if tap != nil { require(!wheels.isEmpty, "no delivered wheel events: trace/input permissions or point failed") }
        child.terminate(); child.waitUntilExit()
        exit(0)
    } catch { fputs("wheel probe failed: \(error)\n", stderr); exit(1) }
}
dispatchMain()
