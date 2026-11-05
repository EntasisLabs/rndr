# E6 — structured extensions and embedded boundaries

Status: **E6.1 accepted and closed**, 2026-11-05. Principal desktop signoff: "ok I think noooow we can close this one out" after rendering and interaction fixes. E5 was accepted and committed at `c6355e6`; this closeout records the E6.1 commit candidate. Nothing pushed or merged. E6.2 tables and E6.3 embedded boundaries remain unimplemented.

Authority: [epic E6](native-document-editing-epic.md#e6--structured-extensions-and-embedded-editing-boundaries). Each epic line is a separate bounded implementation/review/demo gate. The principal authorized at most two scoped helpers, not reviewer or replacement chains. Exactly two helpers contributed model/headless tests and structural chrome; the host owns native integration, toolbar/demo, review, fixes and verification. Recovery after the provider error spawned no new helpers.

## E6.1 structural contract and review

- **Real tree ownership:** stable `ContainerId` quote/list/item containers plus stable `BlockId` editable leaves. Parent identities encode nesting; contiguous descendant leaf order determines sibling order. Lists contain only item containers. Task state belongs to the item, never a checkbox widget or source prefix. Missing parents, cycles, invalid membership, discontiguous subtrees and invalid numbering refuse atomically. Empty containers prune deterministically; IDs are never reused after undo.
- **Coordinates:** existing typed block-local UTF-8 positions remain separate from source offsets. `StructuredDisplay` exposes containers, block parent/attributes, revision and text ranges. Native text is a disposable newline-separated projection, not Markdown or serialized tree authority. Structural markers never enter text, clipboard bytes or editable spans.
- **List/quote operations:** wrap sibling leaves sharing a parent, indent entire items under their preceding sibling, outdent/promote whole subtrees, and split outer lists/quotes without reordering text. Cross-parent wrapping refuses rather than guessing. Ordered starts are explicit positive integers; split/outdent tails retain ordinal continuation. Adjacent independent lists are not implicitly merged.
- **Keyboard:** nonempty Enter splits an item and continues its list; new tasks start unchecked. Descendant lists follow the right split item. Empty Enter exits one structural level. Backspace at a structural start outdents first, otherwise joins/deletes through existing grapheme-aware transactions. Tab/Shift-Tab inside list content indent/outdent; outside lists ordinary native focus policy remains. Plain newline paste creates sibling items in an item, not painted bullet text.
- **Tasks:** toolbar and pointer commands toggle the nearest owning task item, including quoted content. Selection remains directed and unchanged; one toggle is one document undo step. Pointer admission uses explicit retained gutter ownership, same-parent validation, topmost hit testing, clipped fresh shaped geometry, revision fencing and focus policy. Readonly and native preedit prevent mutation.
- **Horizontal rules:** nontext whole-node boundaries; no text positions inside them. Click selects the rule; adjacent Backspace/Delete selects it before deletion, and node deletion is undoable. Arrows/Tab/Enter leave a selected rule for an editable leaf. Plain typing/paste cannot silently convert it into text. Deleting the last editable leaf creates a real empty paragraph. Standalone rule copy/cut fallback is `---`; ordinary multi-block text fallback contains only projected text/newlines, not synthetic chrome.
- **Attributes/styles:** at most 32 inert attributes per block, ASCII identifier keys of 1–64 bytes and control-free values up to 1024 bytes, sorted deterministically. Text styles support actually projected RGB color and literal font family; family is 1–80 bytes and rejects controls/CSS delimiters. Per-run font size is deliberately not exposed because current `TextSpan` cannot render it. Mark operations preserve strong/emphasis/link and directed selection. Invalid style/metadata transactions publish nothing; equivalent updates do not advance revision.
- **Host boundary:** attributes are the optional block-ID/annotation/footnote metadata seam, not executable footnotes or a registry of arbitrary executable schema types. No Liquid, transclusion, network, image fetching or app interpretation is added. The existing M3 audit in `native-document-editing-evidence.md` is the parity reference; no claim of a fresh external consumer checkout audit. Editable code blocks remain excluded, matching that reference.
- **Native/retained ownership:** document transactions own history; disposable buffers and UI do not acquire independent undo. Toolbar focus preserves selection, IME capture excludes structural mutations, and document/inherited readonly applies to all editing origins. Gutter/editor are retained owned children and tear down with the surface. Detach, stale projections, changed shaping, composition and invalid ownership suppress chrome. Marker paint uses actual wrapped/scrolled row geometry and parent clips.

The API remains experimental, with one native view per structured document. Existing accessibility text revision fences and typed block navigation are preserved; native OS list/task roles and IME/assistive-technology acceptance are not certified by headless geometry.

## Demo

```sh
cargo run -p source-session --bin structured
```

The fixture contains an actual nested task list and quote, plus formatting controls and a generic-tree inspector. Try checkbox click → Undo; Tab/Shift-Tab in a task; Enter to continue, then empty Enter to exit; Quote/Bullet/Ordered/Tasks on sibling text blocks; Rule at a collapsed caret, arrow away, adjacent Delete/Backspace, Delete block and Undo. Blue mono/Clear style and Metadata exercise inert marks/attributes. Readonly still permits selection/copy but no checkbox or toolbar mutation. Test both ordinary text and mixed Unicode.

## Verification

Fresh recovery receipts (2026-11-05):

| Check | Result / worktree-local evidence |
|---|---|
| `cargo test -p rndr_core -p rndr_ui -p rndr_paint` | **919 passed**, zero failed, three opt-in probes ignored; `target/e6-recovery-full-tests.log` |
| `cargo test -p source-session` | **15 passed**, zero failed (includes two structured fixture tests); `target/e6-recovery-demo-tests.log` |
| `cargo test -p rndr_render` | **74 passed**, zero failed, two opt-in probes ignored; `target/e6-recovery-render-tests.log` |
| Combined affected correctness/doc tests | **1,008 passed, zero failed; five opt-in scale/storage probes excluded.** This is the listed affected-suite scope, not a claim that every workspace test ran. |
| Strict production lint | `cargo clippy -p rndr_core -p rndr_ui -p rndr_paint -p source-session --lib --bins -- -D warnings` passed; `target/e6-recovery-clippy.log` |
| Workspace compatibility | `cargo check --workspace` passed, including create-view/gallery; `target/e6-recovery-workspace.log` |
| Feature-off compatibility | `cargo check -p rndr_core -p rndr_ui -p rndr_paint --no-default-features` passed; `target/e6-recovery-feature-off.log` |
| Source hygiene | `cargo fmt --all -- --check` and `git diff --check` passed. |
| Actual public fixture render | `cargo run -p source-session --bin structured -- --capture target/e6-recovery-structured.png` passed. Host opened the actual pixels: text and selection visible, three toolbar rows reachable, nested task markers and quote bar present, editor/inspector within the 1000×820 frame. Generated PNG stays out of source/commits. |

New regressions cover list model invariants/inverses, ordered continuation and middle outdent, Unicode cross-container replacement, split/nesting ownership, readonly/IME/schema refusals, HR selection/navigation/copy/delete/undo, retained gutter teardown, typed chrome paint and stale layout suppression. Full-frame pointer regressions additionally cover selection-preserving checkbox undo, quoted task ownership, readonly, stale projection and native preedit refusal.

The recovery fixed a `Children.list` test-fixture compile error, finished the interrupted structural pointer routing, covered quoted-task ownership, and removed an unrendered font-size field rather than promising invisible formatting. The final demo-only Clippy redundant-default fix was subsequently rebuilt by the successful capture. Only worktree-local `target/debug/incremental` generated cache was removed to recover disk space; no source or saved captures were deleted. Existing untracked `examples/create-view/target/` captures are preserved and excluded from this candidate.

## Desktop rendering follow-up — 2026-11-05

Principal confirmed functional behavior but rejected the fixed text positioning in the desktop screenshot. The old 100-point marker column left all paragraph text flush at one x-coordinate, regardless of list/quote ancestry.

Replaced that column with a 4-point retained margin and 24-point per-item/quote paragraph indentation. Each paragraph shapes at its own remaining width; glyphs, caret edges, selection/highlight boxes and wrapped continuation rows share the same origin. Structural depth participates in both native full-document row-cache and visible-window layout keys, so indent/outdent/undo reflow even when text bytes are unchanged. Native pointer/navigation/scroll geometry uses the same shaper, rather than merely moving painted glyphs. Checkbox/ordinal/quote geometry now sits directly before its paragraph; nested rules follow paragraph indentation too. Clipboard and model text remain unchanged.

Fresh follow-up verification: **921 passed, zero failed, three opt-in probes ignored** across `rndr_core`, `rndr_ui`, `rndr_paint` (`target/e6-indent-full-tests.log`), plus **15 demo tests passed** (`target/e6-indent-demo-tests.log`). Strict production Clippy, workspace check, feature-off check, formatting and whitespace gates passed (`target/e6-indent-{clippy,workspace,feature-off}.log`). Regressions cover nested text/marker alignment, wrapped tasks, empty-line selection, structure-only undo relayout, and a full-frame native click landing on the exact painted nested character. Final actual demo capture: `target/e6-indent-final-structured.png`, visually inspected with aligned task text, further-indented nested task, quote text inset from its bar, and no 100-point text gap. An attempted capture to an existing filename refused overwrite; the final unique filename succeeded and prior evidence was preserved.

Principal subsequently accepted the visual positioning ("looking good") and reported two remaining interaction issues; see below. No commit, push, merge or additional helpers. Native OS IME/AT acceptance remains separate.

## Desktop interaction follow-up — 2026-11-05

- **Tab ownership:** the global interaction prefilter treated every editor Tab as focus traversal before native structured input ran. It now reserves plain Tab/Shift-Tab when a structured list item, whole-node selection or native composition owns it, leaving execution to the canonical tree editor stage. Ordinary paragraphs and source/legacy editors retain their existing traversal policy. Readonly list Tab stays focused without mutating the document.
- **Checkbox viewport:** structural pointer routing now returns explicit caret-reveal intent. A checkbox toggle preserves the saved selection and current editor viewport rather than revealing a possibly distant caret. Rule clicks still select/reveal their node; real keyboard editing/navigation still reveals the caret. Structural clicks clear any stale text-drag capture.
- **Regression evidence:** full-frame UI tests cover logical and legacy Tab/Shift-Tab, real indent/outdent with stable focus, readonly and paragraph traversal, and a 100-line task document with an offscreen bottom caret. Checking a top task leaves the selection and viewport unchanged both when already focused and when focus is reacquired; subsequent ArrowLeft still reveals the caret.

Fresh verification: **923 passed, zero failed, three opt-in probes ignored** across `rndr_core`, `rndr_ui`, `rndr_paint` (`target/e6-interaction-regression.log`), including all **20** structured UI tests. Strict production Clippy, workspace compatibility, feature-off compatibility, formatting and whitespace checks passed (`target/e6-interaction-{clippy,workspace,feature-off}.log`). Principal completed the desktop recheck and accepted these interaction fixes on 2026-11-05; E6.1 is closed.

## Remaining E6 slices

1. **E6.2 — actual document tables:** row/cell/header identity, cell/rectangle selection, Tab navigation, row/column insertion/deletion and fragment clipboard/history. Not a retained UI DataTable; resize handles remain deferred.
2. **E6.3 — atoms/images and embedded boundaries:** stable model identity, retained mount/update/detach, explicit selection/copy fallback and permission-free host intent. Same-document nested commits use parent history; foreign-document edits remain host-owned. No execution/network authority.

Principal desktop signoff received on 2026-11-05; the epic E6.1 checkbox is accepted. E6 as a whole remains open for E6.2 and E6.3. E3 latency/storage certification, native OS AT checks and E7 source-fidelity/Markdown decisions remain separate gates.
