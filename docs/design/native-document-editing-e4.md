# E4 — source selection and extension composition

Status: E4 accepted by the principal on 2026-11-05 after native demo retest ("demo is demoing"; "we can close this one out"). The full candidate's automated verification is recorded below. E3 was accepted and committed as `47ec4dc`; this ledger accompanies the E4 closeout commit. Principal demo signoff does not certify the separate native IME/accessibility/platform or performance boundaries below. E3's quantitative native performance certification remains separate; see [E3](native-document-editing-e3.md).

## Full-slice contracts

### E4.1 — multiple and rectangular selections

`source_selection::SourceSelections` normalizes directed, grapheme-snapped source selections, merges overlaps/duplicate starts, keeps adjacent ranges independent, and preserves the primary range's direction. Views own their selection sets; one document owns all history. Native typing, clipboard paste/cut/copy, deletion and undo/redo use the batch history owner. Secondary selection state is restored by undo, while peer positions map through exact operations.

`select_source_rectangle` is an explicit host command over actual window-logical shaped geometry, not guessed monospace columns. The rectangle uses visual rows (including soft wraps), tab advances and bidi caret edges, excludes folded rows, clamps short rows without inserting virtual spaces, and refuses discontiguous mixed-bidi slices atomically. IME captures one primary range and collapses secondary selections rather than broadcasting a composition commit. The demonstration offers explicit selection/rectangle commands; this is not a new global modifier-drag convention.

### E4.2 — owned annotations and literal search

`source_extensions::SourceExtensions` owns registrations and independent revisioned request streams. Results are validated at admission and at consumption. Provider priority, not response arrival time, controls composition. Retiring a provider cancels its pending work and removes only its own marks.

`source_presentation` adds generic colored underlines, diagnostic underlines and labeled view-edge gutter markers through the standard painter. It does not overwrite host `Decorations`, `Spans` or gutter components. Generic decoration classes are opaque, not a language grammar. Gutter descriptions are available to host tooltip/accessibility adapters; this does not certify native accessibility-tree exposure.

`source_search::SourceSearch` is literal, case-sensitive, nonoverlapping UTF-8 search, with revision-fenced navigation/current replacement/replace-all. Empty queries match nothing, so zero-length matching cannot loop. Exact source bytes, CRLF and Unicode are retained; no implicit newline or Unicode normalization or regex syntax. Replace-all is one atomic history step. Optional `rndr_ui::source_tools::FindBar` uses canonical fields/buttons and honors document and inherited target-view policy. Unrelated host action requests remain in the canonical FIFO.

### E4.3 — source hooks and actual folding

`source_folding` offers optional syntax/indent/bracket/fold analysis hooks and a representative language-neutral brace/string/number implementation. It is deliberately not a complete programming-language parser. Fold ranges hide complete lines while keeping original document byte positions, hit testing, source-line numbering and copy semantics. Copy across a fold includes hidden bytes. Moving selection into hidden source expands its fold. An edit invalidates revision-owned folds, and detach/reattach clears them. Accessibility source geometry and rectangle commands use the same fold projection as visible layout.

### E4.4 — delayed completion, diagnostics and navigation

Provider requests capture document/revision/view/selection and cancellation identity. UTF-16 conversion rejects surrogate halves, out-of-line columns and CRLF interior offsets. Invalid or overlapping provider edits refuse before mutation. Completion and diagnostic fixes enter document history as atomic source batches; navigation changes selection without inventing an edit.

`rndr_ui::completion::CompletionPopup` is a passive anchored display: it never takes source focus or IME ownership. The native chronological key boundary handles Up/Down selection, Enter/Tab acceptance and Escape cancellation. IME has precedence. Earlier text in a coalesced queue invalidates older completion before a later acceptance key can run; generic focus traversal/keymaps do not execute completion controls early.

The optional source-session `--extensions` demonstration includes find/replace, multi-selection, shaped rectangles, folding/bracket navigation, owned annotations, TODO diagnostics/fix, and a fake delayed completion provider. No language-service transport, app-specific dependency, filesystem persistence or production language client is included.

## Verification and desktop retest

Recovery verification on 2026-11-05, all builds run serially against the preserved E3-based worktree:

- `cargo test -p rndr_core --features highlight -p rndr_ui -p rndr_paint -p rndr_render -p source-session --no-fail-fast`: **989 passed, zero failed**, across 131 test/doc-test targets. Five opt-in storage/scale characterization probes remain ignored; this is not a new performance certification. Receipt: `target/e4-recovery-tests.log`.
- Strict production Clippy for these crates (`--lib --bins -- -D warnings`) and strict source-session test Clippy passed. Receipts: `target/e4-recovery-clippy.log`, `target/e4-recovery-demo-test-clippy.log`.
- Workspace check, core/UI feature-off check, formatting and whitespace checks passed. Receipts: `target/e4-recovery-workspace.log`, `target/e4-recovery-feature-off.log`.
- The public fake-fixture extension split capture generated successfully at 1000×800: `target/e4-recovery-split-final.png`; receipt `target/e4-recovery-capture-final.log`. Capture generation is not native desktop/IME acceptance. Artifacts remain untracked build output.
- Final regressions cover document-owned readonly publication through retained UI updates, independent host view readonly across projection, retained native completion wake deadlines, and source/command reachability in single/split layouts at the extension demo's default, minimum and larger sizes.

The extension demo now uses a 1000×800 default and an enforced native 800×720 minimum, with more compact chrome. Its extra toolbars made the base demo's old 800×600 default leave only 9 points of source height; the geometry regression caught this. Tests require at least 180×180 points for each source pane and every command inside the viewport at supported sizes. The base demo's 640×540 minimum is unchanged. This extension demonstration does not promise small-window responsive layout below its own minimum.

A passing headless suite does not certify macOS VoiceOver, Japanese/Chinese/Korean/dead-key IME, Windows/Linux native behavior, or E3 input-to-present latency.

Run the optional native demonstration with:

```sh
cargo run -p source-session -- --extensions --split
```

Exercise typing/copy/paste/delete/undo across multiple selections; rectangle geometry under wrap/tabs/bidi; fold/unfold and hidden-source copy; search next/previous, readonly and replace-all undo; delayed completion stale refusal, Up/Down/Enter/Tab/Escape and IME precedence; diagnostics/fix and provider retirement. Both panes retain independent selection/viewport over one document.

## Recovery provenance

The initial E4 worker admission was blocked by runtime-store ENOSPC despite available worktree space. A daemon restart restored admission. A folding peer later hit a transport failure; its partial writes were inspected and recovered, not replayed blindly. Generated `examples/create-view/target/` artifacts remain outside this slice. No push or merge is implied.
