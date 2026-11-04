# Native document editing — E2 viewport and closeout ledger

Status: **experimental implementation closeout; native certification remains open**.
The user accepted E2.2, committed as `6afdb36`, and authorized finishing E2.
This record covers the remaining viewport work and reconciles the milestone;
it does not authorize E3, structured editing, integration, a push or release.

## E2 inventory

| Unit | Implementation | Evidence boundary |
| --- | --- | --- |
| E2.1 input | Committed `3042621`: shaped Page movement, directed selection, explicit logical/visual movement policy, Tab ownership, durable IME ownership | [Input contract](native-document-editing-e2-input.md); named native IME/candidate-window combinations still unverified |
| E2.2 semantic ranges | Committed `6afdb36`: full text, directed selection, source/revision fences and shaped geometry through AccessKit | [Semantic contract](native-document-editing-e2-semantics.md); no VoiceOver/NVDA/Linux AT certification |
| E2.3 viewport | Interactive canonical scrollbar, retained source-surface integration, elapsed-time selection autoscroll and drag cancellation | Automated results and platform limitations below; not evidence of global pointer capture on every desktop |

## Viewport composition

`rndr_core::Scrollbar { target, axis }` is an additive focusable track. Its
`ScrollbarGeometry` is shared by pointer input, painting and accessibility.
It changes the target viewport, never text, source revisions, history or
selection. An ordinary `Scroll` supports horizontal (`Axis::Row`) and vertical
(`Axis::Column`) controls. A soft-wrapped `Editor` supports vertical scrolling
only: there is no artificial horizontal scroll range for wrapped source.
Existing passive `ScrollIndicator` consumers keep their existing behavior.
Editor ranges deliberately preserve the existing wheel/selection-drag convention:
the last shaped row can sit at the viewport top. The virtual extent includes
that trailing viewport, so even a short document may offer trailing scroll
space; the scrollbar reflects that canonical range rather than inventing a
second, ink-only clamp.

The retained `rndr_ui::editing::EditorSurface` mounts a vertical scrollbar by
default. `EditorSurfaceProps::scrollbar = false` opts out for a host-supplied
controller. Toggling the bar does not reattach the document, recreate its editor
or reset the viewport/history. Destroy frees owned chrome but preserves borrowed
root children and the session. The default track reserves 24 logical points;
its thumb has a 24-point minimum length when space permits. Ancestor clipping
limits the actual active interval. Host layout remains responsible for providing
usable physical track width; a clipped-away control does not gain an invisible
hit region.

Thumb drag retains pointer identity and grab offset, track clicks page, and
focused keys adjust the same viewport. Pointer presses preserve the existing
keyboard/text focus rather than selecting the scrollbar. The held thumb highlight
is driven by pointer capture and ends on release; explicit Tab/AT focus remains
visible and keyboard-operable. Accessible value/action routing uses the
same controller. Readonly source remains scrollable. A stale/hidden target,
changed contact/button, release or lost ownership cancels rather than retargets.
Scrollbars claim input before parent scrolling/editor selection in UiPlugin,
Shell and Chrome/GenericInteraction hosts. Retained tests cover both Shell/UI
plugin installation orders and UI-only hosts. An active preedit/native commit
blocks scrollbar focus stealing, preserving source ownership even when a commit
and track press share one frame. Coalesced pointer presses use the original
press point independently of hover; unrelated and synthetic keys are not
consumed as viewport commands. Ordinary
nested wheel remainder behavior is unchanged outside those controls.

Wheel input over the editor now routes to its canonical viewport even without a
scrollbar or keyboard focus. Over the track it routes to the same target. A
line-number `Gutter` routes to its single sibling editor; an ambiguous sibling
relationship does not guess an owner.
Sub-row pixel deltas accumulate across delivered frames instead of disappearing
when the editor publishes row-snapped offsets. Explicit scrollbar positioning
resets the residual; a changed text-layout key or external viewport movement
invalidates it. Editor vertical boundary remainder chains to ancestor `Scroll`
containers; horizontal deltas remain available to those ancestors.

Selection drag uses elapsed time and scheduled deadlines rather than scrolling
one row on each incidental delivered update. Its rate is one shaped row per
50 ms, with a maximum five-row catch-up after stalls; entering or reversing an
edge starts a fresh period. Stationary off-editor selection
therefore progresses without unrelated mouse motion. Release/cancellation
stops future ticks. Native blur also clears the runner's held pointer/contact
state. The winit backend source audit found AppKit mouseDragged/mouseUp and
Windows SetCapture/ReleaseCapture paths; this is code inspection, not live
cross-platform outside-window gesture evidence. Capture loss without release
or blur remains backend-dependent. The drag anchor remains directed and source selection stays
in the existing document/session path.

## Movement/geometry policy, not an implicit new bidi model

Left/Right and word commands remain logical source/grapheme motion. Home/End
remain the documented platform logical line/document commands. Up/Down/Page
use shaped rows and a retained wrap-row hint. No new public visual-bidi caret
model or universal affinity type is claimed. Source-session Parley tests now
exercise combining accents, ZWJ emoji, flags, mixed Arabic/Latin and CRLF;
readonly directed Page selection is checked against semantic caret geometry
under width changes. These are real shaped-font headless checks, **not OS input
or assistive-technology observations**.

## Native certification ledger

| Environment / behavior | Result |
| --- | --- |
| Headless model/input/retained UI/Parley/AccessKit bridge | Automated checks, commands and totals recorded below |
| macOS actual Japanese/Chinese/Korean/dead-key IME; candidate rectangle under resize/scroll | Not exercised; open native gate |
| macOS clipboard text/image fallback and VoiceOver text selection | Not exercised; open native gate |
| Windows IME, clipboard, NVDA and outside-window drag delivery | Not exercised; open native gate |
| Linux AT, X11/Wayland clipboard/primary and outside-window capture | Not exercised; open native gate |
| Broader physical bidi-affinity interaction across native fonts/platforms | Open; logical movement policy above is the supported experimental contract |
| Large-document latency/memory | Not signed off; E1 cost evidence remains, E3 is separate |

The original E2 native acceptance gate is **not checked off** merely because
implementation tests pass. E8 must retain these cells, not convert this ledger
into a blanket platform pass. A real native test operator/environment is needed
to close the certification gate. The implemented editor remains experimental.

## Verification

### Desktop feedback correction

The user reported missing wheel scrolling and a sticky selected scrollbar after
the initial candidate. The earlier suite covered wheel events over the track,
not the editor itself, and used large deltas that missed trackpad row-snap loss.
The persistent selected paint came from pointer presses assigning keyboard focus.
The correction adds editor wheel routing, fractional accumulation and focus-
preserving momentary capture, with retained tests for no/editor/track focus,
readonly and both Shell/UI installation orders; core boundary chaining and paint
held/release checks are included. The embedded-markdown regression now asserts
that its editor consumes wheel motion first and only boundary remainder scrolls
the markdown parent. Independent read-only inspection found no additional
concrete blocker in the inspected wheel/native-delivery and capture paths.
These are automated regressions. The corrected
native wheel/trackpad/release gesture still requires desktop retesting.

Correction verification:

- `cargo test -p rndr_core -p rndr_ui -p rndr_render -p rndr_paint -p source-session`:
  **901 passed, 0 failed, 1 ignored** across 120 test/doc-test binaries.
  Local receipt: `target/e2-wheel-regression.log`.
- Workspace all-target check, core/UI no-default-feature check, strict renderer
  clippy (including changed core/paint dependencies), formatting and whitespace
  checks pass. Workspace check retains only the unchanged `ui_polish` unused
  `prefix` warning. Receipt: `target/e2-wheel-check.log`.
- The prior focused-editor polish regression now asserts local accumulation,
  selection preservation and idle viewport retention instead of ancestor-only
  scrolling. When scrolling hides the caret decoration, selection is asserted
  against the canonical `Buffer`, not an offscreen `Caret` component.

### Initial candidate evidence

Initial candidate verification (all commands exited 0):

- `cargo test -p rndr_core -p rndr_ui -p rndr_render -p rndr_paint -p source-session`:
  **896 passed, 0 failed, 1 ignored** across 120 test/doc-test binaries. The ignored
  opt-in E1 whole-source cost probe is not a performance sign-off.
- `cargo check --workspace --all-targets`: pass. An unchanged `ui_polish` example
  reports its existing unused `prefix` variable; it was not changed for E2.
- `cargo check -p rndr_core -p rndr_ui --no-default-features`: pass.
- `cargo test -p rndr_render --no-default-features --lib`: **26 passed**.
- `cargo test -p rndr_render --no-default-features --features accesskit --lib`:
  **42 passed**.
- `cargo clippy -p rndr_render --all-targets -- -D warnings`: pass, including
  the changed core and paint dependencies.
- `cargo fmt --all --check` and `git diff --check`: pass.
- Independent read-only review first caught missing retained scrollbar input
  registration, IME focus theft and coalesced-press ownership. All were fixed
  with regressions. Fresh independent review found no remaining concrete blocker
  in the inspected paths and independently ran **34 passing** focused tests.
- `cargo run -p source-session -- --capture target/e2-viewport-final.png`:
  the final public fake fixture was rendered headlessly using the same shaped
  typography setup as native, then the actual **800×600** pixels were inspected.
  SHA-256: `5a9b625955f2b43ff7a1110b31a3981326ebad5248cae03ea0c60e813c212c57`.
  This verifies presentation, not live desktop interaction.

E2.2 is committed. The E2.3 closeout candidate remains uncommitted for user
review in the isolated worktree. Nothing is merged or pushed; E3 is not started.
The implementation work is complete within this experimental boundary, but the
original E2 native acceptance/certification gate remains open as recorded above.
