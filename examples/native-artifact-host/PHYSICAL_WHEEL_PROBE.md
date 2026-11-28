# Physical mouse / local native artifact probe (macOS)

This is instrumentation, **not a scrolling or white-flash repair**. The same
probe runs in production `ArtifactPreview` users, including Medousa, when their
RNDR build enables `macos-private-webkit-artifacts`. No Medousa source hook or
`native-artifact-harness` feature is required. Ordinary `Browser` is untouched.

## Production integration contract

Set `RNDR_ARTIFACT_DIAGNOSTICS` to an **absolute, existing private directory**
(mode 0700) before starting the app. The first attached artifact starts one
process-wide session. The env var is read once, not a live toggle. With it
unset no monitor, file or writer thread is created. Invalid/nonprivate paths,
existing output files or writer failures disable diagnostics, not previews.
The logger writes directly, so release log-level filtering cannot remove it.

Output: `DIRECTORY/artifact-PID.jsonl`, exclusive creation, mode 0600.
- 256-slot nonblocking queue; UI callback copies fixed-size values only.
- At most **20,000 event/marker records plus a header and summary**, or **600
  seconds** after first attach. Limits are per process, across replacements.
- One background writer; periodic flush approximately every 250 ms. It checks
  marker/stop controls between records or at most 100 ms idle waits. These are
  best-effort timing targets, not scheduler/disk guarantees.
- `touch DIRECTORY/diagnostic.marker`: process-wide external marker, no preview
  attribution; timestamp is observation time, not exact `touch` or flash time.
  Repeated touches faster than observation may coalesce.
- `touch DIRECTORY/diagnostic.stop`: stops diagnostics only. It does not close
  the app, change scrolling, or inject input. Wait for the final `summary`
  record before retrieving the log or quitting. Already queued records drain
  within the cap; summary reports `queue_dropped` and `pending_discarded`.
- Stop, expiry or cap exhaustion is one-shot; restart the app with a fresh
  directory to rearm. Do not delete the stop file expecting a restart.
- Abrupt or ordinary process exit is **not** a guaranteed final flush: the
  writer is detached and the session sender is static. Stop first and verify
  summary. A missing summary means an incomplete capture, not zero loss.

### What is recorded

`wheel`: AppKit local **pre-dispatch observation**, hit-tested to this preview's
native subtree in its own window. `event_uptime` is NSEvent's system-uptime
seconds; `seconds` is the logger's monotonic receipt time relative to session
start. The session header includes an approximate wall-clock `unix_seconds`
anchor taken by the writer, not an exact conversion between these clock domains.
`delta` is `[scrollingDeltaX, scrollingDeltaY]`, `precise` and `units` distinguish
points from line-wheel values. `legacy_delta_z` is NSEvent's separate legacy
accessor, not claimed to share the other axes' units. Raw native phase and
momentum bit values, natural-scroll inversion, window-local position in points
and window number are included. No synthesized events are sent by this probe.

`local_monitor_ready` confirms successful monitor registration. Attach/detach,
preparing/loading/ready/failure, navigation progress/completion/errors and
`content_process_terminated` are correlated by monotonically assigned preview
IDs. `geometry` records **requested** host bounds/visibility, current backing
scale and native window ID, only when changed. It is sampled when the embedding
host calls the artifact frame method; it is not an independent display observer.
Replacing a preview creates a new ID and removes the old local monitor before
retiring its view. Closing/dropping the preview removes its monitor.

**Flash marker:** click inside the artifact first, then press **Cmd+Shift+M**
(ANSI physical M key position, keycode 46). The fixed chord is recognized only
when first responder is the artifact's WKWebView subtree in its own window.
It logs `flash_marker`, not ordinary key contents. The event is returned
unchanged, so an existing host/guest shortcut may also run. The external marker
file is an alternative without touching artifact focus. Marker delay includes
human reaction and UI scheduling; it is not the flash onset timestamp.

No key text, HTML, URL, clipboard, screenshots or input outside the artifact
subtree is recorded. No global input monitoring, Accessibility or Screen
Recording permission is needed **for native logging**. Local monitor hit-testing
excludes host controls and other previews. It is not evidence that WebKit
consumed the event, moved the scroll destination or presented pixels; gesture
capture with a pointer outside the subtree may also be excluded.

## Manual RNDR session (no automatic input)

Coordinate with anyone using the desktop first. Do not stop their application
or launch a second Medousa instance against their workspace. The Medousa host
owns its launcher and existing-app safety checks. For a separate RNDR fixture:

```sh
cd /Users/theelevators/rndr
cargo build -p native-artifact-host --features rndr_browser/macos-private-webkit-artifacts
RUN=$(mktemp -d /tmp/rndr-artifact-physical.XXXXXX)
chmod 700 "$RUN"
echo "$RUN"                 # retain this path in your second terminal
RNDR_ARTIFACT_DIAGNOSTICS="$RUN" target/debug/native-artifact-host --scroll-stress \
  >"$RUN/host.log" 2>&1 &
PROBE_PID=$!
echo "owned RNDR PID=$PROBE_PID"
```

Use `--html /absolute/path/to/fixture.html` instead of `--scroll-stress` for the
exact authored artifact. Neither form includes `--smoke` or a synthetic input
driver. The native test window is interactive until you close it.

1. Select the owned test window yourself. Verify `local_monitor_ready` and
   `ready` in `artifact-PID.jsonl` (the file is created on first attach).
2. With the **physical mouse**, scroll normally, then the rapid/reversal pattern
   that causes the flash. Mark each visible flash with the focused chord; note
   wheel model, smooth/discrete mode and whether it was the whole guest or edge.
3. From another terminal, using the printed `RUN` path:
   ```sh
   touch "$RUN/diagnostic.marker"   # optional external marker
   touch "$RUN/diagnostic.stop"
   tail -n 1 "$RUN"/artifact-*.jsonl  # wait until kind is summary
   ```
4. Retain `artifact-PID.jsonl` and `host.log`, plus your observations. Check the
   summary reason and both loss counters. Close **only this owned test window**
   after summary. Do not interpret a silent/unfinished file as a clean run.

## Scope and observer effects

Native logging does not alter wheel routing, scroll deltas, gesture phases,
WebKit preferences, rendering, CSS, background, overlays or sandbox policy.
Nevertheless callbacks, queueing and filesystem work can affect scheduling;
compare with a fresh env-unset run if the failure changes.

No guest JS scroll/rAF channel is added. The isolated srcdoc has an opaque
origin; introducing a new production guest-message bridge solely for diagnostics
was deliberately avoided. Existing harness-only observation channels remain
harness-only. **JS rAF is not paint completion**; native navigation completion
and Ready are not paint completion either. These logs plus a human marker can
correlate delivered input and host lifecycle, but cannot independently prove
WebKit scroll position, tile readiness, raster stall or the cause of a flash.
`wheel_probe.swift manual` can separately capture an owned window for 10 seconds,
but has different Screen Recording/input-permission and observer-effect costs;
it is not required to use this logger and its passive event tap is not relied on.

## Validation receipt

On macOS 27 / Rust 1.100, a separately owned full-Metal host was activated only
in a coordinated foreground slot. Native synthesized events traversed AppKit:
8 guest wheel records (line/precise, phase/momentum), a focused WKContentView
marker and external marker reached JSONL; a host-strip wheel was excluded.
Explicit stop produced a flushed loss-free summary. This validates plumbing,
**not physical mouse certification or white-flash resolution**. Another native
run verified hide/show, fresh replacement IDs, detach, resize and stop; an
env-unset run completed without diagnostic initialization. All owned windows
were closed; the principal app was not stopped. Queue overflow, hard admission
cap, serialization, private-path refusal, stop drain and record-limit discard
accounting have unit regressions.
