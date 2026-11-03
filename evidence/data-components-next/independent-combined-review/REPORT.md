# SINGLE combined independent review — changes requested

Candidate: **39b049feb32b50a7000bcb60ead62e16ee4fe8e2**. Audit parent
`132258c535b7c75dcc7256b1eea89e913eaaed15`; accepted product baseline
`57c30a3242efcea7d54a6083251575ac04f1c603`.
Undertaking: `work-8edd6bffbd5f264c66f0ffd4aef16464`.
Branch: `worktree/rndr-composable-datatable-and-propertyinspector`.

**Verdict: DO NOT INTEGRATE this candidate.** Four confirmed code findings below
(five failing added tests). This is one combined review of the canonical editor
prerequisite, both data components and the connected gallery, not a separate
prerequisite approval. No production repair, acceptance, merge, push, or Blooey
operation was performed. Review artifacts remain untracked for owner handoff.

The interrupted review's passing gate/logs were reused after verifying actual
HEAD, no tracked worktree diff, and matching capture-source hashes. Only focused
adversarial suites were rerun. Scope was the implementation/API, affected shared
interaction/accessibility paths, retained regressions, gallery host and native
capture evidence. This is not exhaustive platform certification.

## Confirmed findings

### F1 — High / P1: acknowledging Submit discards later queued typing

Source: `crates/rndr_ui/src/data.rs:1349-1372`, especially unconditional draft
replacement at 1368-1372; `set_draft` clears pending at 1637. Connected host
consumer: `crates/rndr_ui/examples/support/data_editors_gallery.rs:433-458`.

In the joint gallery, focus title `Brand refresh`, enqueue `1`, Enter, `2` in one
frame. The host correctly commits `Brand refresh1` at Enter, but inspector draft
becomes `Brand refresh1`, losing `2`; expected uncommitted draft is
`Brand refresh12`. This is observed loss in the actual host, not just a field
snapshot concern. Later typing clears `pending`; acknowledging the preceding
commit is then treated as an ordinary changed committed value and resets draft,
caret and history. The prerequisite's standalone Submitted snapshot tests pass,
but its intended chronological safety does not survive host acknowledgement.

Reproduction: `reviewer_gallery.rs:218-227`,
`reviewer_host_ack_preserves_text_queued_after_submit`.
Old log `host-queue-probe.log`; fresh confirmation `retry/gallery.log`.
Required correction: distinguish acknowledgement of earlier transactions from
unrelated external replacement and retain subsequent uncommitted edits without
committing them early. Add permanent end-to-end coverage.

### F2 — Medium / P2: window blur fails to cancel non-text-focused dirty drafts

Source: `crates/rndr_ui/src/data.rs:1731-1759`; related dispatch
`crates/rndr_core/src/interaction.rs:504-532` and
`crates/rndr_core/src/field.rs:78-85`.

Two independently reproduced cases:

1. Focus Boolean editor, Enter toggles draft false -> true, send
   `WindowFocus::Blurred`, frame: draft remains `Boolean(true)` rather than false.
2. Text `A`, focus editor, enqueue `x`, Tab to that property's Commit button,
   send window blur, frame: draft remains `Ax` rather than `A`.

Generic interaction notifies focus loss on blur, but field notification applies
only to opted-in TextFields. Canonical logical Focus remains on the button;
post-update inspector containment sees it as inside and does not cancel. This
contradicts the explicit window-blur cancellation contract in docs/data-editors.md.
Ordinary same-property editor -> Commit retention is correct and must remain so;
window blur is not ordinary intra-property navigation.

Reproductions: `reviewer_data.rs:542-559`,
`reviewer_boolean_window_blur_cancels_draft` and
`reviewer_commit_control_window_blur_cancels_draft`.
Original `probes.log`; retry `retry/data.log`.
The maintained text-editor-focused window-blur control passes unchanged in the
same run. Confirmed: dirty draft survival. NOT claimed: blur alone commits model,
physical OS input reproduction, or every custom editor failing identically.
Boolean/Commit failure is enough to block this lifecycle contract; choice/custom
paths warrant the same correction/coverage, not unsupported separate findings.

### F3 — Medium / P2: leave-and-return within a batch bypasses cancellation

Source: `crates/rndr_ui/src/data.rs:1017-1033,1462-1478,1731-1759`;
`crates/rndr_core/src/field.rs:78-85`.

Start in text property editor, enqueue `x, Tab, Tab, Tab, Shift-Tab, Shift-Tab,
Enter`. Focus leaves the property from its Cancel control, then returns to its
Commit control in the same input frame. No TextField transition hook observes
that outbound button transition. End-of-frame containment sees focus back inside,
and Enter emits CommitRequested for the stale `Ax` draft; cancellation at the
actual departure should have reset it to `A` first. This is a queued native-key
API reproduction, not a claim about physically timed keyboard events.

Reproduction: `reviewer_data.rs:562-569`,
`reviewer_leave_and_return_in_one_batch_must_cancel_before_commit`.
Original `focus-queue-probe.log`; confirmation `retry/data.log`.
Do not fix only final focus containment: actual boundary ordering matters for
non-text controls too. Preserve legitimate same-property Commit/Cancel behavior.

### F4 — Medium / P2: opted-in per-key IME filter omits ime_enabled

Source: `crates/rndr_core/src/field.rs:125-133`; canonical filter contract
`crates/rndr_core/src/ime.rs:132-155`.

Set Input.ime_enabled=true, ime_commit=`日`, keys=[character `n`]. Opted-in field
renders `n日` rather than `日`. The temporary Input used by transaction_key copies
preedit/commit but not ime_enabled, bypassing canonical enabled-IME text stripping.
The maintained matching-character/preedit test passes; it does not cover distinct
raw key text transformed into the committed character.

Reproduction: `reviewer_field.rs:239-244`,
`reviewer_ime_enabled_commit_strips_different_key_text`.
Original `ime-probe.log`; confirmation `retry/field.log`.
Confirmed public Input-snapshot contract defect. The desktop runner already
resolves ordered IME events, which may sanitize such keys before this seam;
there is NO claim that a real OS IME was observed duplicating text. Real IME
integration remains untested, not an extra confirmed native defect.

## Implementation and public API assessment

The reusable design is coherent: DataTable composes ListView and canonical
controls; stable row/column IDs, passive cell slots, host-owned sorting and
explicit declarative widths fit existing architecture. Table/row/header/cell
roles and supplementary header associations preserve cell names through the
AccessKit bridge. No separate editor/selection/scrolling implementation is added.
Inspector explicitly separates raw/parsed/error draft from host value; events
carry object/property identity, validation is deterministic, and custom editors
use explicit composition rather than reflection. The gallery really applies
requests to an eight-record host model and repaints table cells.

Reviewed tests cover ID/order/resize retention, invalid/duplicate/bounded mounts,
stale handles, grouping, object switches, validation, disabled/readonly,
reconciliation, cancellation/unmount, geometry and accessibility projection.
No additional DataTable-specific blocking defect was established. This is not
an unconditional table acceptance independent of the combined candidate.

Limits are honestly documented: 256 rows x 32 columns, application paging rather
than virtualization, row-only focus/single-or-no selection, passive cells, width
48-2048 changed declaratively (no drag-resize), no spreadsheet editing, inspector
128 properties/32 groups/64 choices, no persistence/app undo. These are explicit
milestone limits, not silently implemented features. Accessible SetValue remains
unsupported; no unsafe history-bypassing edit is advertised. Live AT is untested.

## Opt-in editor correction versus legacy fields

`FieldTransactions` opts TextField into chronological GenericInteraction routing;
existing canonical grapheme/caret/clipboard/undo logic remains authoritative.
Changed/Submit/Cancel/FocusLost hooks are synchronous. reset_field_draft is explicit
owner replacement, not typing or application undo. Unopted fields deliberately
retain legacy behavior. This is an additive correction, NOT a blanket fix for all
existing TextFields or Blooey adoption.

The maintained nine field tests pass all five saved boundary scenarios under the
new marker, including exact Submit snapshots, repeated Enter, text around Tab and
captured Escape via the typed Cancel hook. Escape's assertion is adapted to the
new contract rather than leaking captured keys into tree navigation. Historical
probes remain unchanged. The independent legacy replay still reproduces all five
old failures (expected exit 101); it is not counted as passing correction proof.
See `legacy-reproduction/receipt.json`, `probes.log`, `source.sha256.json`.
F1 shows why standalone field snapshot success is insufficient for the integrated
host; F4 is a separate missing input flag on the opt-in path.

## Exact independent verification and results

Saved independent gate command:

`bash scripts/verify-data-editors.sh evidence/data-components-next/independent-combined-review/verification`

All **10** log labels exit 0, **708 passing executions**, not unique tests:

| Log | Exact recorded command | Passing executions |
|---|---|---:|
| `core-paint-ui.log` | `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui ` | 599 |
| `facade-fallback.log` | `cargo test --offline --locked -p rndr --lib --no-default-features ` | 1 |
| `facade.log` | `cargo test --offline --locked -p rndr --lib --features observe ` | 1 |
| `fallback-renderer.log` | `cargo test --offline --locked -p rndr_render --lib --no-default-features ` | 25 |
| `formatting.log` | `rustfmt --edition 2021 --config skip_children=true --check crates/rndr_core/src/access.rs crates/rndr_core/src/field.rs crates/rndr_core/src/interaction.rs crates/rndr_core/src/lib.rs crates/rndr_render/src/a11y.rs crates/rndr_ui/src/collections.rs crates/rndr_ui/src/controls.rs crates/rndr_ui/src/data.rs crates/rndr_ui/src/lib.rs crates/rndr_ui/tests/data_editors.rs crates/rndr_ui/tests/data_editors_gallery.rs crates/rndr_ui/tests/field_transactions.rs crates/rndr_ui/examples/support/data_editors_gallery.rs crates/rndr/examples/data_editors_gallery.rs ` | 0 |
| `gallery-build.log` | `cargo build --offline --locked -p rndr --example data_editors_gallery --features observe ` | 0 |
| `gallery-fallback.log` | `cargo check --offline --locked -p rndr --example data_editors_gallery --no-default-features ` | 0 |
| `historical-six-and-activation.log` | `cargo test --offline --locked -p rndr_ui --test combined_original_six --test combined_activation_policy ` | 8 |
| `native-observe.log` | `cargo test --offline --locked -p rndr_render --lib --features observe ` | 74 |
| `whitespace.log` | `git diff --check ` | 0 |

Commands and stdout are in `verification/*.log`; environment in
`verification/environment.txt` (rustc 1.98.0, Cargo 1.98.0, offline/locked,
two jobs, incremental/debug disabled, existing compatible target cache).
The original six collection reproductions and two activation-policy tests pass;
the maintained 12 chronology regressions also pass. No full-gate rerun was needed
on resume because HEAD/source bytes did not change.

Fresh retry commands (temporary reviewer targets copied byte-for-byte from this
directory; removed afterward, never production edits):

```
cargo test --offline --locked -p rndr_ui --test reviewer_data -- --nocapture
cargo test --offline --locked -p rndr_ui --test reviewer_field -- --nocapture
cargo test --offline --locked -p rndr_ui --test reviewer_gallery -- --nocapture
```

| Retry log | Pass | Fail | Exit |
|---|---:|---:|---:|
| retry/data.log | 13 | 3 | 101 |
| retry/field.log | 9 | 1 | 101 |
| retry/gallery.log | 5 | 1 | 101 |

Thus 27 controls pass and all five added assertions fail. These repeat existing
maintained controls; do not add them to 708 as unique-test coverage. An initial
combined `... --test reviewer_data --test reviewer_field --test reviewer_gallery
reviewer_ -- --nocapture` stops after the three data failures; that limited run
is `retry/confirmed-probes.log`, not evidence that the other two targets ran.
They were then executed separately as above.

Reproduce all suites using the saved runner (requires cargo on PATH and a
compatible CARGO_TARGET_DIR, just like the existing verification script):

```
bash evidence/data-components-next/independent-combined-review/reproduce.sh /tmp/rndr-independent-review-NEW
```

It refuses existing outputs/targets, copies evidence fixtures, runs all three
targets despite test failures, records exits, removes only its three temporary
targets and exits 101 for confirmed failures. Use a fresh output each time.

## Visual evidence assessment — provenance passes; pixel sign-off BLOCKED

Authoritative capture set:
`evidence/data-components-next/combined-candidate/native-final-4/`.
12 states, two frames each: light, dark, narrow, narrow-inspector,
compact-hc-200, compact-hc-200-inspector, compact-hc-200-bottom,
invalid, readonly, disabled, empty, workflow-bottom. Metal/Apple M4/Parley,
actual native pre-present surface-copy readback, not HTML rendering. Source
manifest binds 131 files; PNG/metadata SHA bindings and successful capture exits
are recorded. `presented_confirmation=false` honestly excludes compositor proof.

Prior independent provenance assessment: `native-provenance-review.json`.
The retry revalidated **all 24 PNG/metadata pairs**, all 131 source bindings and
all eight preserved owner files, with no tracked diff. Receipt:
`retry/final-revalidation.json`. Native receipt SHA256:
`3a68d5ddcbbff791bae25970df89edfa6a8a91323fff9fe8728d5f4bad21e447`.
Prior audit in `audit.log` establishes 3,073 historical evidence files/57,991,081
bytes preserved, plus eight owner reproduction files. Existing tracked evidence
was not rewritten by the retry.

**No independent pixel sign-off.** Prior image-view attempts returned metadata
without inspectable attachments. Retry directly opened native-final-4/light/
frame-2.png; the tool again returned metadata (`vision_available=true`) but no
inspectable pixels in the reviewer context. Tool success is not visual inspection.
The early retry progress assertion that image delivery worked was corrected.
The worker's visual-inspection.json is implementation evidence, not substitute
independent inspection. Geometry tests pass but cannot establish appearance,
clipping, contrast or legibility. No new visual defect is alleged without pixels.
The review decision is complete (changes requested), with visual acceptance
explicitly unresolved due to this tooling blocker. No need to recapture identical
states simply to produce another uninspectable file.

Physical input, live assistive technology, real OS IME, compositor/presentation,
physical DPI and Windows/Linux validation remain open platform limits.

## Owner correction / re-review requirements

1. Correct F1-F4 within the existing undertaking, preserving these reproductions
   as maintained regressions; no unrelated cleanup/Blooey changes.
2. Re-run the five assertions unchanged in intent plus controls: legitimate
   within-property focus retains draft, leaving/window blur cancels without commit,
   accepted earlier Submit preserves later draft, external value/object changes
   still cancel safely, legacy unopted behavior remains deliberately scoped.
3. Repeat the bounded full combined gate on the corrected candidate; retain old
   logs/probes. Refresh source/binary/capture bindings for changed production code.
4. Complete actual independent pixel inspection of the relevant native matrix
   through functioning image delivery. Do not treat metadata as completion.
5. Return corrected candidate and evidence to owner for review before native
   acceptance/integration. This report grants neither.
