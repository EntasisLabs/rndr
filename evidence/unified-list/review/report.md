# Independent unified-list review and corrections

Reviewed candidate **e98048b1abb4b4b4a446b2d69ffac45665976788** against baseline
**0b997b4253c3545786a56a5b91f30bbdae76e6b5** in the same admitted checkout.
This is an independent source/diff review and correction pass, not acceptance,
integration, publication, capture implementation or Markdown implementation.
The original implementer report/logs/manifest in the parent directory are preserved
as historical evidence; they describe the original candidate, not these corrections.
`source-manifest.json` here binds the corrected source and added regressions.

## Findings: three reproduced correctness defects, fixed

1. **Pooled content leaked components across stable keys and renderers.**
   `realize` and renderer replacement retired interaction targets and destroyed
   children of `RowSlot.content`, but left the content entity's own components.
   A renderer is allowed to populate that content entity (the palette itself does).
   A Label/Color from record zero survived rebinding to a different record; other
   caller-owned components could likewise survive. The managed row root is still
   pooled, but its entire content entity/subtree is now replaced before rebinding
   or changing renderers. Overlapping same-key, same-renderer content is retained.
   Regression: `recycled_and_replaced_renderers_do_not_inherit_content_components`.

2. **Immediate canonical focus changes were not reconciled on metadata update.**
   With cached active key zero, `request_focus(row 3)` followed directly by disabling
   row 3 (before the next Input requests pass) fell back to row zero, not the
   documented enabled row at/after the prior position (row four). Mount now adopts
   the actual canonical/logical key before validating active state and calculating
   the fallback position. External focus owners retain their explicit navigation
   key when this list does not own canonical focus.
   Regression: `immediate_canonical_focus_removal_falls_forward_from_actual_row`.

3. **Width-only reflow could leave stale child hit/layout geometry.**
   A row renderer adapting child width to its realized row's width was called again
   after the first placement at the new viewport width, but `realize` returned no
   geometry change because height/window/topology were unchanged. The final
   measure/place was skipped: after 400→800 viewport width, a half-width child's
   Rect stayed 200 instead of 400 in that same propagation. Tracking viewport width
   as well as height now requests final placement for this second reconciliation.
   Regression: `width_only_reflow_remeasures_second_reconciliation_before_hit_geometry`.

`reproductions.log` establishes defects 1 and 2 against the original implementation:
13 existing tests passed and both added reproductions failed. After fixing those,
`reflow-reproduction.log` establishes defect 3 (15 pass, one fail). All reproductions
are retained and pass on the corrected source. No substantive finding was dismissed
merely because the implementer's suite passed. Crate/lifetime documentation was
also corrected: default retained collections coexist with virtual ListView, and
pooling reuses row roots, not arbitrary caller-populated content components.

## Independent source and contract coverage

Read the actual core/UI/palette diffs and implementations, both requested reports,
canonical layout/scroll/hit geometry, request_focus/access/activation dispatch,
transactional and ordinary field/IME processing, row realization/retirement,
retained collection integration, subtree styling and gallery application/tests.

* **Windowing/geometry:** both legacy core List and virtual semantic ListView call
  `list_window`; semantic realization uses core Scroll/layout/clipping/hit testing,
  not another scroll engine. Viewport callbacks are snapshotted and bounded to two
  calls per propagation, with a conditional final placement. New core probes check
  initial/width-resize placement, ordinary topology mutation and callback self-removal/
  despawn without stale callback execution. Virtual tests cover scroll-before-click
  press geometry, release target cancellation, resize/reflow, and deliberate scrolling
  not being undone by keyboard reveal. Retained core and UI regressions were rerun.
  Callbacks must not recurse into layout/focus or mutate their managed model/nodes;
  this is an explicit supported-usage boundary, not runtime reentrancy isolation.
* **FocusTarget:** original target admission precedes delegation; returned targets
  are revalidated. New probes cover disabled original (no callback), disabled/stale
  return (no focus change or cancellation of an unrelated Space arm), successful
  transfer (old arm cancelled), and independent child/AT Focus without inherited
  delegation. Existing UI probes cover invalid external owner fallback, synchronous
  row→search delegation, nested controls and same-key nested focus retention.
* **CapturesKeys/KeyNavigation:** removing the blanket guard is deliberate opt-in,
  not ancestor routing: only the canonical focused entity's explicit hook runs,
  after transactional ownership and before ordinary field editing. New core probes
  exercise ordinary fall-through typing/Space/Enter submission and transactional
  precedence with the same hook installed. Existing field, ownership, chronology,
  acknowledgement, correction and selection suites remain passing. This does not
  prove native OS IME or assistive technology delivery.
* **Virtual lifetime/identity:** stable ItemId selection/logical focus survive
  filter/reorder/replacement/recycle; removal/disabled states and empty metadata
  reconcile. Arms/queued core interaction targets are retired before rebind.
  Nested buttons/checkboxes exclusively own their pointer/keyboard/AT requests.
  Replacing content also prevents stale component/editor state crossing identities.
  Persistent offscreen editor drafts, selection ranges and open popup lifetime/
  exact nested refocus remain explicitly unsupported; durable application state
  must be external and committed/cancelled before recycling. Existing popup anchor
  cleanup is not a promise to keep a recycled editor or popup alive.
* **Actual palette:** it mounts the semantic list, not core List. Existing tests
  independently rerun debounce/stale-query rejection, fuzzy highlights, shortcut
  labels, icon geometry, stable-key filtered icons/reorder, navigation/selection/
  activation, same-batch pointer+typing and Ask draft/dismissal behavior. Two new
  actual-application tests cover IME preedit/commit snapshots (no navigation or
  duplicate commit text) and hover selection, double-click Ask entry, Shift+Enter
  newline and Enter prompt submission with search/prompt ownership retained.
* **Dependencies:** palette adds rndr_ui; core only exports generic hooks/windowing.
  `cargo tree -p rndr_core --edges normal` contains no rndr_ui. No core→UI cycle.

## Exact independent commands and results

All Cargo commands below used ONLY the requested shared target and environment:

```sh
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export CARGO_TARGET_DIR="/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target"
. scripts/data-gallery-env.sh
# jobs=2; incremental=0; dev/test debug=0; offline and locked
```

Environment/toolchain/OS/disk observations: `environment.log`.
Counts are **test executions**, not sums of unique tests across overlapping runs.

| Command | Independent result / log |
|---|---|
| `cargo test --offline --locked -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p palette` | **742 passed, 0 failed/ignored**, 99 result summaries including doctests; `final-full-gate.log` |
| `cargo test --offline --locked -p rndr_ui --test virtual_list --test virtual_gallery -- --nocapture` | **17 passed** (16 list + one gallery matrix), overlaps full gate; `final-focused.log` |
| `cargo test --offline --locked -p palette` | **12 passed**, overlaps full gate; `palette.log` |
| `cargo test --offline --locked -p rndr_core --test unified_viewport` | **3 passed**, overlaps full gate; `core-probes.log` |
| `cargo test --offline --locked -p rndr_ui --test scroll_installation --test collections --test collection_chronology --test selection --test selection_corrections --test selection_independent_review --test field_transactions` | **74 passed**, overlaps full gate; `scroll-selection.log` |
| `python3 -m unittest discover -s tools -p 'test_observe*.py' -v` | **20 passed**; `python.log` |
| `cargo fmt --all --check` | pass; `fmt.log` |
| `cargo clippy --offline --locked -p rndr_core -p rndr_ui --lib -- -D warnings` | pass; `clippy-libs.log` |
| `cargo clippy --offline --locked -p rndr_core --test unified_viewport -- -D warnings` | pass; `clippy-core-probes.log` |
| `cargo clippy --offline --locked -p rndr_ui --test virtual_list --test virtual_gallery -- -D warnings` | pass; `clippy-virtual.log` |
| `cargo clippy --offline --locked -p palette --all-targets -- -D warnings` | pass; `clippy-palette.log` |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p rndr_ui --no-deps` | pass; `ui-docs.log` |
| `cargo build --offline --locked -p rndr --features observe,ui --example ui_gallery --example data_gallery --example data_editors_gallery --example selection_gallery --example virtual_list_gallery` | all five cached native gallery binaries build; `gallery-build.log`; **not a visual check** |
| `cargo tree --offline --locked -p rndr_core --edges normal` | no normal rndr_ui dependency; `core-dependencies.log` |

The full gate includes core List tests, scrolling hygiene (`a1_*`/`a2_*`), retained
ListView/TreeView/chronology, data/inspector transactions, popup/selection correction
suites, headless paint tests and renderer regressions. Eight maintained tests were
added in this review (3 virtual list + 3 core + 2 actual palette); 734→742 full-gate
executions is consistent with that addition, not an independent uniqueness metric.

Known baseline tooling gaps were independently confirmed, not cleaned up:

* `cargo clippy --offline --locked -p rndr_core -p rndr_ui --all-targets -- -D warnings`
  exits 101 at existing needless borrows in `crates/rndr_core/tests/a1_wrapped_hug.rs`
  lines 79/125; `clippy-all.log`.
* `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p rndr_core -p rndr_ui --no-deps`
  exits 101 on four existing unresolved Fired links in `crates/rndr_core/src/button.rs`;
  `core-docs-known-gap.log`.
* `git diff --exit-code 0b997b4253c3545786a56a5b91f30bbdae76e6b5 -- crates/rndr_core/tests/a1_wrapped_hug.rs crates/rndr_core/src/button.rs`
  exits zero; those files are unchanged (`unchanged-baseline-tooling.log` is empty
  because there is no diff). The source manifest also records matching hashes.

## Bounded work and remaining validation boundaries

Metadata mount/replacement copies names and builds the ID index: **O(n) time and
storage**. Steady realization/layout/style work attributable to this list is bounded
by the realized viewport window, not logical metadata length (the rest of the app's
normal world traversal is not claimed constant). Shared Arc metadata is not cloned
per frame. Independent 100,000-record tests retain **35 Layout entities**, the same
steady-frame entity identities and **400 callbacks over 20 steady frames**. The exact
new diagnostic timings are in `final-focused.log`; they are not a native/cold-start/
comparative performance benchmark. Content entities can be newly allocated on rebind
or renderer replacement after the fix. Bounded temporary Vec/Map/set/string work
still allocates on the heap; unchanged steady entities do **not** establish global
heap-allocation freedom. No allocator instrumentation or speed guarantee is claimed.

Disk was about **2.5 GiB free** at entry and **2.4 GiB** after cached validation.
No alternate target directory, broad cleanup, other checkout/repository mutation,
credentials, spending, push/publication or history alteration was used.

**No native window pixels were captured or inspected in this review.** All input,
IME and accessibility evidence here is headless/synthetic; no physical mouse/key/
scroll, OS IME, screen-reader/AT, Windows/Linux or MSRV certification is asserted.
Only realized rows expose accessibility entities; full offscreen AT collection
position/count/navigation remains outside the implemented contract. Variable-height
virtualization and persistent offscreen editor/popup lifetimes remain unsupported;
f32 total-height precision remains a documented limit.

The three established defects are corrected with maintained regressions, and the
independent cached gates above pass subject to the unchanged tooling gaps. Final
sealed review, implementation acceptance and native integration remain with the
owner. Nothing is approved or integrated by this review/correction commit.
