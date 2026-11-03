# ListView + TreeView — implementation and evidence handback

**Status: implemented and locally verified; ready for sender's independent review. Not approved, integrated, published or deployed.**

- Undertaking: `work-715a501cc17006dd49e1738f40a94df0`
- Branch: `worktree/rndr-composable-listview-and-treeview-foundation`
- Approved baseline: `d0cbecf3524acd39fc9fc1c3cb35e33f04b62eeb`
- Implementation commit: **`865cbb3463eb2ab1c4460888ebfdfd3b37052857`** (`SOURCE_COMMIT`). The following evidence commit does not alter that implementation.
- Scope: reusable `rndr_ui` components and an independent synthetic gallery. **No Blooey edits, other-worktree source edits, remote push, approval or integration.**

## Delivered

Public `rndr_ui::collections::{ListView, TreeView, ViewHandle, Item, ItemId, TreeNode, RowSlot, ViewProps, SelectionMode, CollectionEvent}`:

- Application-owned metadata/data, scoped stable IDs, persistent passive composition slots, accessible names, explicit state events.
- Independent focus, zero/one selection and activation. Click/Space-release selects; arrows move focus; Enter activates. Modes are **None/Single only**; no initial automatic selection.
- Stable entities/content across reorder/rename; deterministic focus/selection reconciliation when data is filtered, removed, disabled or reparented.
- Tree preorder validation, nested expansion, Up/Down/Home/End/Left/Right navigation, safe ancestor-collapse focus and preserved collapsed selection. Surviving branches that lose their last child retire expansion; passive leaf indentation remains selectable row area.
- Existing core Scroll, hit/clip, focus/reveal, release activation, request revalidation, theme/style and accessibility machinery reused. Legacy pooled `List`, filesystem-aware `Disclosure`, Shell and global Selection remain intact.
- Small additive core seams: scoped action-intent drain, roving TabStop, per-event KeyNavigation inside the chronological dispatcher, canonical `reveal_entity`, logical collection accessibility metadata/roles. No parallel input queue, selection resource or scroll engine.
- Interactive `data_gallery`: live filter, reset, add/remove, reorder, empty/restore, theme/density/disabled toggles, native resizing, nested tree and host paging. Real core text editing and public components, not mock screenshots.

API/ownership/reconciliation docs, precise limitations, dependency prep and launch instructions: **[docs/data-components.md](../../docs/data-components.md)**. `DataTable`, `PropertyInspector` and Charts remain documented extension/roadmap work, not stubbed implementations.

## Reproduction

Commands were run from the isolated worktree, not another checkout. The final verification script logs each exact command and exit code:

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
bash scripts/verify-data-components.sh
```

`CARGO_BUILD_JOBS=2`, `CARGO_INCREMENTAL=0`, dev/test debug info disabled, assertions retained. Rust/Cargo **1.98.0**; dependency resolution/builds were **offline + locked**. The existing compatible 4.6 GiB build cache was reused, not copied or cleaned. A fresh machine must first run `cargo fetch --locked`; no Blooey `.deps` snapshot is involved.

Launch the standalone interactive app:

```sh
# From this checkout after the environment/dependency preparation above:
cargo run --offline --locked -p rndr --example data_gallery --features observe
# Ordinary use does not need --observe. Optional startup fixtures:
cargo run --offline --locked -p rndr --example data_gallery --features observe -- --dark --narrow
```

The verification runner executes formatting checks, complete core/paint/UI suites, collection measurements, native observation/AT tests, fallback-renderer tests, both facade startup configurations, Python tests, native gallery build and no-default-features gallery compile. It stops rather than cleaning caches if free disk falls below 2 GiB. For an independent rerun use a fresh log directory: `bash scripts/verify-data-components.sh evidence/data-components/review-run`.

## Verification results — final committed source

| Final command/log | Result |
|---|---:|
| `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui` / `core-paint-ui.log` | **558 passed; 0 failed/ignored** |
| `cargo test --offline --locked -p rndr_ui --test collections -- --nocapture` / `collection-metrics.log` | **17 passed**, already included in the 558 above |
| `cargo test --offline --locked -p rndr_render --lib --features observe` / `native-observe.log` | **73 passed** |
| `cargo test --offline --locked -p rndr_render --lib --no-default-features` | **25 passed** |
| `cargo test --offline --locked -p rndr --lib --features observe` | **1 passed** |
| `cargo test --offline --locked -p rndr --lib --no-default-features` | **1 passed** |
| `python3 -m unittest discover -s tools -p 'test_*.py'` | **20 passed** |
| Native gallery build with observe, fallback gallery check, scoped rustfmt check | **All passed** |

The 17 collection integration tests mount the actual public components with UiPlugin/core scrolling, deliver pointer/key/access commands through the real pipeline and produce real paint frames. Coverage includes stable IDs/content under mutation, independent focus/selection/activation, chronological Tab→Down→Enter batches, AT-disclosure→Tab traversal, pointer release/cancellation/scroll dragging, nested expansion/collapse, removal, surviving branch→leaf, selected hidden descendants, disabled ancestry, empty view focus, None mode, wheel/reveal, stale handles, atomic validation, namespace conflicts and unrelated host request preservation. Rendered assertions check clipping, actual focus/selection/disabled quad colors and comfortable/compact sizing across light/dark and standard/high contrast. Native adapter tests check Tree/TreeItem roles, logical hierarchy, level, selected=false/true, focused target and disabled action advertising.

### Large-data strategy and measured cost

**Not virtualized.** A view retains at most **1,024 total nodes**, depth <=32. Layout/style/accessibility scale with supplied nodes, not viewport height. The final diagnostic run measured **6,147 layout entities** and **276.43 ms** for the first 1,024-row headless test frame. This unoptimized, concurrent test-suite measurement is neither a desktop FPS benchmark nor a performance guarantee; it demonstrates why full-cap retained rendering is not the preferred large-data UI strategy.

The gallery owns **10,000 logical records** and composes **128-record pages**, plus four tree grouping nodes. Paging tests measured **1,639 total mounted layout entities** in the application and verified the count stays stable across full pages while the complete 10,000-record model remains app-owned. Filtering scans app data and page changes reconcile off-page IDs away. This bounds this gallery's mounted work; it does **not** prove collection virtualization or viewport-bounded work.

## Native visual evidence and inspection

Final binary SHA is recorded in `native-binary.sha256`. Every final state was captured **after the final source changes** with the existing native observation validator, then opened directly with `cognition_image_view` for pixel inspection. Hardware/render provenance: **Apple M4 / Metal / Parley / quads / 4x scene samples**, opaque clear. PNGs are actual rendered native surfaces, not desktop photos or static design mocks.

| Final folder / flags after `--` | Pixels inspected / finding |
|---|---|
| [final-light/frame-2.png](final-light/frame-2.png) / none | Quiet surfaces and secondary metadata; selection and separate focus frame visible. |
| [final-dark/frame-2.png](final-dark/frame-2.png) / `--dark` | Correct dark ink/surfaces, no white legacy disclosure fallback; state contrast retained. |
| [final-narrow/frame-2.png](final-narrow/frame-2.png) / `--dark --narrow` | **380×920**, stacked usable panels, selected first row fully visible, normal scroll clipping at the bottom. |
| [final-tree-focus/frame-2.png](final-tree-focus/frame-2.png) / `--tree-focus` | Whole-row tree focus frame is distinct from selected descendant and list selection; no filled focus gutter. |
| [final-disabled/frame-2.png](final-disabled/frame-2.png) / `--disabled` | Disabled row/disclosure ink, no active focus; selected context remains visible but inert. |
| [final-empty/frame-2.png](final-empty/frame-2.png) / `--empty` | Inset empty-state text, named focused viewport, no orphan row or stale scroll offset. |
| [final-filtered/frame-2.png](final-filtered/frame-2.png) / `--filtered` | Actual query in framed text field and distinct no-results feedback in both views. |
| [final-compact-hc-200/frame-2.png](final-compact-hc-200/frame-2.png) / `--dark --compact --hc --200` | Enlarged UI text, high-contrast boundaries including the field, wrapping and feedback area remain usable. This flag scales UI text; not a physical DPI/OS accessibility certification. |
| [final-large/frame-2.png](final-large/frame-2.png) / `--large` | Previous/Next page controls, page count, dataset status and honest paging caption fit in the initial viewport. |

All non-narrow captures are **1040×820**. Each folder includes two idle-identical PNGs, both JSON frame headers, process log and `result.json`. All nine cases passed revision freshness, invalid-auth rejection, normal zero-exit shutdown and observation-session cleanup. The capture tool's capability values are not persisted. Two frame hashes/JSON receipts alone were not treated as visual acceptance; the final PNGs were actually opened.

Exact capture commands (fresh output paths required for a non-destructive reviewer run):

```sh
source scripts/data-gallery-env.sh
BIN="$CARGO_TARGET_DIR/debug/examples/data_gallery"
for state in light dark narrow tree-focus disabled empty filtered compact-hc-200 large; do
  set --
  case "$state" in
    dark) set -- --dark;;
    narrow) set -- --dark --narrow;;
    tree-focus) set -- --tree-focus;;
    disabled) set -- --disabled;;
    empty) set -- --empty;;
    filtered) set -- --filtered;;
    compact-hc-200) set -- --dark --compact --hc --200;;
    large) set -- --large;;
  esac
  python3 tools/validate_observation_native.py "$BIN" \
    "evidence/data-components/review-$state" -- "$@"
done
```

The executed final matrix used `final-$state` and `capture-final-$state.log`; the `review-` prefix above intentionally avoids overwriting supplied evidence.

### Iteration trail / corrections

Earlier `light`, `light-fixed`, `iteration-1-*` and `iteration-2-*` folders are development snapshots, **not final-source certification**. They preserve the render→inspect→fix trail: style-owned glyph ink was initially clobbered by replacing Label components; row/empty focus surfaces needed consistent insets; the high-contrast raw text field lacked a visible outline; narrow initial focus clipped the selected first title; paging/enlarged type needed viewport room for feedback. These were corrected and the full matrix recaptured from the final binary.

`iteration-at-tab-failure.log` preserves the actual failing AT-disclosure Tab regression before the dispatcher refreshed its focus origin. `iteration-launch-fixture-failure.log` is an earlier startup-fixture compile failure, not a current limitation. Source audit also found branch→leaf stale expansion and a leaf's disabled icon swallowing its indent hit area; the final public regression verifies both corrected behaviors and that unrelated host requests survive reconciliation. Development compile failures do not count as passing verification; only the final script/summary above does.

## Integrity and review

- `source.sha256`: the **15 implementation/doc/script files** changed from the baseline to the source commit. Includes the exact shared gallery fixture used by tests and native launcher.
- `MANIFEST.sha256`: evidence bundle hashes, including this report and audit helper; excludes itself and the generated `audit.log` receipt to avoid circular hashes.
- `verification-summary.json`: machine-readable final counts/measurements.
- Raw stdout logs intentionally preserve original trailing whitespace/EOF. Source `git diff --check` and rustfmt passed; the evidence-only commit's whitespace check excludes blank-at-eol/blank-at-eof instead of rewriting captured receipts.
- `SOURCE_COMMIT` and `native-binary.sha256`: implementation and captured-binary pointers.

From repository root:

```sh
shasum -a 256 -c evidence/data-components/source.sha256
shasum -a 256 -c evidence/data-components/MANIFEST.sha256
python3 evidence/data-components/audit.py
```

The helper verifies both manifests, native result/provenance consistency, PNG dimensions, idle PNG identity, freshness/auth rejection and cleanup. It does not grant approval or substitute for independent source/visual review.

## Residual limits / platform gaps

- No full virtualization, multi/range selection, nested interactive row editors, typeahead/PageUp/PageDown, lazy children, drag-and-drop, cross-page selection persistence or double-click activation. Passive row subtrees are collection-owned and destroyed when their IDs disappear. API guide spells out initial modes and lifecycle rules.
- AT Activate on a row is canonical click/selection, not the separate Enter/host-activation event. Disclosure commands are separately exposed. Real screen-reader interaction remains unverified.
- `AccessRole` adds Tree/TreeItem. Downstream exhaustive matches must handle them when adopting this framework revision; no downstream app adoption is performed here.
- Headless input tests are not proof of physical mouse/keyboard, touch, real IME or live assistive technology. Native captures prove rendering on this Mac only. Windows/Linux, real DPI changes and physical OS interaction remain manual/platform gates. Fallback tests/build passed; fallback native appearance is not visually certified here.
- No claim that the entire framework now matches Apple/Fluent finish, and no publication/release certification.

## Disk / preservation

Free space began at about **7.7 GiB** and the final post-capture/source-commit check reported **6.7 GiB** (APFS availability fluctuates). All substantial builds reused the compatible cache with debug/incremental output bounded. One early check used a previously absent default target and left **4.1 MiB** of regenerable check output; it was not a second heavyweight build. No broad cleanup was performed. Only this turn's generated `tools/__pycache__/observe_client.cpython-314.pyc` was removed before committing; source/history/other caches/evidence/worktrees were preserved. The evidence directory is about **7.3 MiB** before this report/manifests, not a duplicated build tree.

**Sender action:** arrange the independent reviewer against this committed candidate, review the code/evidence and use native owner integration if accepted. The implementation worker has not approved, merged, pushed or deployed anything.
