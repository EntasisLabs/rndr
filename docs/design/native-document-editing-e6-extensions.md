# E6.2 / E6.3 — document tables, atomic nodes and embedded boundaries

Status: **E6.2 and E6.3 accepted and closed**, 2026-11-05. Principal desktop signoff after the shared appearance pass: "oh yeah so much better! Lets call this good now", with the 2:50 PM desktop screenshot. Starts from accepted E6.1 commit `e52f982`; this record accompanies the extension closeout commit. Nothing pushed or merged. Two scoped helpers contributed canonical model/tests and shared geometry/tests; the host integrated native input, retained UI, lifecycle, demo, review, fixes and verification. No replacement/helper chains.

Authority: [E6 epic](native-document-editing-epic.md#e6--structured-extensions-and-embedded-editing-boundaries). E6.1 acceptance remains intact; principal desktop acceptance now closes the E6.2 and E6.3 checkboxes.

## E6.2: real document tables

- Canonical `ContainerKind::{Table, Row, Cell { header }}` membership, stable container/leaf IDs and validated rectangular shape. This does not use the UI `DataTable` widget or serialize table structure into source text.
- Table insertion at a collapsed caret splits the owning paragraph while preserving its marks/attributes. Insert/delete row and column operations are atomic transactions; selection is repaired into surviving content. Undo/redo restores tree and typed selection, without reusing fresh IDs.
- Equal-width columns with independent cell wrapping/padding and row height determined by the tallest cell. Header cells have distinct chrome. The same shaped glyph/line coordinates drive rendering, caret/selection rectangles, pointer placement and keyboard navigation. Cell whitespace belongs to its allocated column, not whichever text glyph happens to be nearest.
- `StructuredSelection::Cells { anchor, head }` is distinct from a scalar text range. Click a cell border, then Shift-click another border for a rectangle; native cross-cell drags also publish a cell rectangle rather than flattening cell boundaries into text edits. Shift-arrows extend a cell rectangle; Escape/arrows can return to text.
- Tab/Shift-Tab traverse cells before global focus navigation. At the first/last cell they move into surrounding document text. Arrow navigation follows shared visual geometry. Structural selection and navigation do not create content history entries; readonly still allows navigation/copy.
- Rectangular copy emits a validated rich `TableFragment` and TSV/plain fallback. The app-local rich clipboard is reused only when the native clipboard's plain value still matches; otherwise plain tab/newline input is parsed and validated. Marks, inert attributes and supported subtree content survive rich paste with remapped identities. Cut/clear/paste use one parent transaction and undo; malformed/nonrectangular/overflow/cross-table shapes refuse without partial application.
- Native grid publication retains the full disposable text projection and clips canonical geometry, avoiding the old monotonic-y window-slicing assumption. No new million-row/virtualization performance certificate is claimed for table-bearing documents.

Current bounded table shape: no merged/spanning cells, column-resize UI or nested tables. Existing list/quote content may live within cells under schema validation; a scalar text replacement cannot cross cell/atom boundaries.

## E6.3: opt-in atoms and retained embedded editing

- `BlockKind::Atomic` carries a validated permission-free `AtomicPayload` (`kind`, fallback and inert attributes), stored without changing the public `Block` literal shape. Whole-node selection, keyboard delete, fallback copy and parent undo preserve stable model identity. Arbitrary metadata does not execute host actions.
- Atomic/image cards reserve shared layout geometry and render a bounded fallback caption. Unresolved image resources remain alt/fallback text; the library performs no fetch, disk access, navigation or implicit decoding. A host may supply its own permissioned rendering/child mount.
- Core `AtomicMount` binds exact parent view/document/block and host-owned child root/input target. UI `AtomicPanelHandle` is a retained, host-placed panel with an explicitly staged child editor. Repeated mount/update preserves identity and drafts; clean host changes can refresh a draft, dirty conflicts refuse instead of overwriting either side.
- **Same-document child:** typing, IME, wheel and drag stay in the staged child. Explicit `Commit child` publishes one complete atomic payload transaction into parent history; parent Undo restores the prior payload. The child does not silently write parent text on every keystroke.
- **Foreign-document child:** ownership is an explicit host mount choice, not inferred from payload strings. Only a typed `AtomicHostIntent` is emitted. Both the panel commit API and an async response token refuse a parent-history commit from a foreign mount; the application must separately admit any external execution.
- Async responses are fenced by parent document/revision, policy/composition epoch, exact attachment incarnation, child/root ownership, focus epoch and atomic retained lifetime. Deletion followed by Undo restores model identity but allocates a new retained lifetime, so an old mount/request cannot resurrect its child or apply a stale response. Readonly/disabled/hidden/preedit admission refuses.
- Child Escape is captured before generic focus handling and tears down after ordered child input; the same frame cannot replay queued text into the parent. Detach/parent deletion revokes mounts and requests, clears owned captures/preedit, closes and destroys detached popups anchored in the child scope, destroys the adapter's complete child subtree and repairs focus only when the scope still owns it. No keyboard trap or implicit foreign operation is introduced.

This is a retained native editing boundary, not an HTML/webview embed, automatic inline widget injection or permission grant. The demo deliberately opens a host-placed child panel separately from the inert inline card.

## Shared document appearance defaults

The principal's mechanical tryout passed, but the initial debug-like grid/cards were **not visually accepted**. The follow-up appearance pass lives in core/UI/paint, not demo-only colors or a toolbar redesign:

- Quiet neutral table boundaries, tinted semantic header surface, medium-weight header text and ten-point cell padding. Shared shape coordinates still own wrapping, pointer placement, caret, IME and selection.
- Rounded neutral atomic cards with fourteen-point internal padding, block gaps and a small secondary type/status label (`Image unavailable`, `Embedded content`, or `Attached content`) above readable fallback text. The visual caption is inert and bounded to two fallback lines; the full fallback/payload remains in the model, copy and accessibility contract. No image fetch or automatic resource decoding was added.
- Surfaces/boundaries paint **before** text and selection. Selected cells/cards use the accent outline and restrained tint; structural card/rectangle selections no longer paint a false scalar insertion caret over the object. Ordinary text editing retains normal caret/selection behavior.
- `StructuredChromeStyle` is an optional world resource for host defaults and/or a per-editor component. Per-editor supplied fields override resource fields; absent fields derive from the editor's current foreground/background and core theme. Colors and card radius are overridable without changing document bytes/history. Layout cache fencing includes the secondary caption color.
- `UiTheme::structured_chrome_style()` supplies semantic defaults through `apply_theme_bridge`: neutral card/header surfaces, subtle resting stroke, secondary label and focus accent; high-contrast themes use `StrongStroke`. Reapply the existing explicit bridge when changing themes. Local editor overrides survive bridge updates.
- Retained child panels now use theme recipes for their group surface and Reading-role editor instead of hardcoded light-only swatches. Theme changes preserve retained editor identity and draft content.

Principal accepted the shared defaults in the final desktop screenshot on 2026-11-05. Acceptance is based on that explicit desktop signoff, not the automated render alone.

## Verification

Initial extension implementation receipts, all successful (appearance follow-up below supersedes these totals):

```sh
CARGO_INCREMENTAL=0 cargo test -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session
CARGO_INCREMENTAL=0 cargo clippy -p rndr_core -p rndr_ui -p rndr_paint -p source-session --lib --bins -- -D warnings
CARGO_INCREMENTAL=0 cargo check --workspace
CARGO_INCREMENTAL=0 cargo check -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
cargo fmt --all -- --check
git diff --check
```

**1,086 tests passed, zero failed; five opt-in probes ignored.** Includes existing E6.1 Tab/checkbox viewport regressions, canonical table/atom tests, three shared grid-geometry regressions, eleven full-frame extension tests, renderer contracts, demo layout checks (including 880×720) and doc-tests. New full-frame coverage includes cell-border/column whitespace placement, Tab escape, visual arrows, rich copy/paste/clear/undo/readonly, retained panel identity/automatic clean-draft refresh after parent Undo, rectangle chrome without false scalar text highlighting, child IME/read-only teardown, stale blur/policy/reattach/delete+undo callbacks, foreign history refusal, detached popup deletion/focus repair and nested child wheel/drag isolation.

Actual native render generated with:

```sh
CARGO_INCREMENTAL=0 cargo run -p source-session --bin structured -- --capture target/e6-extensions-reviewed-final.png
```

The host opened and visually inspected the exact final 1000×1080 PNG (original file SHA-256 `22bc11a50cbe3b23d988e172154a1901edf5d88de6db837f23dc1c582fabe8b0`; viewed PNG rendition SHA-256 `6708f4c5841622e5bf8649d1bd6c5239f2d77006dd79c1aafe61aae01de4a155`): real equal-width header/body grid, cell wrapping, aligned existing task/quote text, inert labeled atomic cards and retained toolbar controls. Generated captures/logs stay outside source changes. Native OS IME and assistive-technology acceptance still require desktop/platform checks; headless tests are not that certification.

Disk exhaustion was recovered by deleting only regenerable worktree incremental caches and obsolete pre-turn executable caches. Source, libraries, current executable caches and existing saved captures were preserved. Subsequent builds disabled incremental caching.

### Appearance follow-up verification

Re-ran the full command set above after the shared-default changes: **1,090 tests passed, zero failed; five opt-in probes ignored**, across 138 test/doc-test suites. Strict production Clippy, workspace check, feature-off check, formatting and whitespace gates all passed. The five shared geometry tests now include padded inert fallback glyph bounds, thin header boundaries/semantic font weight, theme-derived palettes and local overrides without document/history changes. New paint/UI regressions verify rounded layers precede glyphs, selected atomic cards have no full-height scalar caret, and live light/dark/high-contrast theme changes preserve retained child identity and draft bytes. Existing eleven extension interaction tests and E6.1 regressions remain green.

Final appearance render was generated from the verified candidate:

```sh
CARGO_INCREMENTAL=0 cargo run -p source-session --bin structured -- --capture target/e6-style-reviewed-final.png
```

The host opened the actual final 1000×1080 native render for visual review: original PNG SHA-256 `ca30e8c57a31c3f6fc2885b256f84f261a476e31bca504f25fd380022bb8a0fe`; viewed rendition SHA-256 `b96476bf459bbdc751d41dd356f795793a615cde141e8136f0858d3f08c1a84c`. Generated captures/logs remain outside source changes. No new helpers were used for the appearance follow-up. Principal subsequently accepted the desktop result; closeout includes these shared defaults. Nothing pushed or merged.

## Principal desktop tryout

```sh
CARGO_INCREMENTAL=0 cargo run -p source-session --bin structured
```

1. Click table text; Tab/Shift-Tab through cells and into surrounding text. Try row/column insertion/deletion and Undo.
2. Click a cell border, Shift-click a second border; copy/paste the rectangle and undo a clear. Readonly must block mutations while permitting copy/navigation.
3. Select the embedded card → **Open child** → type/scroll/select inside it → **Commit child** → parent **Undo**. Escape/Close child must return focus without leaking text or leaving overlays.
4. **Foreign child** → **Host intent** produces only a displayed intent. **Commit child** must refuse; parent content/history must remain untouched.

Accepted: principal desktop signoff for E6.2 and E6.3 received on 2026-11-05 after the mechanical tryout and shared-default visual pass. Generated captures/logs are excluded from the closeout commit. Deferred resize handles, host-supplied image resources, native OS IME/AT certification and E3 performance/storage certification remain separate; this acceptance does not claim those gates.
