# Codebase maintainability: assessment and refactoring plan

Status: principal accepted R0–R2 (numbered steps 1–3) and requested their commit.
Verification is recorded below. R3–R5 (numbered steps 4–6) are now authorized
as the next independently reviewable implementation slices.
Assessed: 2026-11-06, against `b042b429e5365b220a3b098b287fa78a6967ea73`.

## Goal and scope

Make the code easier to read, navigate, and safely change after the editing epic.
Prioritize descriptive names, cohesive functions, shared lifecycle mechanisms,
and explicit semantic rules. Preserve behavior, public APIs, ownership, event
ordering, source fidelity, accessibility, and component appearance.

This is a first-pass assessment, not an exhaustive audit or a defect report.
Workspace-wide file/function size screening was followed by direct inspection
of editing and retained-UI hotspots. Rendering/paint hotspots were screened but
need a separate focused read before prescribing changes. That initial assessment
was documentation-only; the subsequent authorized R0–R2 implementation and fresh
verification are recorded separately below.

## Evidence

Line references below are against the assessed commit, not permanent anchors.
Approximate function spans came from a lexical brace-counting script (not a Rust
parser or cyclomatic/cognitive complexity measurement). File sizes include
inline tests and documentation; generated data and large dispatch matches must
not automatically be treated as poor design.

| Location | Observed issue | Proposed direction |
| --- | --- | --- |
| `rndr_core/src/structured_native.rs:720`, `apply_key` (~261 lines) | Undo/redo, table delegation, atomic copy/navigation/deletion, structural keys, vertical motion, scratch text interpretation and transaction publication share one function. | Preserve the dispatch order in a short coordinator; extract cohesive handlers that explicitly distinguish handled from unhandled. Table handling is already extracted: extend that pattern. |
| `rndr_core/src/structured.rs:2188`, `apply_operation` (~371 lines) | Some enum arms already delegate; others contain substantial editing algorithms. Uses `s`, `a`, `z`, `b`, `p`, and table editing is called with paired booleans. | Keep exhaustive operation dispatch; move substantive algorithms to model-local functions. Name ordered endpoints and replace ambiguous operation flags with private semantic enums where justified. |
| `rndr_ui/src/selection.rs:258`, `mount` (~242 lines) | Validation, keyed reuse, value normalization and widget subtree creation are interleaved. | Separate preflight, retained reuse/new construction, reconciliation and projection; make mutation boundaries visible. |
| `rndr_ui/src/data.rs:327` and `:1326`, `mount` (~314/~354 lines) | Large table/inspector mount functions surfaced by screening. Helpers at `:147-183` combine lookup, tree traversal, interaction cleanup and child ordering. | Inspect each full mount and its tests before extraction. Candidate boundaries: shared lifecycle mechanics versus table/inspector-specific schema and reconciliation. |
| `rndr_ui/src/collections/virtual_list.rs:300`, `realize` (~223 lines) | Geometry, focus preservation, retirement/recycling, row construction, child ordering, accessibility and callbacks share one pass. Locals such as `w`, `v`, `h`, `s`, `l` obscure roles. | Name world/view/viewport/scroll/layout explicitly; extract phases without changing focus parking, callback order, or resize-only reveal behavior. |
| `rndr_ui/src/controls.rs:202,233`; `data.rs:147,155`; `selection.rs:241,258`; `source_tools.rs:40` | Repeated parent checks, keyed-child lookup, and spawn/parent operations. Existing shared identity lookup lives under controls and knows about multiple sibling modules. | Pilot a small private lifecycle module. Preserve per-family ownership semantics; do not create a catch-all utility module or duplicate the current identity mechanism. |
| `rndr_core/src/editing.rs:387-450` | `SourceSession::admit` already centralizes readonly/composition/backpressure/revision capacity. `commit` separately checks document/revision and exact scalar-boundary/overlap rules. | Treat this as a useful existing boundary. Extract batch validation only if it improves clarity/reuse, preserving refusal ordering and all-or-nothing semantics. Do not eliminate safety checks to reduce branch count. |
| `rndr_core/src/structured_native.rs:160-180`, `command_ready` | Commands must consider both captured composition and newly delivered focused preedit. | Keep temporal admission rules explicit. An attachment check is not permission to edit, and a previously successful check is not durable authority. |

Broader screening found large modules including markdown (~4,205 lines), renderer
scene (~3,058), structured (~3,034), editor (~2,263), UI data (~1,964), and source
editing (~1,777). These sizes are navigation signals only. GPU setup, text shaping,
painting, generic input and chrome also surfaced in function screening. They are
later audit candidates, not pre-approved rewrite targets.

## Design rules

1. **Names convey domain roles.** Prefer `world`, `document`, `selection`,
   `block_index`, `start_position`, `end_position`, `viewport_height`, and
   `scroll_offset`. Short mathematical coordinates and tiny iterator bindings
   remain reasonable. Rename within a coherent refactor, not by global regex.
2. **Reduce mixed responsibilities, not the count of `if`s.** Early refusals,
   exhaustive enum matches and local boundary checks often improve clarity.
   Extract meaningful operations, not arbitrary chunks to hit a line limit.
3. **Share mechanisms only when contracts agree.** Parent validation, keyed
   lookup and basic node construction are initial candidates. Destruction,
   borrowed children, detached popups, focus retirement and interaction cleanup
   need explicit ownership analysis before sharing a lifecycle helper.
4. **Keep policy with its owner.** Source and structured models retain their
   own validation/history rules. UI projects state and emits requests. Native
   adapters translate input without becoming a second document authority.
5. **Use types to expose semantic choices.** Prefer named ranges/positions,
   direction/axis/edit-kind enums and explicit handler outcomes over ambiguous
   boolean pairs and sentinel return values. Do not wrap every primitive.
6. **Validation precedes side effects.** Reusable checked data must be tied to
   the identity/revision/incarnation that made it valid. Recheck live readonly,
   composition and backpressure at mutation time. Never cache permission.
7. **Keep dependencies one-way.** New UI internals may depend on core; core
   must not depend on UI or renderer. Keep helpers private until actual public
   reuse and a stable contract justify exposure.
8. **Preserve explicit differences.** Source bytes, display bytes, grapheme
   positions, structural IDs and ECS identities are not interchangeable.
   Plain-source and structured documents must not be forced into a universal
   editor abstraction. Avoid generic `validate_everything` / `helpers.rs` bags.

## Sequenced work

### R0 — Characterization and review baseline

- For the chosen slice, list callers, current helper boundaries and relevant
  tests; record refusal order, allowed side effects and schedule/callback order.
- Add characterization coverage where behavior is currently implicit. Assert
  refused operations leave content/history/events/selection/focus unchanged as
  required by that operation (not a blanket assumption about every refusal).
- Establish current targeted test/lint baseline before changing implementation.
- Use complexity/size reports as review prompts, not pass/fail targets.

Exit: a bounded behavioral contract and reproducible test commands for R1.

### R1 — UI lifecycle pilot (recommended first implementation)

- Limit the first slice to controls/selection parent validation and keyed-child
  lookup, with descriptive naming in touched code.
- Inventory identity consumers first; move or delegate the existing identity
  mechanism rather than introducing a second registry. A candidate private
  module is `rndr_ui::lifecycle`; final location follows dependency review.
- Preserve current differences until tests establish whether they are equivalent;
  standardizing different failure behavior is a separate change.
- Test invalid parents, same-key reuse, cross-family key conflicts, stale
  handles and unchanged subtree/state after refusal.
- Do not include subtree destruction or a workspace-wide module reshuffle in
  this pilot. Extend to data/collections only after the pilot proves useful.

Exit: two real consumers, one explicit contract, no public API or visual change,
less duplicated reasoning rather than merely fewer lines.

### R2 — Retained UI reconciliation

- Break selection mounting into preflight, existing-state resolution, subtree
  creation and projection/reconciliation. Apply the same approach to table and
  inspector mounts only after inspecting their specific contracts.
- Extract virtual-list geometry/window calculation separately from ECS mutation;
  keep focus retirement and renderer callback sequencing visible.
- Reuse proven lifecycle helpers; keep collection/selector/data policies local.

Exit: shorter coordinators with identifiable phases; retained identity, focus,
keyboard/popup behavior, virtual recycling and accessibility tests still pass.
Visual comparison is required if construction/projection paths are changed.

### R3 — Editing dispatch and semantic operations

- Refactor `structured_native::apply_key` incrementally: history commands,
  existing table handler, node-selection actions, structural text actions,
  vertical motion, then ordinary text interpretation/publication.
- Use an explicit handled/unhandled result; an unhandled event is not an error
  or an already consumed event. Preserve current precedence exactly.
- Keep `apply_operation` exhaustive while extracting large operation algorithms.
  Name endpoints and table edit modes. Keep transactional atomicity intact.
- Reuse `SourceSession::admit` rather than adding a competing admission layer.
  Consider checked batch data only where it removes repeated reasoning safely.

Exit: equivalent event consumption, copy/cut behavior, history and revision
changes, no-op semantics, atomic refusals and composition/readonly handling.

### R4 — Module navigation and second-pass rendering audit

- Split by responsibility behind stable existing module paths/re-exports where
  useful: document model vs native attachment/projection vs input routing;
  data table vs inspector; structured text vs tables vs containers/atoms.
- Move tests only if it improves navigation; do not use test relocation to claim
  reduced implementation complexity. Keep behavioral refactors and file moves
  in separate reviewable commits when possible.
- Directly inspect renderer/paint/markdown hotspots, callers and performance
  fixtures before proposing their own slices. A long match or GPU descriptor
  setup may be clearer left intact.

Exit: clearer ownership and dependency map, stable feature boundaries, no new
cyclic helper dependencies or public API churn.

### R5 — Lightweight regression prevention

- Write a short contributor guide: naming, validation ownership, typed semantic
  choices, lifecycle rules and examples from accepted refactors.
- Keep formatting and existing strict production Clippy gates. Trial scoped
  complexity/argument-count warnings if useful; first record existing debt,
  avoid workspace-wide warning floods and broad lint suppression.
- Review changed hotspots for responsibility count, policy duplication and
  dependency direction. No arbitrary requirement that all functions fit N lines.

## Verification and review protocol

Each implementation slice is independently reviewable and behavior-preserving.
Do not mix feature additions, deliberate bug fixes, global renaming and module
moves into one diff. If characterization finds a real bug, document it and make
its behavior change explicit rather than silently changing it during extraction.

Relevant existing coverage includes core `editing_compatibility`,
`source_session`, `source_lifecycle`, `source_multiview`, `source_multiselection`,
`source_extensions`, `projection_contract`, `projection_custody`,
`source_projection`, `ime`, editor input/focus tests and structured/table tests;
select the exact UI lifecycle/control/collection/data suites after reading them.

For each slice: run affected tests and added characterizations, formatting,
whitespace checks and strict production Clippy for touched crates. At cross-cutting
milestones run the broader core/UI/paint/render/example regression set, facade
compatibility, feature-off/on builds and public rustdoc gates. Retain established
cost probes when touching allocation/snapshotting/virtualization/shaping paths;
functional tests alone do not prove unchanged performance.

Compare invariants and failure behavior, not just the green total. The E8 receipts
are historical baseline evidence, not a fresh pass for a refactor.

## R0–R2 implementation candidate — 2026-11-06

The principal approved numbered steps 1–3 after reviewing the assessment. One
bounded peer owned the data-table/inspector slice; the host reviewed that diff
and integrated it with the lifecycle/selection/virtual-list changes. No peer
fan-out, deliberate behavior fix, public API change, new dependency, module
reshuffle or editing-dispatch change is included. The principal subsequently
accepted this candidate and requested its commit, then authorized R3–R5.

### R0: characterized contracts

- `tests/lifecycle_contract.rs`: parent validation before props/identity for
  controls/selectors, parent-scoped retained keys, stale generations and missing
  slots, no allocation/focus/content changes for the tested refusals, stable
  selector option IDs across reorder, normalization and open-popup refusal.
- `tests/data_reconciliation_contract.rs`: data props before parent validation;
  family conflict and retained-handle refusal; table composition in supplied
  row/column order; inspector custom callbacks in BTree property-ID order;
  object-change cancellation before custom projection and frame-bound delivery.
- `tests/virtual_reconciliation_contract.rs`: renderers see the newly published
  canonical metadata and rows, in presentation order. This differs deliberately
  from table/inspector callbacks, which see the previous published managed model.
- Two pure virtual geometry tests include a 1,500-case clamp/reveal/window matrix
  against the original algorithm, plus explicit resize-reveal versus deliberate
  scrolling. Existing input, focus, IME/draft acknowledgement, popup and recycling
  suites remain the behavioral authority; the new tests supplement them.

Existing controls/selection/data suites passed before their implementation
changes. The three new lifecycle tests additionally passed before pilot wiring.
Immutable-HEAD comparisons cover projection as well. The virtual callback test
was added after extraction and verifies its original publication point rather
than claiming a pre-extraction execution.

### R1: private lifecycle mechanism

`src/lifecycle.rs` owns Layout-parent admission and first direct keyed-child
lookup. Controls and selection are the two mount consumers. Lookup preserves
child order and existing identity precedence; callers retain family admission,
slot validation, error types and mutation timing. Controls return
`IdentityConflict` for a cross-family collision while selection retains its
historical `InvalidProps` response. No universal validator is introduced.

The identity-consumer inventory includes the public capture adapter, passive
surfaces, source tools, editing wrappers, collections, virtual lists and data.
`controls::key_at` remains a compatibility delegate for existing private callers;
`controls::local_key_at` only resolves its own control family. The aggregate
lookup calls that local function, never the delegate, so it does not recurse.
There is no second registry or retained identity tree. Public capture identity
still recognizes structured surfaces; mount admission is not silently broadened
to that observation-only contract. Passive mount conflict checks remain separate
so passive components do not conflict with themselves.

### R2: explicit retained reconciliation phases

- Selection: admission/normalization, retained state or owned subtree resolution,
  option reconciliation with focus parking and retirement, then publication and
  refresh/popup policy. Detached popup ownership is unchanged.
- Data table: resolve, retire removed entries, project chrome, reconcile headers,
  reconcile rows/cells with caller composition, project body, publish.
- Inspector: resolve, retire fields/groups, project groups, reconcile retained
  drafts/pending acknowledgements, create missing editors, project/order fields,
  invoke custom callbacks, publish. Validator identity, accepted pending index,
  local caret/IME/history preservation and cancellation timing remain explicit.
- Virtual list: pure geometry policy, ECS window update, focus-aware retirement
  and recycling, spacer/order/semantic projection, publication, row callbacks,
  final style resolution. Width-only reflow and resize-only focus reveal remain
  distinct. No new per-frame clone or whole-collection scan was introduced.

Names were improved around coordinators, selection reconciliation, virtual
geometry and inspector draft policy. This is not a workspace-wide rename:
older local construction/input names remain for later focused slices. Longer
cohesive subtree construction helpers are not split solely to hit line limits.

### Fresh verification

Local commands/logs and generated comparisons are under ignored
`target/maintainability/`; generated captures are not source changes.

```sh
cargo test -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render
cargo test -p source-session
cargo test -p rndr --no-default-features --test editing_compatibility
cargo test -p rndr --no-default-features --features ui --test editing_compatibility
cargo clippy -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session --lib --bins -- -D warnings
cargo check -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
cargo check --workspace
RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_core -p rndr_ui --no-deps
cargo fmt --all --check
git diff --check
```

The broad core/UI/paint/render run passed **1,068 tests, zero failures, five opt-in
probes ignored**, across 142 suites including doctests. The existing 100,000-row
virtual-list test retains bounded entity/callback and steady-state allocation
assertions. Ignored release-scale probes are not claimed as new performance
certification.

`tests/reconciliation_equivalence.rs` provides optional paint/access dumps using
`RNDR_RECONCILIATION_CAPTURE_DIR`. The same test is compiled against an immutable
`git archive` of assessed HEAD and the candidate, sharing unchanged core/paint
paths; `diff -qr` compares their output. The expanded matrix has **30 scenes**:
standalone single/multi and inspector popups genuinely opened by canonical
activation, plus default/empty/readonly/disabled/invalid states, table/inspector
and 100,000-row virtual galleries, across light, narrow-dark, and compact
high-contrast 200%-text presets. Paint dumps retain their existing half-point
snapshot rounding; access dumps retain full Debug geometry, semantics, identity
and links. Expanded popup coverage revealed baseline nondeterminism: two runs
of unchanged HEAD can enumerate detached root groups in different orders.
`canonical_access` normalizes node storage order and stable top-root-group
order **only in the comparison harness**. It preserves sibling order within
each hierarchy, every non-window children vector, all IDs and all values.
This pre-existing enumeration behavior was not changed in production; it is
an observation for the later accessibility/structure audit, not a hidden fix.
This is reproducible headless projection comparison, not native presentation,
OS accessibility or physical keyboard/mouse/IME certification.

Final gates passed: source-session example **20/20**; isolated facade compatibility
**2/2 UI-off** and **3/3 UI-on**; strict production Clippy; core/UI/paint feature-off
check; workspace check; warning-free core/UI rustdoc; formatting and whitespace.
The example run is separate from the 1,068-test broad total; repeated fixture
and comparison executions are not added to that total. The expanded 30-scene
canonicalized paint/access comparison matches the original HEAD output.

Four additional **real offscreen GPU** PNG comparisons on the Apple M4 adapter
also matched byte-for-byte: an open multi-select popup, narrow-dark selectors,
table/inspector, and a 100,000-item virtual gallery. These use disposable copies
of the original/candidate UI crate and `pixel_capture.rs` under ignored
`target/maintainability/`; their temporary manifests add the renderer dependency
only to the disposable harness, not the repository dependency graph. The
candidate harness uses a distinct disposable package version to prevent Cargo
from reusing the original standalone package's same-identity artifact in a
shared target directory; its log proves candidate-source recompilation. These are
synthetic offscreen fixtures using the fixture's headless text path, not native
OS presentation or native font-backend certification. PNGs, raw comparisons and
logs remain generated evidence, excluded from source commits.
