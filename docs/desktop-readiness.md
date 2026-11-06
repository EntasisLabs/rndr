# Desktop readiness evidence and release gate

**Status: useful runnable foundation; native visual and cross-platform acceptance
is still open.** This checklist records this undertaking's evidence, not a claim
that all implemented platform features are production-ready.

## Recovery onto current main — 2026-11-06 UTC

The foundation was recovered from commits
`53cfe295ba21bb4b7d0190e5398b53df368b1481` and
`f58c91b1f33cd4d7529177fa2435bae1f035e6de` onto main at `f1b91e2`.
The original undertaking, branch and worktree remain untouched. This is an
adaptation, not a replay of old core input code: `FormNavigation` now installs
`GenericInteraction`, kit fields use `FieldTransactions`, buttons use canonical
release activation, and draft cancellation uses the existing `reset_field_draft`.
The old queue-splitting router, `apply_field_keys` and `reset_text_field` APIs
were superseded and were not restored. Existing semantic UI and CI gates remain.

Fresh recovery verification is recorded in [desktop-recovery.md](desktop-recovery.md).
**Everything below is historical evidence reported by the original undertaking,
not fresh verification of this adaptation.** Its ignored logs, renders and bundle
are not present in this checkout merely because their paths are listed here.
Native IME, clipboard, accessibility and cross-platform/distribution gates remain
open; the old test counts and package launch do not close them.

## Historical scope and provenance — 2026-10-27 UTC

- Isolated branch: `worktree/rndr-desktop-app-readiness-foundation`, based on
  local main `6e611f59eaa3768cb946c757339abe63384af6c7`.
- Work: `work-e34eea8aac8625b96cec9bf7cfd991c0`.
- Host: macOS 27.0.1 (26A434), arm64, Rust 1.98.0.
- Read-only reference: `/Users/theelevators/learn-anything`, its README,
  VERIFICATION, `app/src/settings.rs` and UI helpers/navigation/tests in
  `app/src/main.rs`. Its old verification explicitly withholds native visual,
  IME, clipboard, accessibility and live-provider acceptance.
- This increment adds `rndr_app`, `examples/desktop`, packaging/observation
  scripts and docs. **No core/paint/render platform implementation was changed.**
  No Learn Anything or Medousa files, existing worktrees, refs or build outputs
  were modified. No publishing, push, signing identity, provider call or
  permission change occurred. Independent review/integration remains with the
  owner; completion is not approval or merge.

### Reproduction before behavior changes

The initial reference interaction test failed at Tab from Settings' field to
Save: focus remained on the field. Saved evidence is
`target/desktop-evidence/tab-before.log`. Inspection confirmed core's documented
`CapturesKeys`/file-tree behavior and Learn Anything's explicit app-form Tab
routing. The new `FormNavigation` plugin supplies that **opt-in form policy**
using the existing hierarchy and Focus resource. The same test now passes;
additional tests cover reverse/wrap, hidden ancestors, disabled controls, queued
key events and composition guard. This is not a claim of an upstream renderer
bug. No upstream failure was patched without reproduction.

The text-visibility fix is already on baseline main. Existing uncolored-label
regressions are preserved and run, and the example adds both-page static-text
paint coverage. It does not redo or clean the old text-visibility undertaking.

## Automated/local checks

Build environment throughout: `CARGO_BUILD_JOBS=2`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_INCREMENTAL=0`; locked, offline dependencies.
One new `target` was used. No release build or second large target was built.
The dev bundle is for local acceptance, not an optimized shipping artifact.
The existing macOS/Windows/Linux CI matrix already runs workspace tests and
strict clippy, so it includes the new tests. Its explicit example build now
includes `desktop`; registry archive validation explicitly excludes the two
unpublished packages. That CI configuration is updated, **not remotely run**
in this undertaking; Windows/Linux results remain unverified.

| Command or check | Result |
| --- | --- |
| `cargo fmt -p rndr_app -p desktop -- --check` | PASS |
| `cargo test -p rndr_core -p rndr_paint -p rndr_app -p desktop --locked --offline` | **390 passed**, 0 failed/ignored, including 9 doctests |
| New tests within that run | **10 passed**: 6 reference-app tests, 1 form-navigation test, 3 preference-store tests |
| `cargo test -p rndr_render --no-default-features --features parley,accesskit --lib --locked --offline` | **30 passed**, 0 failed/ignored; text shaping/scale/atlas, color/upload, decode, scene and idle-loop coverage |
| `cargo clippy -p rndr_app -p desktop --all-targets --locked --offline -- -D warnings` | PASS |
| `scripts/package-desktop-macos.sh dev` | PASS: linked arm64 app, original icon, valid Info.plist, complete `.app` |
| `sh -n scripts/package-desktop-macos.sh` | PASS |
| GPU `desktop --export target/desktop-evidence/render` | PASS: 880×760 1×, 1760×1520 2×, 420×640 Settings; real readback and PNG encoding |
| Local `open -n -W .../Desktop Workshop.app --args --memory` | PASS: LaunchServices launched the packaged executable, owned native window remained alive after 3 seconds; then only that owned process was stopped |
| `otool -L`, `file`, `vtool -show-build` | arm64; only Apple/system dynamic library paths; Mach-O minos 11.0, SDK 27.0. **Not proof of macOS 11 runtime compatibility.** |
| Read-only `codesign -dv` | Linker ad-hoc executable signature only; no TeamIdentifier, no sealed resources, Info.plist not bound. **No Developer ID/notarization claim.** |
| `git diff --check` | PASS |

Logs, images, preference-test fixtures and artifact hashes are local ignored
files under `target/desktop-evidence/` (plus kit fixtures under
`target/preferences-tests/`). `full-test.log`, `renderer-test.log`, `clippy.log`,
`package.log`, `render.log`, `native-app.log`, `binary-info.log`,
`codesign-info.log` and `artifact-sha256.log` are evidence, not committed source.
Initial implementation compile/lint errors were corrected before final checks;
there is no hidden failing final check.

The GPU images are **offscreen application renders**, not screenshots. Readback
checks only detect unexpectedly uniform output. The new paint tests prove
static text emits glyph geometry; headless layout proves constraints/scroll
behavior, **neither proves attractive typography or usable visual quality**.
No pixel-level aesthetic review or manual native mouse/keyboard interaction
was performed. A capability search found no local image/desktop inspection
tool. `CGPreflightScreenCaptureAccess()` returned **false** and no permission
was requested or changed. The window observer reads only the owned process's
CoreGraphics metadata, not content or unrelated window details.

## Desktop quality checklist

“Headless verified” means deterministic tests executed on this macOS host.
“Implemented/compiled” is weaker than a native interaction pass. Unchecked
acceptance items below remain release blockers for a production-quality app.

| Area | Verified here | Still required on a real desktop |
| --- | --- | --- |
| Text visibility/typography | Both example pages emit uncolored-label glyphs; existing paint regressions pass; Parley tests and actual 1×/2× GPU exports pass. Kit uses shared typography/spacing/Theme tokens. | **Unverified:** inspect actual GPU text, contrast, clipping, Unicode/font fallback, caret and focus fill on screen. App looks are not signed off. |
| Focus/keyboard | Example uses real Chrome TextField edits and Button Enter dispatch; form Tab/Shift-Tab traversal skips hidden/disabled controls and wraps. Page navigation moves focus; invalid settings do not commit. | **Unverified:** physical keyboard, menu/Command-Q behavior, key repeat, keyboard-layout changes, full-keyboard-access settings and focus after native activate/deactivate. Form plugin is single-window, no modal focus trap claim. |
| IME | Existing core IME/input/field tests pass; form navigation does not steal Tab during preedit/commit. Parley CJK/Arabic/emoji tests pass. | **Unverified:** Japanese/Chinese/Korean composition, candidate window placement, selection replacement, cancel/commit without duplicate characters and submit suppression in the actual OS input method. |
| Clipboard | Existing core clipboard/field tests pass, and native clipboard adapter compiles. Normal fields intentionally expose plaintext; no credential widget exists in this kit. | **Unverified:** native copy/cut/paste between this app and another app, Unicode/newlines, selection/caret behavior, OS clipboard ownership. X11 primary selection and Wayland clipboard require their own hosts. |
| Accessibility | Example access tree includes the TextInput with value and caption, excludes hidden page controls; button/focus roles use core's existing AccessKit path. | **Unverified:** VoiceOver announcements, traversal/order, set-value/activate from assistive technology, focus synchronization, status announcements. Windows Narrator and Linux AT-SPI need native tests too. A nonempty tree is not accessibility acceptance. |
| Scrolling/resizing | 420×640 structural layout, clipping width and wheel offset tested; body is a real Scroll container; native runner requests 420×640 minimum content. | **Unverified:** trackpad momentum, drag/wheel ergonomics, native minimum-size behavior, scroll-to-focused-field, long names, screen-edge clamping, maximize/fullscreen and tiny displays. |
| DPI/window behavior | Logical-point/window core tests pass; renderer's scale-reshape test passes; real offscreen 1×/2× exports pass. LaunchServices created an owned on-screen window (observed bounds 862×776 including chrome; OS placement/size can constrain the requested 880×760 content). | **Unverified:** moving between physical displays/scales, native resize/surface recreation, occlusion/minimize/restore, sleep/wake, title/Dock icon appearance and older OS/GPU combinations. Metadata is not a render-quality check. |
| Async/loading/error/empty | Sim fake-clock tests prove Empty→Loading→Ready, cancellation, deterministic failure→Retry→Ready and rejection of stale epochs. TaskHandle ownership uses the existing runtime. | **Unverified:** manual native responsiveness/animation/latency perception. This is local simulated work, not network/provider resilience or streaming acceptance. |
| Preferences | Missing/corrupt/oversized/unknown JSON, Unix 0600, atomic replacement/failure cleanup tested. App schema version/validation, successful-save relaunch, discarded draft, failed-save preservation and no inference on save tested. | **Unverified:** Windows overwrite semantics, concurrent writers, crash/disk-full fault injection, OS secure storage. No secret, encryption or hardened-memory claim. |
| Distribution | A local arm64 dev `.app` launches through LaunchServices without invoking Cargo; icon/plist/library metadata inspected. Reproducible release command documented. | **Unverified:** optimized release build, clean-machine/older-macOS install, Intel/universal binary, Gatekeeper download path, Developer ID signing/notarization, updates, uninstall and App Store packaging. |

### Platform status in this increment

| Platform | Build/headless evidence | Native evidence | Packaging/signoff |
| --- | --- | --- | --- |
| macOS arm64 27.0.1 | Tests/lint/build above passed | Packaged app launch and owned-window metadata only | Local dev `.app` built. Visual, IME, clipboard and VoiceOver remain open; unsigned/ad-hoc, not notarized |
| macOS Intel / older OS | **Not built or tested** here | None | No universal binary or compatibility promise |
| Windows | Core code exists; **not built or tested on Windows** here | None | No installer/signing test; preferences replacement needs validation |
| Linux X11 | Core/platform code exists; **not built or tested on Linux** here | None | No AppImage/deb/rpm or distro dependency validation |
| Linux Wayland | Core/platform code exists; **not built or tested on Linux** here | None | Compositor/portal/clipboard/a11y/window limitations require native validation |

## Artifact and resource accounting

Local artifact (relative to this isolated worktree):

```text
target/bundle/Desktop Workshop.app
```

The bundle is about **58 MiB**, includes an arm64 dev executable, Info.plist and
`.icns`. It is intentionally left for independent review. Logs and PNGs live
beside it under `target/desktop-evidence/`. It is not committed or published.

`df` was checked before building: about **13 GiB** available. An intermediate
native smoke reported **11 GiB** free; the final check reported about **12 GiB**
(free space fluctuated independently). This work's final target was about
**2.2 GiB**, including bounded Cargo output, the bundle, Swift observation
module cache, and the first smoke bundle preserved as
`target/bundle/Desktop Workshop.initial-smoke.app`. The final bundle was
rebuilt after setting the tested 420×640 minimum size; both native smoke runs
passed. Refusal to overwrite an existing bundle was also exercised.
No old target was used as a write destination, cloned, hardlinked or cleaned.
No source/history/user work was removed. Package generation cleans only its
own uniquely-created staging directory and refuses to replace a prior bundle.

## Review and next steps

1. Independently review/integrate this bounded patch; run the manual reference
   app pass in [desktop-apps.md](desktop-apps.md), inspect the exported PNGs,
   and do native text/focus/scroll/resize/IME/clipboard/VoiceOver acceptance.
2. On separately scoped app work, adopt suitable kit helpers in Learn Anything
   without moving learning/Medousa/provider logic into rndr. Preserve its
   session-only secret boundary and explicit live-cost consent. No cross-repo
   changes were made here.
3. Only after those gates: validate Windows/Linux and clean-Mac distribution,
   choose supported OS/architecture floors, build an optimized signed package
   with the owner's release workflow. Do not infer those results from this
   macOS headless run.
