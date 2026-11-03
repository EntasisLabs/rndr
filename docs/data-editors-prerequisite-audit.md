# DataTable / PropertyInspector — blocked prerequisite audit

**Status: milestone not delivered.** This is an audit and exact reproduction
handoff, not an implemented component release. The existing ListView/TreeView
milestone remains accepted and unchanged. Product source in this undertaking is
byte-for-byte the admitted `57c30a3242efcea7d54a6083251575ac04f1c603` baseline.

The request explicitly asks for an exact essential interaction/API blocker if
either component cannot be built safely on the canonical mechanisms. The
blocking prerequisite is the **single-line editor transaction boundary**, needed
for PropertyInspector's ordered typed draft/change/commit/cancel semantics.
DataTable does not have a confirmed essential blocker; it is not silently
substituted for the requested two-component milestone.

## Audit: reuse, not an application-specific component system

- `docs/data-components.md` defines the existing keyed collection contracts:
  stable IDs, bounded supplied data, passive custom content, single/no selection,
  focus distinct from selection/activation, canonical scrolling and accessibility.
  Its roadmap statement remains accurate: DataTable/PropertyInspector are not
  implemented in the public API by this handoff.
- `crates/rndr_ui/src/collections.rs`: `ListView`, `TreeView`, `ViewHandle`,
  `RowSlot`, `ItemId` and collection request events are the appropriate reusable
  row interaction foundation. Programmatic disclosure changes have scoped
  activation cancellation; existing chronology must not be replaced by another
  dispatcher. Table columns/cell slots could be composed above this foundation.
- `controls.rs`: keyed command controls, the bounded host intent queue,
  release activation, `ControlHandle::validate`, names, disabled/read-only states
  and theme resolution are reusable. Control/surface/collection identity checks
  would also need to recognize new data components, in both conflict directions.
  A boolean or enum property could compose existing controls with explicit
  draft and commit requests; it does not require reflection or business logic.
- `composition.rs`: use existing Layout/Parent/Children and responsive workspace
  conventions; narrow tables need bounded horizontal overflow, not invented
  automatic spreadsheet behavior. Column width updates can be declarative;
  a real column drag-resize lifecycle is not supplied by ListView. No resizing
  affordance or implementation is being claimed here.
- `style.rs`, `tokens.rs`: opt-in UiStyle/UiRecipe with density, type scaling,
  semantic colors and authored override ownership. New components must use
  these contracts rather than hard-coded gallery appearance.
- `crates/rndr_core/src/access.rs` and `crates/rndr_render/src/a11y.rs`: existing
  names, roles, selected/focused state, values and honest supported actions are
  reusable. Table/row/header/cell projection and header relationships would be
  an additive semantic extension; there is no native table certification here.
- `crates/rndr_core/src/field.rs`: TextField is the real existing single-line
  editor over Label/Caret and the shared typing engine, including selection,
  clipboard, history and IME plumbing. It is a draft editor, not a property model.
  `caret.rs::Field` is a pane-oriented editor, not a generic numeric property
  control. `editor.rs` is a multiline buffer editor, not a reason to introduce
  filesystem/document dependencies into the inspector.
- `editor.rs::Buffer::{insert_str,delete_range}` are raw buffer changes, not
  history-aware property commits. This work never used them as undo-aware edits,
  never synthesized typing to seed/reset a draft, and makes no app-wide undo
  claim. An inspector may discard/remount a draft editor on identity change;
  private typing-history internals should not become public model ownership.

## Essential gap: edits and commit snapshots are not chronological

`Chrome::apply_input` invokes `apply_generic_input` before `apply_fields`.
GenericInteraction routes the queued key edges, including Tab focus traversal.
TextField's later `edit_label` then edits the retained **whole** key batch against
its current focused entity. It computes `wants_submit` with `any(Enter)` and
emits a single Submitted with the **after-batch** value.

References on the admitted baseline:

- `crates/rndr_core/src/chrome.rs:68–74` — ordering of generic input and fields.
- `crates/rndr_core/src/interaction.rs:616–636` — CapturesKeys skips the
  composite KeyNavigation hook; plain non-editor keys can traverse focus first.
- `crates/rndr_core/src/field.rs:327–379` — boolean submission admission,
  whole-queue editing, final-value emission, missing repeat filtering.

The public tests use `App + UiPlugin`, real `Input.keys` KeyEvents and
`rndr_paint::frame` scheduling. They do not merely set a model or call a setter.
The observed acceptance failures are:

| Chronological input | Required boundary | Observed baseline |
|---|---|---|
| `1`, Enter, `2` in one frame | submit `"1"`; later draft may be `"12"` | submits `"12"` |
| `1`, Enter, `2`, Enter | two snapshots `"1"`, `"12"` | one snapshot `"12"` |
| Enter, then repeat Enter in next frame | no second transaction for held Enter | emits another Submitted |
| `a`, Tab, `b`, without CapturesKeys | first field `"a"`; second `"b"` | first `""`; second `"ab"` |
| CapturesKeys field + KeyNavigation Escape hook | ordered cancel request reaches owner | hook called zero times; queue consumed |

These are **prerequisite contract gaps for the new milestone**, not a claim that
ListView/TreeView regressions failed. In particular the Escape observation
reflects the current intentional separation of CapturesKeys and navigation;
using that hook as a second editor dispatcher would be the wrong fix.

Post-frame parsing of Submitted cannot recover an earlier snapshot or multiple
collapsed boundaries. Replaying Input in the inspector would create the second
dispatcher prohibited by the request. A change listener after the whole batch
cannot reliably distinguish Escape, blur and Enter positions. Debouncing
Submitted would hide one symptom, not restore chronology.

## Required canonical prerequisite, not claimed implemented

A coherent solution should extend the existing canonical loop with an opt-in
single-line editor transaction seam, while reusing the same typing engine:

1. Apply each admitted real queued key against its focus target **at that point**.
2. Deliver change, submit and cancel boundaries with precise draft snapshots;
   reject repeat/released/synthetic submit/cancel edges as appropriate.
3. Leave Tab traversal, pointer/AT focus and reveal under canonical ownership.
4. Define IME preedit/commit, clipboard, selection, undo and read-only policy;
   an Enter during composition must not accidentally commit a property.
5. Make blur/identity-change/reset/unmount semantics explicit and ensure no
   previous object's draft or typing history survives into another object.
6. Verify mixed pointer/key/AT scheduling and scoped cancellation, preserving
   unrelated controls, the six original reproductions and 12 maintained tests.

An opt-in prototype passed five isolated key-boundary tests during this turn.
That is feasibility evidence, **not** proof of an accepted lifecycle contract or
complete milestone. The full lifecycle and component experiment was not verified
and was removed from production source. Its final unverified artifacts are
honestly preserved under `unaccepted-experiment/`; they must not be applied as a
candidate. This blocker is repairable engineering work, not a claim of platform
impossibility or a request to waive the requirements.

## Exact reproduction and verification

From this isolated checkout:

```sh
bash scripts/reproduce-property-editor-blockers.sh evidence/data-components-next/fresh-editor-gap
```

Use a new directory. The script refuses existing output/test fixtures and less
than 2 GiB free disk. It uses the specified compatible target cache by default,
`scripts/data-gallery-env.sh`, offline/locked builds, two jobs, no incremental
and no debug info. It temporarily installs the preserved integration fixture
and removes only that exact worker-created fixture on exit. The normal shipped
suite is not contaminated with knowingly failing/ignored component tests.

The script exits **0 only when all five known failures reproduce**; the actual
acceptance test binary exits **101**. Do not report this as five passing tests.
The fresh `reproduction/` contains build JSON, logs, source SHA-256 manifest and
an exact test binary SHA-256/path/command receipt. These are scheduled synthetic
input tests, not physical keyboard, IME or assistive-technology validation.

Full recorded commands and results are in
`evidence/data-components-next/blockers/REPORT.md`.

## Outstanding milestone scope

Neither public component API, table sorting/selection/header interactions,
property parse/range/object reconciliation acceptance tests, interactive joint
gallery, native screenshot matrix nor component performance costs are delivered.
There are no new screenshot/binary/capture claims to inspect. All those requested
gates remain open; the original gallery and historical evidence remain intact.
No Windows/Linux, physical input, real IME, live screen-reader certification,
publishing, deployment or integration is claimed. Owner review/integration
responsibility remains unchanged.
