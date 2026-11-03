# Unified semantic list + real palette candidate

Baseline: **`0b997b4253c3545786a56a5b91f30bbdae76e6b5`**. Isolated branch:
`worktree/rndr-unified-virtualized-list-and-palette-migration`. This report and
`source-manifest.json` are committed with the candidate; the full candidate commit
is returned to the owner. Nothing is approved/integrated, pushed or published.

## Implemented and changed API

* `ListView::mount_virtual`, `VirtualViewHandle`, `VirtualCompose` and optional
  `VirtualListPolicy` in `rndr_ui::collections`. Uses existing Item/ItemId,
  ViewProps, RowSlot, SelectionMode and CollectionEvent contracts. A distinct
  handle makes the ephemeral/offscreen-row lifetime explicit rather than
  pretending the retained entity-addressable handle can keep every subtree alive.
* Core `list_window` is shared by legacy core List and semantic virtual ListView.
  Core `ViewportRows` runs bounded content reconciliation in canonical layout,
  including scroll-before-hit-geometry refresh. The engine still uses existing
  Scroll, layout, clipping, release admission and focus; no parallel scroll/layout
  engine and no core dependency on UI. See dependency-boundary.log.
* Core `FocusTarget` admits and revalidates explicit focus delegation on only the
  requested entity. `KeyNavigation` may explicitly opt a CapturesKeys field into
  forwarding unconsumed navigation before ordinary editing; transactional editor
  ownership still runs first. Nested controls never inherit either hook.
* The actual palette now installs UiPlugin and mounts semantic virtual rows with
  stable command keys. Its hover, navigation-selection and double-click behavior
  are explicit opt-ins; passive rows synchronously delegate to its search field,
  preserving printable input even with a pointer/key in one batch. Fuzzy matching,
  debouncing/stale reply rejection, shortcuts, icons, Ask flow, scrolling and
  reveal retain real application regression coverage. Rendering uses direct
  stable-key command/icon lookup rather than filtered-index identity.
* Default semantic behavior remains: navigation focuses without selecting,
  Enter activates without selecting; canonical click/Space/AT Activate selects
  without host activation. Button/Checkbox children exclusively own their
  pointer/keyboard/AT activation and focus. Retired subtrees lose armed targets
  before rebind; overlapping same-key subtrees survive reorder/replacement.
* New interactive `virtual_list_gallery` with 100,000 synthetic records and
  genuinely operating Inspect/Checkbox controls; stable-key model acknowledgement,
  filter/reverse/replace/disable/jump actions and shared headless gallery tests.
  All five gallery binaries are included in the CI build group.
* Docs: docs/virtual-list.md, retained data component guidance, crate/root README,
  and API lifetime/selection docs. Existing retained ListView/TreeView/data APIs
  remain compatible and their tests were not edited.

## Reproduction environment

All Cargo verification used the existing shared cache, no clean or alternate
heavy target cache, offline/locked dependencies and scripts/data-gallery-env.sh:

```sh
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export CARGO_TARGET_DIR="/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target"
. scripts/data-gallery-env.sh
# CARGO_BUILD_JOBS=2, CARGO_INCREMENTAL=0,
# CARGO_PROFILE_DEV_DEBUG=0, CARGO_PROFILE_TEST_DEBUG=0
```

Tool versions, OS and disk observation are recorded in environment.log. No
source/evidence/cache cleanup was performed by this implementation. Free space
was initially about 3.6 GiB and remains about 2.5 GiB after shared-cache reuse.
The owner's historical cleanup manifest/logs/screenshots were not changed.

## Exact commands and results

These are **execution counts, not distinct-test totals across overlapping runs**.

| Command | Result / saved log |
|---|---|
| `cargo test --offline --locked -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p palette` | **734 passed**, 0 failed/ignored, including doctests; full-tests.log |
| `cargo test --offline --locked -p rndr_ui --test virtual_list --test virtual_gallery -- --nocapture` | **14 passed** (13 list + one actual gallery matrix), overlaps full gate; focused-tests.log |
| `cargo test --offline --locked -p palette` | **10 passed**, overlaps full gate; palette-tests.log |
| `python3 -m unittest discover -s tools -p 'test_observe*.py' -v` | **20 passed**; python-tests.log |
| `cargo clippy --offline --locked -p rndr_core -p rndr_ui --lib -- -D warnings` | pass; clippy.log |
| `cargo clippy --offline --locked -p rndr_ui --test virtual_list --test virtual_gallery -- -D warnings` | pass; clippy.log |
| `cargo clippy --offline --locked -p palette --all-targets -- -D warnings` | pass; clippy.log |
| `cargo clippy --offline --locked -p rndr --example virtual_list_gallery -- -D warnings` | pass; clippy.log |
| `cargo build --offline --locked -p rndr --features observe,ui --example ui_gallery --example data_gallery --example data_editors_gallery --example selection_gallery --example virtual_list_gallery` | all five actual binaries built; gallery-build.log |
| `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p rndr_ui --no-deps` | pass; ui-docs.log |
| `cargo tree --offline --locked -p rndr_core --edges normal` | no rndr_ui normal dependency; dependency-boundary.log |

Broader tooling is **not wholly green**, and unrelated source was not modified:

* `cargo clippy --offline --locked -p rndr_core -p rndr_ui -p palette --all-targets -- -D warnings`
  stops at existing `needless_borrows_for_generic_args` in
  `crates/rndr_core/tests/a1_wrapped_hug.rs:79,125`; broad-clippy-known-gap.log.
* `RUSTDOCFLAGS='-D warnings' cargo doc --offline --locked -p rndr_core -p rndr_ui --no-deps`
  stops at four unresolved existing `Fired` links in
  `crates/rndr_core/src/button.rs`; docs.log. UI-only strict docs pass above.
* `git diff --exit-code 0b997b4253c3545786a56a5b91f30bbdae76e6b5 -- crates/rndr_core/tests/a1_wrapped_hug.rs crates/rndr_core/src/button.rs`
  confirms both failure source files are unchanged; dependency-boundary.log.

## What the regressions actually establish

The new tests cover key identity across filtering/reorder/replacement/recycling;
removed/disabled keys and empty data; invalid input atomicity/key collisions;
separate selection/focus/activation; retired keyboard arms; nested Button and
Checkbox pointer/keyboard activation with no parent bubbling; same-key nested
focus retention/disabled ancestry; explicit external focus delegation and
IME-owned forwarding; palette opt-in behavior; press geometry from a simultaneous
scroll/click **before** release; keyboard reveal versus subsequent deliberate
scroll; and resize/reflow of a focused bottom row in the actual gallery.

The actual palette tests additionally prove there is **no core List instance**,
search-field navigation/activation remains functional, command keys survive
filtered reordering, pointer+printable input in one batch still types in search,
and a filtered command's icon uses its stable key rather than its new index.
The prior palette tests still cover debouncing/stale results, fuzzy match ranges,
Ask draft/dismissal behavior, shortcut labels and image geometry.

The unchanged full gate includes core List, retained ListView/TreeView, scrolling
hygiene, data editor/selection correction suites, paint snapshots and renderer
regressions. This is not a claim of new native input/AT certification.

## Bounded work / allocation evidence (not a speed guarantee)

In the 100,000-record simple-row fixture, 320-point viewport / 32-point stride:

* **10 realized rows, 35 Layout entities total**, including row/content/label
  entities, viewport/parent/spacers/empty label; assertion requires fewer than 50.
* 20 steady headless paint frames retain the same Layout entity identities and
  produce **400 renderer callbacks**; an asserted upper bound prevents metadata-
  count-dependent callback work. Jumps through keys 1,000, 20,000, 50,000, 99,999
  and back to zero keep the entity bound while retaining offscreen selection.
* Final diagnostic run: metadata mount + first headless frame **59.296333 ms**;
  20 steady headless frames **28.888583 ms total**. These measurements include
  headless layout/style/paint with fallback resources and concurrent test load,
  not desktop GPU presentation, cold launch or a controlled comparative benchmark.
* The unchanged-entity assertion is an entity-allocation proxy, **not global heap
  allocator instrumentation**. The implementation intentionally allocates bounded
  temporary vectors/maps and style strings per reconciliation. It is not claimed
  heap-allocation-free. Metadata replacement still copies/indexes all n records.
* Actual nested-control gallery tests exercise light/dark/high-contrast 200%
  headless geometry and display-list production with fewer than 220 Layout
  entities for 100,000 records. Checked state is application-owned by ItemId and
  is acknowledged, recycled away, then restored on a newly mounted Checkbox.

## Remaining boundaries / independent review

This is a committed implementation candidate, not owner acceptance or integration.
**No desktop pixels were inspected or captured for this increment.** New gallery
checks use headless display lists/geometry; building a native binary is not visual
verification. Actual mouse/key/scroll hardware, native OS IME, screen readers,
Windows/Linux and MSRV runs remain unverified. No startup/GPU speed claim is made.

Uniform-height virtualization only; retained variable-height/tree/table APIs keep
their existing budgets. Render callbacks are ephemeral and must upsert in place;
never recurse into layout or persist row entity handles. Persistent offscreen
editor drafts, open popup lifetimes and exact nested-control refocus after
recycling are unsupported; keep durable state outside rows and commit/cancel
before recycling. Full offscreen accessibility collection navigation/count/
position metadata is not implemented. Very large f32 total heights lose precision;
`u32::MAX` is a count type bound, not a tested dataset-size promise.

Owner retains independent review/native integration. Capture/markdown increments,
platform overlay work and publication remain outside this candidate's scope.
