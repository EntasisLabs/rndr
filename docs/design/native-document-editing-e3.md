# E3 — document-scale source editing and shared views

**Status: E3 implementation accepted by the principal on 2026-11-05; still uncommitted. E2 is committed as `792d3f0`. Quantitative native performance certification remains unmeasured.**

After the verified candidate handoff and disclosure of the remaining measurement limits, the principal confirmed: “OH YEAH we can call this one good”. This records acceptance of the implementation slice, not evidence of a specific desktop test protocol, a ratified latency budget, or measured native event-to-present performance. The technical limits and external native gates below remain documented; no subsequent slice, commit, merge or push is authorized by this acceptance record alone.

The user authorized E3 after accepting the E2 wheel/viewport correction. Work resumed after host disk exhaustion; no source was removed. This record supersedes E1's one-mounted-view restriction, not its exact-source, revision, host-save or native-certification boundaries.

## Implemented contracts

- Multiple attached entities share one `SourceSession` text/revision/history. Each view keeps a directed selection and viewport. `view_selection` and revision-fenced `set_view_selection` address a specific attached view. Inactive endpoints map with `Affinity::After` and snap to valid graphemes. Selection activation does not transfer focus.
- Native edits publish to both panes in the same editor pass regardless of entity order. Switching the invoking pane breaks typing groups. Undo/redo restore the invoking pane's historical selection; other panes map through exact inverse/forward splices, including disjoint host batches and grouped native typing. Splice metadata lives and evicts with the existing bounded snapshot history, not an unbounded second journal.
- Composition remains document-exclusive. Another pane cannot borrow the owner's preedit or accept its orphan commit. Detach/reopen and malformed/dead-view retirement preserve the document/history and remove per-view state. Readonly remains document-wide; mounting/updating retained props changes session policy, not one pane's permission.
- Editor geometry is cached per paragraph under text/local-span, width/style/font and engine identity/epoch fences. Unchanged paragraphs reuse geometry. The assembled document retains caret rows but **no glyph quads**; visible `Label` geometry and glyphs remain window-bounded. Cache eviction never truncates text or rows.
- Paragraph-cache accounting is limited to 8 MiB per view, charging text, spans, rows/edges and per-entry metadata. Rejected entries do not leave empty hash buckets. This is a charged retention budget, not an exact allocator/RSS ceiling. Full-document assembled caret rows remain proportional to document size and are reported separately.
- Immutable decoded buffer snapshots are reused until bytes change. Unchanged managed projection uses document/revision/allocation fences; raw buffer tampering still repairs from the source. Idle publication avoids whole-document text serialization/hashing and uses binary row lookups. Editing/reflow still scans/reassembles document rows; it is not viewport-only layout.
- Per-view source-byte scroll anchors map eagerly through every exact transaction, including multiple revisions between publications and undo/redo. No map history is retained. Native input snapshots externally adjusted offsets before committing; publication records canonical top rows. Width/style/font reflow resolves the same byte anchor in new geometry. Arbitrary legacy buffer replacement uses a conservative prefix/suffix map instead.
- Fallback-font long-line wrapping no longer remeasures the entire suffix for each row on finite nonnegative advance/kerning metrics. An old-algorithm oracle preserves line ranges/whitespace behavior, and a counting-font test bounds calls on a 1.2-million-byte line to linear work. Negative/nonfinite custom metrics retain the legacy correctness path rather than changing its semantics.
- Gap movement uses byte memmove and incremental line-index updates. Snapshot restoration transfers owned strings. Grapheme floor uses full-context `GraphemeCursor`, preserving RI parity, ZWJ, Indic and split-scalar normalization without allocating every document boundary. Existing gap-buffer storage remains: measurements did not justify a rope/piece-table migration.

## Reproduce

```sh
cargo run -p source-session
cargo run -p source-session -- --split
cargo test -p rndr_core -p rndr_ui -p rndr_render -p source-session
cargo test -p rndr_core --release --test source_session_cost -- --ignored --nocapture
cargo test -p rndr_core --release --test editor_scale_cost -- --ignored --nocapture
cargo test -p rndr_render --release --test editor_scale_cost shaped_editor_scale_costs -- --ignored --nocapture
```

`--split` is optional; the original single-pane presentation stays the default. The two panes have distinct accessible names, independent selection/scroll and shared source/undo. The fake host still does not write documents to disk. Offscreen public-fixture captures may combine `--split --capture NEW_PATH.png`.

## Measured provenance and limits

Machine: Apple M4, aarch64 Apple Darwin, macOS 27.0.1, Rust 1.98.0 (`88d9e12ae`), release profile. Baseline storage measurements were collected at E2 `792d3f0`; current measurements use the E3 candidate. Commands print generated fixture sizes and microseconds. No user documents or giant captures are checked in.

The initial model comparison used five samples per fixture/position: those historical values are p50/max, **not p95**. At approximately 10 MiB, ASCII begin/middle/end commit+undo p50 improved from 631.535/624.135/617.947 ms to 5.536/4.572/3.493 ms after the grapheme change. Unicode improved from approximately 372 ms to 3.7–5.5 ms; a long source line from approximately 619 ms to 5.0–6.8 ms. These are model-path observations, not input-to-display timings.

The current model probe uses **21 samples**, reporting nearest-rank p50/p95. On the 2026-11-05 rerun, approximately 10 MiB ASCII begin/middle/end commit+undo measured 1.359/1.414, 1.009/1.067 and 0.660/0.703 ms (p50/p95). Unicode measured 1.379/1.482, 1.061/1.183 and 0.674/0.769 ms; the newline-free line measured 2.737/2.992, 2.606/2.708 and 2.209/2.309 ms. Differences from the initial comparison include subsequent implementation work and run-to-run variation; these are not universal speedup guarantees.

The editor probe uses 21 samples and nearest-rank p50/p95; cold time includes three startup frames, edit includes one commit plus layout frame, resize includes two frames. It measures ASCII paragraphs at approximately 10 KiB, 1 MiB and 10 MiB; deterministic Unicode and mixed-bidi correctness tests are separate. The shaped variant uses bundled Parley plus CPU paint-command generation, **not a native event loop or GPU presentation**. Exact output is regenerated by the commands above; shaped observations from the 2026-11-05 final rerun were:

| Size | Idle p50/p95 | Edit+layout p50/p95 | Resize (two frames) p50/p95 | Assembled row/edge capacity | Visible glyphs |
| --- | --- | --- | --- | --- | --- |
| 10 KiB | 0.087/0.097 ms | 0.146/0.210 ms | 1.546/1.813 ms | 180,576 B | 960 |
| 1 MiB | 0.056/0.057 ms | 3.204/3.337 ms | 3.485/3.548 ms | 18,875,248 B | 960 |
| 10 MiB stress | 0.056/0.056 ms | 32.224/33.230 ms | 24.786/25.230 ms | 184,549,680 B | 960 |

These are a named repeated-paragraph fixture, not a universal throughput promise. Paragraph cache was 2,938 charged bytes because identical paragraphs reuse entries. Diverse paragraphs exercise the 8 MiB retention limit separately. Resident memory also includes source buffers/snapshots, bounded undo, events, spans, engine caches and allocator overhead. The existing 16 MiB history limit remains, now including exact splice metadata; oversized old snapshots may have no retained undo. Events remain count-bounded at 256, **not** byte-bounded.

## Verification and acceptance ledger

### 2026-11-05 recovered final checks

The recovered candidate passed the following after fixing a redundant Delete closure and a needless `Focus` fixture update reported by strict Clippy:

- `cargo test -j 2 -p rndr_core -p rndr_ui -p rndr_render -p source-session`: 869 passed, zero failed, five opt-in tests ignored (including doc tests in the pass total).
- `cargo clippy -j 2 -p rndr_core -p rndr_ui -p rndr_render -p source-session --lib --bins -- -D warnings` and separate strict invocations for core `buffer_storage`, `editor_incremental`, `editor_scale_cost`, `source_multiview`, `source_session`, `source_session_cost`, UI `editing`, and render `editor_scale_cost`: passed.
- Release core model/editor scale probes and shaped render probe: passed explicitly with `--ignored --nocapture`; the model and shaped values above are from this rerun.
- `cargo check -j 2 --workspace` and `cargo check -j 2 -p rndr_core -p rndr_ui --no-default-features`: passed.
- `cargo run -j 2 -p source-session -- --split --capture target/e3-split-recovery-20261105.png`: passed. The image-view tool reported a generated 800×600 public-fixture PNG (98,275 bytes). Its receipt alone is not visual-layout verification; inspect the capture on the desktop. The automated demo layout bounds tests passed. This is not native input/IME/focus certification.

Regenerable local receipts are `target/e3-correctness.log`, `target/e3-clippy.log`, `target/e3-scale-core.log`, `target/e3-scale-shaped.log`, `target/e3-workspace.log`, `target/e3-feature-off.log` and `target/e3-capture.log`. They are not source deliverables or durable checked-in evidence. Builds used two jobs with no overlapping build commands; final free host space was approximately 11 GiB. No E3 commit, merge or push was performed. The native budget decision/measurement below remains open.


- Deterministic cache reuse, cache rejection without data loss, source anchors across disjoint edits/multiple revisions/two panes/undo, width/style invalidation, visible-window bounds, buffer byte/line oracle and Unicode snapping oracle are automated.
- Independent reviews found and drove fixes for coarse multi-splice undo maps, viewport-anchor maps and ambiguous native diffs on repeated source. Native splices are captured from actual operation coordinates, not inferred from matching prefixes; repeated ASCII/Unicode and repeated-newline peer viewport regressions preserve actual insertion/deletion locations. These tests preserve untouched interior positions, not just final text.
- Full test/build/lint/format receipts and final scale reruns are summarized with the candidate handoff. Strict production-target and changed-test Clippy is checked separately: all-target Clippy also reports three pre-existing `needless_borrow` warnings in unchanged `crates/rndr_core/tests/scrollbar.rs`; those unrelated legacy test lines are not silently cleaned up here. Tests do not claim native accessibility or IME certification.
- **Open E3 gate:** the epic requires ratified E0 budgets on a named fixture/machine, or explicit renegotiation. No quantitative native event-to-present budget was ratified here. Headless/CPU measurements do not close that gate. Full assembled row memory and 10 MiB edit/reflow costs are explicit, not hidden behind bounded visible glyph counts.
- **Huge paragraphs:** model probes include a 10 MiB newline-free line. This does not certify synchronous shaping/reflow latency for a 10 MiB paragraph. A paragraph exceeding the geometry-cache budget is shaped correctly but not retained there and may be reshaped after edits; no truncation or silent source limit is introduced. Native long-line responsiveness remains a measured-platform gate.
- Folding/provider cancellation is not invented in E3: those protocols do not yet exist in the source surface and remain E4 work. Existing style/span/font-epoch invalidation is covered.
- Real desktop IME/clipboard/AT/outside-window capture and event-to-present measurements remain external native gates inherited from E2/E8. No merge, push, release, app migration or blanket epic completion is implied.
