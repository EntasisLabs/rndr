# Native document editing — architecture and public composition sketch

**Proposal, not an approved or implemented API.** Names in `text` fences are illustrative Rust-like pseudocode, deliberately not compilable doctests. Existing APIs such as `Button::mount`, `UiId`, `Buffer`, `edit_buffer` and `UiTheme` are real; all `editing::*` names below are proposed. The [evidence matrix](native-document-editing-evidence.md) establishes the baseline; the [epic](native-document-editing-epic.md) scopes delivery.

**Implementation follow-up:** the [E1 record](native-document-editing-e1.md) documents the experimental source-only subset now implemented. Its concrete API differs from the illustrative sketches below; source/tree/projection/extensions and multi-view proposals remain future work.

## 1. Boundaries: cohesive sessions, composable presentation

| Layer | Proposed responsibility | Reuse / constraint |
|---|---|---|
| `rndr_core` editing modules initially; evaluate a GPU-free editing crate later | Document/session identity, source transactions/maps/history, separate structured model, selection and composition contracts, command admission, viewport model and extension lifetime | R1–R10. No new dependency direction from core into UI/render/app. Do not make a new crate before measuring coupling. |
| `rndr_core` text/layout/input/semantics | Shaped rows, hit testing, grapheme/caret geometry, focus and input routing, scroll consumption, semantic text-range projection | Share mechanics, not source/tree position types or every navigation rule. Extend existing seams instead of a second layout/event tree. |
| `rndr_paint` and `rndr_render` | Paint selections/preedit/decoration geometry; production shaping; native clipboard/IME/capture and AccessKit adaptation | Core contracts stay GPU-free and testable. Native protocol execution stays in platform adapters. |
| `rndr_ui::editing` (proposed public composition surface) | Retained `EditorSurface`, source/structured recipes, gutter lanes, find UI, completion popup, formatting/table chrome, scrollbar element; UiTheme bridge and accessible names | A surface binds to a coherent session. Removing a gutter does not remove undo; adding a toolbar does not create another editor. Follow existing mount/update/stale-handle/teardown conventions. |
| Optional generic extensions | Source syntax/indent/fold/search hooks; structured schema nodes/marks; Markdown codec/projection contracts | Parsers/providers optional. No mandatory LSP runtime or Markdown interpretation in `EditorSurface`. |
| Application adapter | Domain identity, save/conflict, Markdown/Liquid/embeds, permission and service connections | Medousa app-core remains outside rndr. Blooey is a small fidelity/reference host, not shared application logic. |

### Shared machinery versus separate models

**Recommend a shared session vocabulary with model-specific document and selection types.** Source text owns UTF-8 bytes and source edit maps; structured content owns stable node identities, text offsets, attributes, marks and schema constraints. A transaction envelope can share document ID, base revision, origin, history policy and change notification shape without requiring identical operations or position maps.

Reject for now:
- A tree whose sole purpose is to wrap every source character, or a source string forced to encode arbitrary table/cell/node selection.
- Independent widgets that each own key handling, mutation and history.
- A supposedly lossless Markdown serializer as the common authoritative model.

A model-neutral *view geometry* result can contain typed opaque anchors, rectangles, visual affinity and revision tags. It must not erase the typed model boundary when a command returns to the document. Do not expose a generic `usize` that could mean byte, UTF-16 unit, child index or screen column.

## 2. Source transaction / selection / history contract

Proposed invariants, to be reviewed in milestone E0:

1. `DocumentId` includes generation/identity; monotonically increasing `Revision` is scoped to one document. Undo/redo produce **new revisions**, not reused historical revision numbers.
2. A source batch is expressed against one immutable base snapshot: ordered, nonoverlapping UTF-8 scalar-boundary ranges. Reject overlap, stale base, invalid boundary, wrong identity or readonly state **before** text/selection/history/events change. Adjacent ranges allowed; duplicate insertion coordinates need an explicit ordering rule (initial recommendation: reject ambiguous duplicates). Empty/no-op batch emits no content revision/history step.
3. Commit emits ordered `before/after` revisions, origin token, normalized changes, forward/inverse position maps and affected regions. Changes are in base coordinates, not sequentially drifting offsets. App listeners cannot mutate reentrantly during commit; enqueue subsequent transactions.
4. Source selection is a normalized set of directed anchor/head pairs, a primary range and per-endpoint affinity. E1 can support one range without pretending multi-range exists. Model mapping defines boundary affinity and deletion collapse; interactive carets snap to valid grapheme stops after mapping. Raw source splices retain scalar-boundary validity for exact syntax edits.
5. History belongs to the document session, **not a view entity**. Store transactions/inverses (or measured bounded snapshots temporarily), selection restoration metadata and explicit grouping barriers. Define memory budget, not only entry count. Typing, paste, IME commit, replace-all and app source commands join the same history path. Changing focus or receiving save acknowledgement does not create a user edit.
6. Multiple views share document/history; each keeps its own selection, scroll, visual-goal and focus state. Map all attached view selections after a commit. Initial undo policy: invoking view restores that history step's selection when compatible; other views map normally, never gain focus. E0 must ratify this before shared-pane implementation.
7. Every mutation origin (keyboard, paste, accessibility SetValue/range action, provider, toolbar, embedded callback, external replace) uses admission. No low-level Buffer escape is advertised for a managed session. Existing raw APIs retain their documented legacy behavior.

### Composition / native input

A `CompositionSession` is owned by one focused view/document generation with a base revision, replacement range and preedit selection. Preedit remains transient, excluded from durable text/history and projected at the same layout revision used for candidate geometry. Commit replaces its captured range **once**, producing one undo step; cancellation produces none.

While composing, editor-owned Enter/arrows/slash/completion must not steal IME keys. Existing `resolve_ime` is a reusable filter, not sufficient alone to define revision ownership. Default initial policy: reject app source commands with `Composing`; hold external document replacement in the host until commit/cancel. No silent rebase, composition loss or acceptance of stale queued positions. Define bounded queue/retry/conflict behavior, focus loss, view destruction, document switch and readonly transition explicitly. OS cancellation is a platform request/ack, not something a toolbar can assume happened synchronously.

Clipboard commands retain model intent: source copies selected source; rich copies a declared structured fragment plus plain-text fallback. Projection copy must declare display/source policy; never infer source bytes from rendered labels. Payload validation, image size limits and asynchronous paste insertion tickets belong in the contract. Image import/storage and external content execution belong to the host. First slice supports existing plain text and explicitly declines rich HTML ingestion.

Keyboard semantics include Shift extension, word/document/visual-row navigation, platform modifiers, selection reversal, Home/End/Page, Tab ownership (indent vs focus/table), Escape precedence, readonly selection/copy and deletion across boundaries. Source and structured handlers share routing, not all editing commands. Test native release/repeat and popup focus behavior using the canonical input path.

## 3. Layout, viewport and extensibility

- Reuse `TextEngine`, `TextLayout` and production Parley; layout/hit-test/caret/preedit/accessibility consume one immutable layout revision. Carry upstream/downstream **and bidi visual** affinity as needed, not only byte offset. Shape the exact displayed text, never source with offsets borrowed from a different display string.
- Introduce incremental paragraph/line layout caches keyed by document changes + width/style/font epoch. Viewport tracks variable heights, overscan, stable scroll anchors and document-space coordinates. Preserve anchor through resize/font/theme/decoration changes and edits above viewport. Cache budgets and invalidation/cancellation must be explicit. Benchmark existing paths first; do not prescribe rope versus piece table prematurely.
- Source decorations are revision-stamped ranges with independent layers/priority (selection, search, diagnostics, syntax, widgets). Structured annotations use typed tree anchors. Providers return immutable results with document/base revision and request generation; stale responses are rejected or mapped using retained maps with a bounded lifetime. Cancellation on detach/document replacement is required.
- Command routing has explicit priority and `Handled / Unhandled / Deferred` outcomes. Default precedence recommendation: native composition > modal/focused child ownership > active completion/search > editor commands > host fallback; E0 must decide Escape/Tab exceptions. Commands are semantic operations, not platform key strings. Plugin order/conflicts must be deterministic and inspectable.
- LSP is one adapter, not the editor's internal coordinate system. Convert UTF-16 line/character using the exact source revision; account for CRLF, non-BMP and invalid ranges. Diagnostics include opaque metadata for host actions; no privileged execution through a decoration.
- Interactive scrollbars should bind to the same scroll/viewport controller: thumb drag capture, track paging, keyboard and accessibility range actions, minimum visual thumb vs hit target, axis and clipped interval semantics. Existing `ScrollIndicator` is paint-only; do not claim it already implements this. Reuse latest remainder chaining and editor drag exclusion.
- Accessibility needs text range/selection/caret actions and viewport-aware geometry, editable/readonly semantics, mark/block/table relationships and embedded boundary navigation. Current text exposes a value but SetValue is deliberately refused (`UnsupportedSetValue`) and not advertised; the new validated semantic operations must not reopen the raw-write bypass. A full-document value alone is insufficient. Headless trees test contracts; native VoiceOver/NVDA/Linux AT and IME remain separate gates.

## 4. Structured model and embedded boundaries

Structured transactions validate schema, node/mark attributes and selection before committing; normalization is deterministic and undoable. Positions use node identity + text offset or structural boundary with affinity. Text/node/cell selections are distinct variants. Implement split/join, mark toggles, paragraph/heading conversion and list nesting before tables. Tables require cell navigation/selection, rectangular fragment copy, row/column edits and selection repair after deletion; UI DataTable does not supply these semantics.

An atomic block is selected/deleted as a unit unless explicitly entered. Declare accessible name, focus/Tab entry and exit, copy serialization fallback, readonly behavior, navigation around adjacent atoms and deletion/undo mount lifecycle. Retained ECS content is useful, but existing Markdown slots bind content-at-index and cannot serve as stable document node IDs.

Child editor boundary choices must be explicit:
- **Same-document edit**: child emits a parent-session transaction at its validated node anchor; one history owner.
- **Foreign-document edit**: child emits a host intent/ticket carrying target ID/revision; host authorizes, commits and owns failure/cross-file undo. No incidental parent undo from an embedded button.
- View teardown cancels requests/composition and releases focus/overlays without deleting document state. Undo recreates mounts from the model, not by resurrecting stale entity handles. Provider resources survive only when their document owner survives.

Medousa's Fence/Embed atoms and foreign undo (M5) justify these hooks. They do not justify adding Liquid, permissions, transclusion fetching or cross-file transactions to rndr.

## 5. Markdown: three policies, one honest fidelity boundary

| Policy | Authority / guarantee | Trade-off and acceptance |
|---|---|---|
| Plain source + read preview | UTF-8 source session | Exact bytes preserved; preview doesn't write back. Smallest slice and safe fallback. |
| Source-preserving rendered editing | Source + revisioned projection or preserved block custody | Preserve untouched raw regions, BOM/CRLF/unknown syntax. Map hidden/replacement/atomic segments with both affinities; editable supported regions emit validated source batches. Unsupported regions reveal source. Requires tests beyond existing read slots. |
| Structured document + serialization | Typed tree session + explicit codec | Natural schema operations; serializer may normalize supported syntax. Keep opaque blocks/frontmatter where promised; cannot claim arbitrary source fidelity. |

Actual Medousa uses both rich policies: Vault Live parses/serializes (M4); CodeMarkdownWriter preserves raw untouched blocks using `MarkdownSourceDocument` (M6/M7). That latter is not yet proof of a generic per-character projection. Choose a separate experiment after source/structured foundations: mapping a hidden emphasis delimiter and an opaque block through selection, IME, copy and undo. Compare source-map projection against block-custody serialization before committing a public losslessness guarantee. Keep codecs and Medousa syntax outside the common UI surface.

## 6. Real consumer compositions expressed by the proposed API

These examples use **proposed** names and abbreviate error handling. They show ownership/composition, not a commitment to builder syntax. `mount(world, parent, UiId, props)`, in-place updates and explicit detach mirror existing UI conventions. A mounted surface retains its session ID; adding chrome never creates another history.

### A. Source editor + find + line/review gutters (Code workspace / scripts)

```text
let doc = documents.open_source(initial_utf8);       // durable session, no UI tree owner
let view = SourceView::attach(&mut documents, doc); // per-view selection + scroll
let surface = EditorSurface::mount(world, parent, UiId(10),
    SourceSurfaceProps::new(view).name("Source").wrap(Wrap::Viewport))?;
let numbers = GutterLane::mount(world, surface.gutter_slot(), UiId(11), LineNumbers)?;
let find = FindBar::mount(world, surface.chrome_slot(), UiId(12), view)?;
providers.attach(view, SyntaxProvider::new(language));
providers.attach(view, host_language_service);       // revisioned data, no transport dependency
providers.attach(view, ReviewMarkers::new(base_revision, host_markers));
// completion is optional retained popup chrome bound to the same session
let completion = CompletionPopup::mount(world, surface.overlay_anchor(), UiId(13), view)?;
let change = documents.commit(doc, SourceBatch::at(revision)
    .replace(byte_range, replacement).origin(Origin::Host(command_ticket)))?;
// One undo restores this operation, alongside typing/paste/IME on this doc.
```

Grapheme and automation scripts supply language/completion adapters; work/project source supplies review and LSP services. FileTextDocument can omit providers and mount readonly using the same surface. Readonly admission still blocks provider/toolbar/accessibility/embedded edits, not only key input.

### B. Structured note with format bar, lists and atomic host block

```text
let schema = Schema::new()
    .blocks((Paragraph, Heading::levels(1..=3), Quote, Rule, List, NestedTasks))
    .marks((Strong, Emphasis, Code, Link, Highlight, TextAttributes))
    .node(AtomicBlock::new(host_block_kind));
let doc = documents.open_structured(schema, validated_tree)?;
let view = StructuredView::attach(&mut documents, doc);
let surface = EditorSurface::mount(world, parent, UiId(20),
    StructuredSurfaceProps::new(view).name("Note"))?;
let format = FormatBar::mount(world, surface.chrome_slot(), UiId(21), view)?;
embeds.register(view, host_block_kind, host_mount_adapter); // typed mount + teardown + intent
// Tables arrive as a separate schema/command extension and optional TableChrome.
// Medousa adapter parses/serializes Markdown; framework never interprets Liquid.
```

### C. Source-backed Markdown projection + fallback (future fidelity experiment)

```text
let doc = documents.open_source(markdown_bytes);
let source_view = SourceView::attach(&mut documents, doc);
let snapshot = host_projection.build(doc.snapshot()); // doc ID + revision + segment map
let rendered_view = ProjectedSourceView::attach(doc, snapshot)?;
let source = EditorSurface::mount(world, left, UiId(30), SourceSurfaceProps::new(source_view))?;
let rendered = EditorSurface::mount(world, right, UiId(31), ProjectedProps::new(rendered_view))?;
// Editing a task must return source ranges in that snapshot's revision.
let batch = host_projection.translate(snapshot.revision, task_toggle)?;
doc.commit(batch)?;                                  // same source history; stale refuses
// Unsupported opaque block requests source reveal, never a lossy full rewrite.
```

This is an experiment API, not a required E1 deliverable. A source session with read-only `MarkdownView` preview is the initial implementation target instead.

## 7. Application synchronization (future Medousa adapter)

Use the current `VaultBufferCore` as a **boundary example**, not the foundation dependency. Maintain a mapping `{application document ID/path, toolkit DocumentId, toolkit Revision, app contentRevision, last outbound origin/ticket}`. Those revision domains are not interchangeable. One actor orders local edit, external replace, mode change and save acknowledgement per document.

Recommended flow:
1. Load app snapshot into a toolkit session as a deliberate reset/attach; no undoable user edit or echo event. Keep baseline/hash in app-core.
2. Toolkit local commit publishes an ordered revisioned event. Adapter sends app edit and records origin/ticket. Today M8 accepts full text; batching/throttling must not lose final flush or allow stale replies to replace a newer session.
3. Echo with same origin/revision acknowledges, not re-applies. External clean update uses explicit replacement policy and resets/maps history by declared choice; dirty external update surfaces conflict without silently mutating local text. Initial policy: clear history for a genuine reload, not for an echo or save.
4. Save starts with a captured toolkit revision/content and daemon token. Completion updates the baseline for **that saved snapshot** while preserving newer edits/dirty state. Current `MarkSaved` overwrites content and clears dirty: adapter must serialize/hold newer edits or app-core must evolve a snapshot-aware save-ack contract. Do not invent an `expectedRevision` field in current code. Test out-of-order saves and edits during save in the adapter project.
5. Mode switches explicitly choose source session/projection or tree serialization policy; flush/cancel active composition and async writes before swapping. Never maintain two independently writable full-document authorities without an ordering bridge.

Toolkit history owns generic local editing; app-core owns baseline/conflict, daemon owns persistence. Medousa foreign-document undo remains an application concern. A fake in-memory host must validate the generic event contract first; no app-core dependency blocks E1.

## 8. Decisions for the continuation discussion

Recommended defaults, **not approvals**:
- Source-first slice, one selection initially; document/view split and batch/revision API first.
- Core modules first; keep crate extraction a measured decision.
- One source history owner, independent typed tree model later; no universal position type.
- Plain source + read preview first; preserve-source projection is a separately gated fidelity track.
- Reject stale edits; do not add OT/CRDT/collaboration or automatic async rebase now.
- Initial composition conflicts defer/reject rather than silently remap; explicit native cancellation policy needs review.
- Keep public API experimental/additive until source, structured and projection compositions expose common needs.

Open choices before implementation: batch insertion ordering; undo selection policy across views; external replacement/history policy; provider priority/retention budget; storage choice after benchmark; first native platform gate; clipboard formats/security; accessible text-range representation; source projection vs block custody fidelity promises. Future Medousa adapter must separately decide late-save semantics and source/Live cross-mode history ownership.
