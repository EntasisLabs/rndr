# E5 — separate structured-model vertical slice

Status: **principal desktop demo accepted on 2026-11-05; implementation closed out for commit.** Principal tested the structured demo and explicitly authorized committing E5 and scoping the next slice. E4 closeout is `e2cfd0c`. This turn used exactly **one** scoped model/test helper; the host integrated native input, UI, demo and verification. No reviewer chain or further workers.

Authority: [epic E5](native-document-editing-epic.md#e5--separate-structured-model-vertical-slice) and [architecture](native-document-editing-architecture.md#4-structured-model-and-embedded-boundaries). E4 acceptance does not certify separate native IME/assistive-technology/platform behavior or E3 latency.

## Reviewed contract and representation boundary

The proposed contract was compared against `SourceSession`, source-only `EditorSurface`, native `Editor`, and revision-fenced `access_text` before integration. It is exposed as **experimental**, not a stable unified source/tree API.

- `structured::StructuredDocument` is the sole tree/history authority. Paragraph/heading blocks have stable document-local `BlockId`s; headings accept levels 1–6. Immutable public block/run access prevents bypassing schema validation. Empty content retains one editable paragraph.
- `Position { block, offset, affinity }` uses **block-local UTF-8 scalar-boundary offsets**, not source bytes or inline-run identities. This refines the kickoff’s illustrative text-node position proposal: runs have value identity only, so canonical merging cannot invalidate a position. Document order, not numeric IDs, orders positions. Interactive endpoints additionally snap to grapheme boundaries.
- Directed `TextSelection { anchor, head }` and `StructuredSelection::Node` remain distinct. Node deletion repairs endpoints; active-range replacement collapses to the inserted text’s end. Splits retain the left block ID and make fresh paragraph IDs to the right; joins retain the left block kind/identity. Undo restores tree IDs and typed selections, never ECS handles; new edits do not recycle undone IDs.
- `Transaction { document, base, origin, group, operations }` admits sequential **candidate-tree coordinates**. Replace, split, join, block-kind, mark and node-delete operations normalize and validate a private candidate before publishing. Wrong document, stale revision, invalid position/heading/link, readonly and active composition refuse atomically. Content no-ops mint neither revision nor history.
- Canonical runs are nonempty, adjacent equal mark sets merge, empty blocks have no runs, and structural newlines separate blocks. CRLF/CR/LF plain input normalizes to LF before mapping native input offsets. Insertion inherits marks immediately before its ordered start (the first run at offset zero). Existing suffix marks are preserved across cross-block replacement.
- Strong/emphasis/link are a complete canonical mark set. Links admit a deliberately conservative http(s) host/port schema, simple mailto and nonempty local anchors; credentials, controls/whitespace, HTML delimiters, backslashes and invalid schemes are refused. IPv6 and browser-grade URL parsing are outside this schema. Links are data and underlined presentation, **not automatic navigation**.
- Undo/redo restore snapshots with fresh monotonic revisions. Explicit transaction grouping requires matching group/origin and is broken by selection/mark/composition boundaries. Native keys intentionally use one undo step per edit; automatic timed typing coalescence is **not claimed**. Formatting and typing share this one document history.

### Source versus structured composition

Source sessions remain unchanged. The structured model never stores Markdown, `SourceSelection`, source edit batches or a source history. Shared vocabulary is identity/revision/origin/affinity; shared native machinery is shaping, pointer selection, visual-row motion, key interpretation, IME presentation and clipboard delivery.

`structured_native` keeps a disposable flattened `Buffer`/span projection with exactly one newline per block boundary. The existing native key interpreter runs on a temporary string/history per event, yielding a private splice that is immediately converted to typed tree positions; that scratch state is discarded. It cannot become a second authoritative history. Raw `edit_buffer` refuses structured bindings. Direct raw Buffer tampering is repaired from the tree on projection.

`StructuredDocuments` retains documents independently of surfaces. One attached native view per retained document is supported in E5; a second view refuses rather than silently sharing native selection ownership. Handle plus model identity plus revision fence projection/accessibility and whole-document replacement/undo ABA. Despawning a view releases orphan ownership at the next editor pass; explicit detach/destroy retains tree/history.

## Native surface and generic gallery

`rndr_ui::structured::StructuredSurface` has parent-scoped keyed mount/update, immutable attachment identity, saved document selection, validated teardown and borrowed-child preservation. Its commands toggle strong/emphasis or set/remove links **without stripping other marks**, set paragraph/heading kind, select/delete nodes and undo/redo. Inline formatting requires selected text; collapsed-carets/node selections return `SelectText` instead of inventing stored typing marks.

- Keyboard Enter splits, Backspace/Delete at a boundary joins, and native grapheme/word/row motion, pointer selection, plain-text copy/cut/paste and scrolling use the ordinary editor lifecycle. A node-selected delete/cut deletes the structural node, not just its text.
- `structured_display` publishes actual ordered block identities/kinds/ranges, directed typed selection and the exact document revision. Generic native text accessibility maps revision-fenced display endpoints back to typed positions. It refuses stale layout, edit/undo ABA and replaced-document identity; block navigation uses the declared block boundaries rather than treating display bytes as tree authority.
- IME preedit is transient. The model captures the typed range/revision across mark runs; native commit inserts once with the declared mark inheritance and one undo step. Toolbar focus does not discard saved selection or silently rebase a live preedit. Formatting/selection/detach refuse while composition owns the range. Native cancellation/window blur releases capture; a model-cancelled composition’s late commit cannot retarget a new selection, including readonly-toggle ABA.
- Model readonly refuses **all mutation origins**, including host/IME/undo, while selection/copy/navigation remain available. View/ancestor readonly is additionally enforced for native and toolbar edits. Projection does not overwrite host-owned view readonly metadata.
- Same-frame UI style resolution precedes rich-span recoloring, including the first paint and theme changes. Run foreground comes from `Label::color`, **not** the entity’s `Color` background fill. A rendered capture caught initially invisible unselected body text; the actual foreground repair and light/dark background-distinction assertions now cover it. Retained metadata updates also preserve a host-owned view readonly flag.

Run the synthetic document fixture:

```sh
cargo run -p source-session --bin structured
# Native gallery facade reuses the exact same fixture:
cargo run -p rndr --example structured_gallery --features observe
# Public-fixture offscreen capture only, never a loaded/native desktop document:
cargo run -p source-session --bin structured -- --capture target/e5-structured.png
```

The original `cargo run -p source-session -- --extensions --split` entry remains the default binary. The new fixture uses a 1000×820 default and an 880×720 minimum, two toolbar rows, an editable native surface and a generic tree inspector. Inspector/debug test serialization exposes block kinds/runs/marks explicitly and does **not** claim Markdown round-trip fidelity. There is no file/persistence/network behavior. The demo link button uses `https://example.com`; hosts can dispatch explicit validated destinations through the API.

## Verification / acceptance boundary

Targeted model/native and retained-surface tests passed during integration. The full affected correctness suite, strict production/selected-test lint, workspace/feature-off and gallery checks passed; exact receipts are below. Tests cover tree invariants, atomic rollback, split/join/mark inverses, Unicode splice sequences, normalization, explicit undo grouping, node/empty-document deletion, paste/IME at marked boundaries, readonly, toolbar focus, stale accessible/block mapping, shaped geometry and retained lifecycle. Supported-size demo layout has its own regression.

**Final receipt (2026-11-05):**

- **1,030 tests passed, zero failed**, across 133 test targets in `target/e5-final-tests.log`: `cargo test -p rndr_core --features highlight -p rndr_ui -p rndr_paint -p rndr_render -p source-session --no-fail-fast`. This adds 41 E5 tests to the E4 baseline: 14 model, 15 native, 10 retained UI, 2 shaped demo. Five opt-in performance/storage probes remain ignored; no new performance certification is claimed.
- Strict affected production Clippy passed (`--lib --bins -- -D warnings`), as did strict retained-UI-test and source-demo-test Clippy. Receipts: `target/e5-final-clippy.log`, `target/e5-ui-test-clippy.log`, `target/e5-demo-test-clippy.log`.
- `cargo check --workspace`, core/UI `--no-default-features`, and the gallery facade with `--features observe` passed: `target/e5-workspace.log`, `target/e5-feature-off.log`, `target/e5-gallery-check.log`.
- Following the final retained-metadata readonly correction, all **10** structured UI tests and both shaped-demo tests were rerun and passed, plus strict UI/test lint: `target/e5-retained-final.log`, `target/e5-shaped-final.log`, `target/e5-retained-clippy-final.log`. These are reruns, not extra tests added to the 1,030 count.
- Corrected public-fixture capture was opened and visually checked: `target/e5-structured-reviewed.png`, 1000×820, SHA-256 `25212bc29115984082e11ed710eedf1d98aedf2b870d75481cddbe7c2757be89`. Body text/selection, heading, all eleven toolbar controls and tree inspector are visible. Automated 1000×820 and 880×720 checks retain at least 200 points of editor height and keep controls in bounds. This is offscreen fixture evidence, not native OS acceptance.
- Verification used `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 RUSTFLAGS='-C debuginfo=0'`. Generated captures/logs and preexisting `examples/create-view/target/` artifacts remain outside source changes. E4 remains safely committed at `e2cfd0c`; E5 source and this acceptance ledger are included in the closeout commit. Nothing pushed or merged.

The implementation/demo can be accepted independently of full native OS IME/AT certification. Principal desktop smoke was accepted on 2026-11-05; this closes the E5 implementation/demo gate only. No Windows/Linux/macOS platform matrix or performance certification is inferred from that smoke, headless shaping or an offscreen image. Lists/tables/embeds remain E6; Markdown codecs/fidelity remain E7.
