# Native document editing — E2.2 semantic range candidate

Status: experimental semantic range increment after the E2.1 input commit
`3042621` (E1 plus polish: `aab3860`). **Not E2 completion or native AT
certification.** Interactive scrollbars/capture and document-scale performance
remain separate slices. No Medousa dependency or structured editor is added.

## Core range and action contract

`access_text(world, entity)` snapshots an available `Buffer` + `Editor` using
full document bytes, never the clipped paint `Label` or transient IME text.
The snapshot carries a directed UTF-8 byte selection, fresh shaped layout from
the editor's engine/style/spans/wrap width, window-logical origin including
row-snapped vertical translation, viewport clip, retained caret-row affinity,
readonly policy and current selection-action admission. `caret_bounds()` and
`selection_bounds()` return unclipped shaped rectangles; offscreen positions
remain addressable, but their rectangles are not a claim of visible pixels.

`AccessCommand::SetTextSelection` echoes an opaque `AccessTextState` plus the
requested directed endpoints. The fence binds exact view generation and bytes;
managed sessions additionally bind document identity and revision. Thus a
source edit then undo, same-text document reattachment, stale host projection,
foreign view token, hidden/disabled/modal-blocked/dead view or nonfocused target
refuses safely. The action does not steal focus: use the existing Focus action
first, then current ranges. Native preedit/commit, projected preedit and active
source composition block selection actions. Scalar-splitting/out-of-bounds
endpoints refuse; valid scalar endpoints floor to interactive graphemes.

Readonly content remains selectable. Managed selection goes through the source
session, breaks typing coalescing and reprojects the view; legacy selection
uses its existing history owner. Neither path creates a content revision,
change event or undo entry. The normal editor publisher reveals the caret and
clears stale drag/preferred-x state. Subsequent native typing replaces the
selected source and uses the existing undo owner.

Legacy Buffer freshness compares exact bytes rather than a revision counter;
an edit/undo returning to identical legacy bytes is not distinguishable. The
managed source contract does distinguish it. Snapshot construction currently
shapes full text on demand. This is **not an E3 performance sign-off**.

## Native bridge

The AccessKit bridge uses the existing desktop runner boundary, not OS calls
from UI. Editor text containers and their published text runs supply native
selection endpoints. The adapter must translate AccessKit character indices
through the run's declared grapheme byte lengths, not through scalar counts,
UTF-16 counts, x coordinates or raw byte offsets. Full text includes hard
newlines and CRLF; empty documents and final empty lines remain addressable.
SetValue remains unsupported: selection is not an alternate raw text-write API.

Published run identities and their text fence are retained by the runner;
selection requests must match the root tree, target editor and endpoint owner.
Changed text/revision or incompatible run boundaries retire old identities
instead of reinterpreting queued endpoints. Geometry is refreshed under
scroll/resize without inventing mixed-bidi directional information.

Representability is deliberately conservative:

- Native selectable units are interactive graphemes; each declared length is
  a checked UTF-8 byte count. A grapheme above AccessKit's 255-byte limit, or a
  shaped partition inside a grapheme, fails the whole range bridge closed to
  ordinary editor semantics without SetTextSelection. No truncation or split
  selectable unit is fabricated.
- Each shaped row is a TextRun; hard-break bytes belong to the preceding run.
  Empty documents and trailing empty lines retain an addressable empty run.
- The runner allocates entity and synthetic IDs in one collision-aware retained
  namespace, remaps relationships, and never recycles retired IDs. The mapping
  advances only when AccessKit's update-if-active callback actually publishes.
- Native containers clip run geometry to the viewport. Unambiguous monotone LTR
  caret edges provide character positions/widths; mixed/RTL/ambiguous geometry
  omits those optional arrays rather than inventing direction. Logical source
  text and selection remain available; visual-bidi certification is open.
- Published focus uses the retained caret row at shared wrap endpoints. Anchor
  endpoints canonicalize downstream. Incoming native selection resolves to
  logical source bytes and the editor's default affinity; full visual-affinity
  round-tripping is not claimed.

The experimental boundary remains narrower than a universal visual-bidi
position API. Lifetime-retained native ID bookkeeping and repeated full-text
shaping require the later document-scale performance/memory gate.

## Verification and remaining gates

Core tests: `crates/rndr_core/tests/access_text.rs` cover full-source vs paint
window exposure, Unicode scalar validation and grapheme snapping, reversed
readonly selection, revision/reattachment/foreign-view fences, unavailable and
composing targets, modal admission, shaped wrap/scroll/reveal geometry and
accessible-selection → native replacement → undo ownership. They also refuse
unprojected host-only selection changes before exposing canonical ranges.

Native tests in `crates/rndr_render/src/a11y_text.rs` and `a11y.rs` cover Unicode
byte units/CRLF, empty/newline-only text, directed endpoints, forged/cross-editor
requests, ROOT tree admission, future entity/synthetic ID collisions, same-text
source switching and revision ABA, readonly/focus/IME policy, scroll/resize,
overlong graphemes, conservative bidi geometry and retained wrap focus. A
bundled Parley fixture exercises mixed Unicode, real shaped wrapping and
scrolling rather than relying only on headless fallback metrics.

Regression gates:

```sh
cargo test -p rndr_core -p rndr_ui -p rndr_render -p source-session
cargo check --workspace --all-targets
cargo check -p rndr_render --no-default-features
cargo fmt --all -- --check
git diff --check
```

Final result: **805 passed, zero failed, one intentionally ignored cost probe**
across the four-package regression command (103 test suites including doctests).
Workspace all-target compilation, the renderer's no-default-features build,
formatting, whitespace and local documentation link/fence checks passed. The
workspace check retains the pre-existing unused `prefix` warning in
`crates/rndr/examples/ui_polish.rs`; no unrelated cleanup was included.

The AccessKit-only renderer configuration also passed **40 library tests** and
strict Clippy (`--no-default-features --features accesskit --lib --tests --
-D warnings`). Two separate read-only correctness reviews found no concrete
blocker in the inspected core and native contracts. The host subsequently added
host-selection projection fencing, explicit empty/newline-only and bundled
Parley geometry tests, and retained-wrap focus publication; all pass in the
final suite. Native incoming visual-affinity round-tripping remains deliberately
outside this source-range contract.

Private reproducible receipts: `target/e2-semantics-tests.log`,
`target/e2-semantics-check.log`, `target/e2-semantics-minimal.log`,
`target/e2-semantics-accesskit-tests.log`, `target/e2-semantics-clippy.log`.
E2.2 remains uncommitted for principal review; nothing merged or pushed.

No real VoiceOver, NVDA, Linux AT, hardware IME, clipboard, out-of-window
capture or performance certification is claimed. The named platform ledger in
[the epic](native-document-editing-epic.md)
remains open. Run `cargo run -p source-session` for the same fake-host editor;
this slice adds the accessibility boundary, not a visual redesign.
