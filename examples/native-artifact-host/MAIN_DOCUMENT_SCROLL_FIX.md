# Main-document artifact scroll repair

## Finding and applied repair

The reported dark artifact flashes an entirely white **interior** during rapid
nonprecise line-wheel scrolling in WebKit's scrolling `sandbox/srcdoc` subframe
path. The same wrapper reproduces in a standalone AppKit WKWebView, without
RNDR/Metal, host repainting, popup compositing or production app lifecycle changes.
Ordinary main-document controls do not reproduce in the recorded runs.
This localizes the regression to the chosen WebKit subframe presentation path;
it does **not** prove a particular missing destination-tile algorithm inside WebKit.

RNDR now delivers the authored HTML as a main document using supported
`WKURLSchemeHandler` and `NSHTTPURLResponse` APIs. Security is retained by immutable
response-header `Content-Security-Policy: sandbox allow-scripts; ...`, not by a
meta sandbox directive or mutable guest wrapper. A source-free trusted preflight
checks opaque `self.origin` and denied localStorage **before the source transport
is armed**. No guest evaluation/message bridge was added. Public API is unchanged.

The per-view synchronous transport serves two exact GET URLs, once each. There
is no network/file resolver or global registry; retired handlers refuse all
requests. Native action and response gates admit only the native-armed main
load, once. Content blocking has two narrowly scoped in-memory URL exceptions;
a guest cannot retrieve either spent response, reload it or load it as a frame.

The existing private-WebKit opt-in remains necessary. No private scrolling or
rendering preference was used. Native security tests caught two differences
from iframe embedding: async clipboard exposure and speculative preconnect
sockets despite CSP. Guarded, read-back-verified artifact-only feature gates now
disable `AsyncClipboardAPIEnabled` and `LinkPreconnect`; payments are disabled
and public element-fullscreen preference is false. Unsupported/ineffective gates
refuse the preview. A response Permissions-Policy is defense in depth, **not** a
claim WebKit implements all its directives. All previous security gates remain.

No recolor, screenshot overlay, wheel cap, event interception, cache/tile tuning,
CSS overflow override, authored layout change or physical-app control was used.
Original doctype/quirks and original stored/exported HTML remain unchanged.

## Native evidence, 2026-11-27

macOS27.0.1 / Rust1.100, 1x60Hz display, owned windows only. Exact self-contained
reported fixture `/tmp/rndr-wheel-host-prepared.html`, 1200x850 owned-window capture.
ScreenCaptureKit requested120Hz; actual complete samples were lower. Navigation
Ready, rAF and didFinish are **not** paint acknowledgments. Actual pixels are the
rendering evidence. Every capture uses an explicit quiet rendered baseline and
clocked injection interval (`timeline.json`, `frame-times.json`).

* `target/cold-frame-{1..5}`: standalone historical wrapper,500ms quiet baseline,
  **5/5** runs each show one completely white 820800-pixel inset, **28.6–51.2ms
  after** first wheel injection. Each has1–9 prior normal quiet-baseline frames;
  none has pre-injection white. Inset excludes window chrome and edges.
* `target/cold-fixed-{2,3}`: matched500ms baseline, repaired full Metal host,
  **0 white**. `cold-fixed-1` had a driver Ready deadline timeout and no valid
  PNG samples; it is excluded, not a clean run.
* `target/main-rapid-line-{1..3}` and `main-rapid-pixel-{1..3}`: six repaired
  native rapid-reversal runs, **0 interior white**; the page traverses and returns.
* With a2-second quiet baseline, historical standalone iframe controls were
  clean in3 rapid-line runs and1 reconstructed physical-profile run. Incidence
  is warm-up/timing-sensitive; clean samples alone do not disprove the issue.
* `target/final-fractional-{1,2}`: all448+364 recorded wheel observations were
  **injected**, with their observed fractional AppKit delta axes, line/pixel
  class, phases and original monotonic intervals reconstructed into CGEvents.
  AppKit observed324+340 events (OS coalescing/delivery differs); native maximum
  vertical delta144.599456 lines. Both captures have **0 interior white**, with
  visible top/bottom traversal and return. Diagnostics summaries show0 queue
  drops/discards in `/tmp/rndr-main-native-fractional-{1,2}/artifact-*.jsonl`.
  This is useful native stress, **not lossless raw-CG replay or renewed physical
  mouse certification**. Source trace contains AppKit fields, not raw CG packets.
* Visually inspected `target/cold-frame-4-contact.png` and
  `target/final-fractional-2-contact.png`.

Native policy probe `/tmp/rndr-main-policy13.log` verifies missing response
sandbox by stripping **only** that directive from the actual production
preflight response: `Unsupported`, detached view, authored source never executed.
A document.open/write/close rewrite retains opaque origin, denied storage, CSP,
popup/navigation/form denial and disabled capabilities. The actual loopback
TCP/STUN sentinels remain zero before ordinary-browser separation testing.
HTTP, WebSocket, relative/file/custom-scheme fetch, spent transport, workers,
clipboard, capture/geo, payment/fullscreen/USB, file input, new windows and
navigation checks pass. The observation handler exists only in the executable.

Final native scrolling fixtures pass root/nested pixelXY, finger, momentum and
line-wheel scrolling and guest/host input separation. Full-Metal smoke passes
Ready, hide/show, fresh replacement, resize800x600→900x650 and cleanup before
parent destruction. One intentionally ignored Aqua unit smoke is not counted.

## Repeatable tools

Use only a **trusted self-contained local fixture** for the standalone comparison
app; it is not the production isolation API. Never target the principal app.

```sh
swiftc -warnings-as-errors examples/native-artifact-host/webkit_scroll_compare.swift -o /tmp/wk-compare
swiftc -warnings-as-errors examples/native-artifact-host/wheel_probe.swift -o /tmp/wheel-probe
swiftc -warnings-as-errors examples/native-artifact-host/wheel_frame_audit.swift -o /tmp/wheel-audit
# Historical standalone iframe (omit RNDR_WK_COMPARE for main-document control):
RNDR_WK_COMPARE=iframe RNDR_PROBE_QUIET_MS=500 RNDR_PROBE_WINDOW=1200,850 RNDR_PROBE_DISPLAY_CROP=1 \
  /tmp/wheel-probe /tmp/wk-compare /path/to/self-contained-dark.html target/comparison rapid-line
/tmp/wheel-audit target/comparison
# Repaired production host:
cargo build -p native-artifact-host --features rndr_browser/macos-private-webkit-artifacts
RNDR_PROBE_QUIET_MS=500 RNDR_PROBE_WINDOW=1200,850 RNDR_PROBE_DISPLAY_CROP=1 \
  /tmp/wheel-probe target/debug/native-artifact-host /path/to/self-contained-dark.html target/fixed rapid-line
# Reconstruct observed AppKit trace (bounded8MiB/10000wheel records/120s):
RNDR_PROBE_PREVIEW=2 RNDR_PROBE_WINDOW=1200,850 RNDR_PROBE_DISPLAY_CROP=1 \
  /tmp/wheel-probe target/debug/native-artifact-host /path/to/self-contained-dark.html target/native-profile \
  replay-native /path/to/artifact-PID.jsonl
```

The audit is **fixture-specific**, counts near-white pixels inside a dark
fixture's inset and generates a contact sheet. Authored white content is not a
general failure. Empty captures fail audit instead of being counted clean.
`RNDR_PROBE_QUIET_MS` is250..5000ms, default2000; vary it to expose warm-up
sensitivity. Optional production `RNDR_ARTIFACT_DIAGNOSTICS` must be a fresh0700
directory; the probe stops/flushes only its owned child's matching session file.

Remaining limits: no new physical G503 pass while the principal is away, no
Retina/multi-display transition or Windows/Linux certification, and no universal
WebKit raster-performance guarantee for arbitrary content. Production Medousa
integration/release rebuild belongs to the host; RNDR makes no app-install claim.
