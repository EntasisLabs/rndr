> Historical investigation; superseded by the applied, verified
> [main-document repair](MAIN_DOCUMENT_SCROLL_FIX.md). Rejected experiments below
> remain evidence, not current backend behavior.

# Fast native wheel diagnostic

The fast-wheel whole-artifact white flash is **not fixed**. The earlier root
overscroll repair only addresses elastic movement beyond document bounds.

## Reproduce without touching the principal's app

Coordinate foreground window ownership first. From the RNDR checkout:

```sh
cargo build -p native-artifact-host --features rndr_browser/macos-private-webkit-artifacts
swiftc -warnings-as-errors examples/native-artifact-host/wheel_probe.swift -o /tmp/rndr-wheel-probe
RNDR_PROBE_WINDOW=1200,850 RNDR_PROBE_DISPLAY_CROP=1 \
  /tmp/rndr-wheel-probe "$PWD/target/debug/native-artifact-host" \
  /absolute/path/to/host-prepared-artifact.html "$PWD/target/wheel-new-run" rapid-line
```

Use a new output directory each time. Supply the preview copy, including the
host's scrollbar normalization, not a differently styled substitute. This does
not rewrite the input or stored/exported artifact.

- `line` / `pixel`: six alternating traversals, 12 events per burst, 4ms requested
  spacing, 500ms holds. 100 lines or 1600 pixels per event.
- `rapid-line` / `rapid-pixel`: 30 alternating traversals, eight events per burst,
  no inter-burst hold, 160 lines or 1600 pixels per event.
- `extreme-line` / `extreme-pixel`: one 30000-unit event per traversal.
- All automatic profiles explicitly set phase and momentum to zero. Line input
  is non-continuous; pixel input is precise/continuous but unphased.
- `manual`: ten seconds of hardware input in this tool's owned child window.
- `replay TRACE`: replays wheel fields and relative timing from a recorded JSON
  sequence. `injected.json` is a synthetic schedule, **not** delivered telemetry.

The tool starts and terminates its own child. It never attaches to the running
Medousa app. Only scroll-wheel events for that child are eligible for the optional
passive process-specific tap. No keys or guest content are traced. That tap was
unavailable in this test environment; hardware event tracing is **unverified**.
Do not grant broad monitoring permissions just to run the automatic profiles.
Manual mode can still capture images without event telemetry.

ScreenCaptureKit captures only complete samples. The default is an independent
owned-window capture; `RNDR_PROBE_DISPLAY_CROP=1` captures only that foreground
window's rectangle on its display, corroborating actual composited presentation.
Keep unrelated windows out of the owned rectangle. Capture requests 120Hz, but
actual delivery depends on display cadence, content changes and encoding load;
**frame index is not elapsed time**. PNGs plus `frame-times.json` retain the
actual presentation timestamps. Counts are not a guarantee of catching every
sub-refresh transient. Frames/events are bounded to 2400/10000 per run.

## Investigation result (2026-11-27)

Exact prior normalized fixture SHA-256:
`b63a4ed90d4e413fce128e5881786df970364ee1f9045f0134651e62dd5b7e29`.
A temporary copy also included the production `medousa-native-scrollbars` style.
Tests used the real native full-Metal host, macOS 27, 1x 60Hz display. An AX
1200x850 window produced a 1200x818 client and 1200x778 guest viewport.

Early slower and sparse-profile runs missed the defect. Dense captures reproduced
whole-guest white with native chrome and scrollbar retained. Final complete-sample
on-display replay controls used 240 unphased events over approximately 0.96s:

| Profile | Fresh processes with white / tested |
| --- | ---: |
| Precise pixel, 1600px/event | 2 / 3 |
| Discrete line, 160 lines/event | 2 / 3 |

Each identified full-white frame had 820800/820800 near-white interior pixels in
the audit rectangle (30px side/bottom margin, 100px top exclusion). Actual frames
were visually inspected, not just classified. This dark fixture authors no such
white canvas; this threshold is not a general oracle for arbitrary HTML.

Custom scrollbar styling is not necessary: faster reversals also reproduced in
three of three fresh no-custom-scrollbar controls. Other earlier control runs
missed intermittently, so one clean run is not proof of a repair.

During the reproduced flash there was no host replacement, visibility transition,
viewport change, overlay or state transition out of Ready. Source inspection found
steady-state geometry/visibility application idempotent. The production host's
separate lifecycle audit found no wheel-triggered fetch/reload. This narrows the
failure to WebKit content presentation; the exact engine-level tile/cache cause
is still a **hypothesis**, not a demonstrated fact.

A throwaway, artifact-only private `AsyncFrameScrollingEnabled=false` experiment
read back false and produced zero white frames, but failed the essential
reachability check: it stopped effective document traversal. It was rejected and
all experimental backend changes were removed byte-for-byte. Background coloring,
matting, arbitrary authored-CSS changes, blind cache preferences and scroll-rate
limiting were not shipped. No safe supported per-view rendering-consistency
control was identified. Production RNDR behavior is unchanged by this diagnostic.

Next useful work is a minimal upstream WebKit reproduction separating isolated
subframe rendering from native host composition, with actual scroll-position /
paint-completion instrumentation and latency/reachability acceptance checks.
Do not label a candidate fixed merely because it eliminates white pixels.
