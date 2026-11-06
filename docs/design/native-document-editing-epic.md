# Epic: composable native document editing through rndr_ui

**Status: E1–E7 committed, including all accepted E6 implementation slices and shared appearance defaults. E8's bounded documentation/compatibility slice accepted on 2026-11-06 for closeout commit. Native/platform certification, stable-API ratification and ratified native performance acceptance remain open.**

2026-11-04 continuation: the user subsequently authorized E1, E2 and E3 implementation. See the [E1 record](native-document-editing-e1.md), E2 ledgers, and [E3 implementation/evidence record](native-document-editing-e3.md). Later milestones, release integration and whole-epic acceptance are not implied. The exploration scope below describes the original docs-only assignment.

Created 2026-11-04 against rndr `40a19db725771fcad8199de96638e7b5be557fc9`. This assignment permits exploration and docs only. No production editor changes, external issue publication, integration or self-approval. Sender retains review/native integration. Subsequent user discussion may authorize bounded implementation; unchecked tasks below are not that authority.

## Read / continue here

1. [Source-evidenced requirements and gap matrix](native-document-editing-evidence.md) — actual consumers, implemented/reusable/partial/missing/unverified distinctions and revision anchors.
2. [Architecture, ownership and public API compositions](native-document-editing-architecture.md) — source/tree separation, transactions, IME, history, projection and app synchronization.
3. [Exact source inventory](native-document-editing-sources.json) — revision and content-hash pins; not test receipts.

This uses the repository's existing **local `docs/design/` proposal + checkpoint/task/gate convention**, seen in `A1-api-decisions.md`, `A2-components.md` and `implementation-tracking.md`. There is no discovered dedicated epic tracker or external issue dependency. This epic is the new entry point; old A0/A1/A2 records are historical and are not rewritten into this project's approval. If implementation starts, append bounded checkpoint/evidence summaries here or in a linked execution record, preserving proposals and exact gate status.

## Goal and measurable outcome

Let applications assemble native source and structured document editors through `rndr_ui`, with coherent model transactions, selection, history, layout, native input, extension hooks and retained UI. Cover capabilities **actually used** by Medousa's CodeMirror/Tiptap consumers, without cloning their APIs or depending on Medousa.

A complete epic means independently reviewed generic source and structured compositions, explicit native/platform results, a tested fidelity policy for Markdown editing, documented compatibility, and a separate app adapter contract. It does **not** mean every upstream JS feature, every Medousa feature or every platform is silently considered complete.

### Non-goals

- Implementing the editor during this exploration; replacing Home now; replicating Medousa business logic or ongoing app-core extraction.
- A universal text/tree representation, universal Markdown losslessness by serialization, or a CodeMirror/ProseMirror API clone.
- Collaboration/CRDT/OT, arbitrary plugin execution, built-in LSP server transport, daemon permissions/filesystem ownership, Liquid runtime or foreign-note execution.
- Fixing DataTable visuals, unrelated UI polish, broad framework cleanup, publishing, release enablement, pushes or deployment.
- Treating existing headless tests/captures as OS IME, clipboard, screen-reader or cross-platform certification. The scripted CJK/Arabic matrix in [ime-complex-scripts.md](../ime-complex-scripts.md) is headless only. macOS and Windows input-method and physical-DPI cells stay open.

## Key findings and recommended shape

- rndr already has byte Buffer/Editor, wrapping/hit testing, keyboard history, validated `edit_buffer`, source styling/decorations, IME/clipboard/platform seams, read Markdown slots and UiTheme bridge. **Build on them.**
- Main gaps: document-owned revisioned transactions/maps/history; explicit view/session split and provider/input contracts; document-scale costs; editable structured model and fidelity-aware projection.
- Actual consumers extend beyond vault: Code workspace, work source, text files, automation/Grapheme scripts and **CodeMarkdownWriter**. The latter preserves raw untouched blocks; Vault Live uses whole-tree Markdown serialization. Preserve that distinction.
- Below UI: model/session/input/geometry/semantics. UI: retained editor surface and optional gutters, find, completion, format/table chrome and interactive scrollbar composition.
- Keep source and structured models separate; share native machinery and transaction vocabulary where semantics match. Markdown projections/codecs are optional adapters.
- Current core has nested wheel remainder chaining and clipped **passive** indicators. Interactive scrollbars remain relevant work; old header styling feedback does not justify a detour.

## Dependency graph and delivery discipline

```text
E0 contract decisions / baseline measurement
 └─ E1 smallest end-to-end source slice
     ├─ E2 native selection, IME, accessibility and interactive viewport
     ├─ E3 incremental performance + multi-view ownership
     └─ E4 source extension protocol and representative language features
E1 + E2 contract gates ─ E5 structured model vertical slice
E5 ─ E6 lists/tables/atomic embedded boundaries (bounded sub-slices)
E3 + E5 + E6 boundary contract ─ E7 source-preserving projection experiment
E2/E3/E4/E6/E7 ─ E8 compatibility, documentation and certification ledger

A separate future Medousa adapter undertaking may start after E1's generic
contract is stable. It does not gate E0–E7 and is not part of framework scope.
```

Each milestone below is a **bounded review unit**, not a single massive assignment. Where a milestone has several sub-slices, land/review them separately. Preserve existing user work and source behavior; no broad rebuild for a small contract change. Before implementation, refresh current source and these evidence claims, especially app-core. Review API contracts at E0/E1/E5/E7 before promoting experimental types.

## E0 — ratify contracts and characterize existing costs

**Depends:** this exploration. **Output:** a short ADR/checkpoint + bounded benchmark fixture plan, not production editing features.

- [ ] E0.1 Decide document/view identity and revision domains; source batch coordinate/overlap/insertion rules; readonly admission; no-op/event policy; history/undo selection ownership across panes.
- [ ] E0.2 Specify IME session ownership, external replace/retry behavior, keymap priorities, clipboard fallback and provider lifecycle; identify minimal native text-range accessibility additions.
- [ ] E0.3 Characterize existing Buffer gap move/line rebuild, history snapshots and changed-width/text layout at 10 KiB, 1 MiB and 10 MiB; separate many-line and single-long-line, ASCII and mixed Unicode, typing at beginning/middle/end, replace-all, undo, idle scroll and resize.
- [ ] E0.4 Choose first native platform (recommend macOS as current host), initial supported document envelope and provisional budgets. Recommend 1 MiB/20k-line fixture: warm p95 key-to-layout <16ms and p95 resize <50ms on a named machine; these are **proposed budgets, not measured claims**. 10 MiB is characterization/stress, not immediate parity. Ratify or amend using E0 data, including total/history/cache memory caps.

**Acceptance:** reviewed decision record includes negative cases, event ordering and concrete chosen budgets; reproducible measurement commands/source/machine/font config and allocation/layout counters; no rope/piece-table decision merely by intuition. Missing native capabilities are explicit blockers/gaps, not hidden as passes.

**Tests:** contract examples for stale edits, reversal/affinity, overlapping batches, echo/save races and composition switches; baseline measurements in fresh private output, no tracked bulky logs. E0 is a decision/measurement gate, not proof that current implementation meets targets.

## E1 — recommended smallest end-to-end slice: source session + native surface

**Depends:** E0 contract decisions. **Scope:** one selection, one document, one view, plain text; no LSP, structured tree, source projection or Medousa dependency.

- [ ] E1.1 Add an experimental source document session with ID/revision and atomic validated multi-range batch. Inverses/maps/change events required even while UI offers one caret. Reject stale/overlap/split-codepoint before side effects.
- [ ] E1.2 Route typing, paste, IME commit, undo/redo and a host range command through **one** history/transaction owner; no competing keyboard and app histories. Reuse current grapheme/input/layout primitives. Define adapter to legacy Buffer, do not silently alter raw APIs.
- [ ] E1.3 Expose retained `rndr_ui` source-surface composition with line-number option, accessible name, readonly mode, existing theme bridge, selection and scroll. Fake in-memory host receives revisions/dirty status; no app-core dependency. Include read-only Markdown preview if useful, but no write-back.
- [ ] E1.4 Add a small native gallery showing typing → host event → dirty indication → host source splice → undo/redo → captured save acknowledgement in the fake host. Test delayed reply and external reload behavior, rather than only a static text area.

**Acceptance:** ordered revisioned event per real commit, no event/no history on no-op; stale and composing host commands refuse without losing selection/focus/input/redo; keyboard undo steps restore typing and host splice separately; IME commit once; readonly blocks every origin; mounting/updating/detaching chrome preserves document history. Fake host acknowledges saved snapshot without replacing newer edits. Surface teardown releases resources without destroying its document. Public sketch updated to actual code and compatibility boundary.

**Tests:** model tests and randomized batches against String reference; real Chrome/Input sequence tests (queued and standalone IME); UI lifecycle/theme/readonly tests; headless Parley Unicode source/resize/hit testing; bounded native macOS smoke and explicit unverified OS cells. Preserve existing `source_edit`, editor navigation/drag, clipboard/IME and render source tests. Do not claim E2 completeness from this smoke test.

**Why this first:** proves composition plus the hard transaction/history/host boundary end to end, without waiting for Medousa extraction or designing the whole rich editor.

## E2 — native interaction, semantic ranges and viewport contract

**Depends:** E1; implement as E2.1 input, E2.2 semantics, E2.3 scrollbar units.

**Implementation status:** E1 plus demo polish committed as `aab3860`;
[E2.1 input](native-document-editing-e2-input.md) committed as `3042621` adds
shaped Page navigation, explicit Tab ownership and composition lifetime guards.
[E2.2 semantic ranges](native-document-editing-e2-semantics.md), accepted and
committed as `6afdb36`, add source-fenced selection and shaped geometry through
the AccessKit runner. The user then authorized finishing E2; the
[E2.3 viewport and closeout ledger](native-document-editing-e2-viewport.md)
records interactive scrollbars, elapsed-time selection autoscroll, input ownership,
896 passing integrated tests (one opt-in probe ignored), final independent review,
and exact certification limitations. Implementation closure is distinct from the
original native acceptance gate: the checkboxes below remain open until the named
native/affinity checks are actually exercised. No E3 implementation is included.

- [ ] Define visual/logical movement and endpoint affinity across wraps/bidi; platform word/Home/End/Page, Shift extension, selection reversal and Tab/Escape ownership. Complete composition lifecycle across focus loss, readonly, teardown and external changes.
- [ ] Add native text-range/selection/caret actions and geometry; focused editable/readonly exposure. Reuse AccessKit and runner boundaries, not direct OS access in UI.
- [ ] Bind an interactive scrollbar to the same editor viewport controller: thumb drag/capture, track paging, keyboard/a11y actions, minimum hit area, clipped interval and both axes. Retain existing passive indicators for noninteractive consumers; no DataTable redesign.
- [ ] Capture off-editor/out-of-window selection drag where supported and schedule stationary autoscroll with time, not incidental frame arrival; document platform limitations.

**Acceptance:** composition cannot be stolen by menus; no duplicate commit after same-frame keys; candidate rectangle follows actual shaped preedit under resize/scroll; selection/hit geometry consistent at mixed RTL/LTR and wrap affinities; nested wheel remainder behavior preserved; interactive bars don't create parent scroll or selection drags; accessible selection action edits intended document or refuses safely.

**Tests:** combining/ZWJ/flag/Arabic+Latin/CRLF corpus; key repeat/release, lost capture, clipped popup/viewport; platform clipboard text/primary and image request/fallback where supported. Native ledger: macOS VoiceOver + Japanese/Chinese/Korean/dead-key IME and clipboard; Windows NVDA/IME; Linux AT + X11/Wayland primary/capture. Record actual exercised combinations, not a blanket matrix pass.

## E3 — document-scale performance and multi-view ownership

**Depends:** E1; coordinate viewport semantics with E2.

2026-11-04: [E3 source implementation candidate and measured limits](native-document-editing-e3.md). Shared views, exact peer/viewport maps, incremental paragraph geometry, idle snapshots and demonstrated model hot-path improvements are implemented. The milestone remains unaccepted until native budgets are ratified/measured or explicitly renegotiated; folding/provider hooks remain E4-dependent. These original checklist items are not a blanket native performance pass.

- [ ] Replace demonstrably costly whole-document edit/index/history paths as measurements warrant; select storage only after E0. Incremental paragraph/line layout and bounded cache/map retention; huge-line behavior explicit.
- [ ] Add per-view selection/scroll over one document history; map inactive view positions and keep focus stable. Retire/reopen views without losing document state.
- [ ] Preserve scroll anchor across edits above viewport, style/font/width changes, folding and provider updates; avoid full-document work on idle scroll.

**Acceptance:** ratified E0 budgets met on named fixture/machine or explicitly renegotiated before claiming complete; edit/undo/cache memory measured; bounded glyph/row publication verified separately from total document layout work. Two views see one transaction; undo doesn't steal other-view focus; stale views/maps fail safely. No data loss on cache eviction.

**Tests:** deterministic cost counters + timed benchmark distributions/memory, 10 MiB stress, long-line failure behavior, Unicode boundaries, multiple views, detach/reopen, resize/theme/font epoch and cancellation. Retain before/after commands and provenance, no giant checked-in captures.

## E4 — source selection and extension composition

2026-11-05: implementation accepted by the principal after native demo retest. See the [E4 closeout and verification boundaries](native-document-editing-e4.md); this does not certify separate native IME/accessibility/platform or E3 performance gates.

**Depends:** E1; performance acceptance relies on E3; source features can be reviewed individually.

- [x] E4.1 Normalize multiple directed selections and rectangular visual selections; multi-paste/delete/undo as a batch, explicit tab/bidi column policy.
- [x] E4.2 Revisioned decoration/provider lifetimes and priority; search/replace controller + optional find bar and generic gutter markers.
- [x] E4.3 Optional syntax/indent/bracket/fold provider hooks; one representative provider and fold geometry, not all language packs.
- [x] E4.4 Completion popup + diagnostics/navigation hooks with a fake asynchronous provider. UTF-16 conversion seam for a future LSP adapter; stale result/edit refusal and cancellation.

**Acceptance:** composition can express M1/M2/M9's needed capabilities without special cases named Medousa; replace-all one history step, zero-length search bounded; fold selection/copy explicit; provider detach causes no stale popup/decoration; key precedence deterministic under IME and completion. App supplies language service transport, review semantics and grammar/slash commands.

**Tests:** fake delayed/out-of-order results, changed document identity, CRLF/non-BMP conversion, multi-edit overlap, rectangular selection on tabs/wraps, provider teardown/focus/readonly; incremental syntax invalidation and viewport cache regressions.

## E5 — separate structured-model vertical slice

**Depends:** E1 session vocabulary + E2 input/selection contracts; no requirement to finish every E4 language feature.

- [x] Review typed tree positions/selection and schema transaction API before exposing it publicly; keep source byte types separate.
- [x] Implement paragraph + heading + strong/emphasis + link, split/join, marks and deterministic normalization, directed text/node selection and history. Use one structure-aware surface + optional formatting UI.
- [x] Demonstrate a minimal generic structured document in gallery, with no Markdown codec required. Share layout/native machinery only where proven compatible.

Implementation, contract review and verification: [E5 evidence](native-document-editing-e5.md). Principal tested and accepted the desktop demo on 2026-11-05 and authorized committing E5. This closes implementation/demo acceptance, not the separate native/platform certification gates.

**Acceptance:** schema-invalid operations refuse atomically; split/join/mark commands maintain selection and inverses; toolbar focus does not lose saved selection; IME across marked boundaries and readonly are defined; serializing to a generic test format does not masquerade as Markdown fidelity. API review compares source and structured compositions without forcing a common representation.

**Tests:** tree invariants and transaction inverse/property tests, node deletion, empty document, mark boundary paste/IME, undo coalescence, keyboard and accessible block navigation, headless shaped geometry plus native structured smoke.

## E6 — structured extensions and embedded editing boundaries

**Depends:** E5; each line below is a separate bounded slice/review.

- [x] E6.1 Quote/HR, ordered/bullet/nested task lists, generic attributes, text-style marks and optional annotation/footnote extension points matching M3. Define nesting/merge/split and checkbox commands through model history. Accepted 2026-11-05 after principal desktop rendering/interaction recheck; see [E6.1 evidence](native-document-editing-e6.md).
- [x] E6.2 Editable tables: rows/cells/headers, cell/rectangle selection, Tab navigation, insert/delete row/column, fragment copy/paste and selection repair. Resize handles are **not** required for current Medousa parity; keep them deferred. Accepted 2026-11-05 after principal mechanical tryout and shared-default desktop visual signoff. See [contracts and evidence](native-document-editing-e6-extensions.md).
- [x] E6.3 Atomic block/image extension with retained mount/update/detach, model-stable node identity, selection/copy fallback and permission-free host intent. Same-document nested commit uses parent history; foreign-document commit remains host responsibility. Accepted 2026-11-05 after principal mechanical tryout and shared-default desktop visual signoff. See [ownership/lifecycle evidence](native-document-editing-e6-extensions.md).

**Acceptance:** required blocks/marks can be composed from schema extensions; atomic boundaries do not trap Tab/arrows/IME, leak events into parent, mutate readonly content or resurrect stale handles through undo. Child deletion tears down overlays/requests and repairs focus. Tables have document semantics, not a UI DataTable masquerading as an editor. No Liquid/transclusion/network execution is added.

**Tests:** list/cell model invariants/inverses and keyboard matrices; block insertion/deletion undo, adjacent atoms, focus return and teardown while async callback pending; clipboard fallback; nested wheel/drag ownership, accessibility structure and history isolation. Native AT/table checks separately recorded.

## E7 — source-preserving staged projection and fidelity decision

**Depends:** E3 + E5 + E6 boundary contract. **Gate:** choose projection versus preserved-block custody based on experiment before a stable public API.

**Accepted and solidified (2026-11-05):** principal desktop signoff accepted the experiment and requested promotion. [E7 supported contract and native example](native-document-editing-e7.md), with [independent custody comparison](native-document-editing-e7-comparison.md). Chosen policy: source-authoritative segment splices for a deliberately narrow grammar, staged selected-region replacement, and source reveal for unsupported edits—not a general inline Markdown surface. Shared `ProjectionTarget` capture/apply, checked live copy, inverse source/display selection maps and exhaustive admission gates replace demo-only target ownership. This closes the bounded E7 gate, not full inline editing, native certification or all-API release ratification. Accepted E7 was committed as `2aa9e28` on 2026-11-05.

- [x] Preserve source for a hidden-emphasis segment, an editable task and an opaque unsupported block; implement explicit map segments/revision/affinity and source reveal fallback.
- [x] Compare generic source projection with M6's block-custody strategy; test untouched raw-region preservation versus changed-block normalization. Structured serialization is a separately named policy, not the same guarantee.
- [x] Publish chosen supported subset, copy policy, IME/external-change behavior and unsupported fallback; retain plain source/read-preview mode.

**Acceptance:** exact unchanged BOM/CRLF/whitespace/opaque fences, Markdown delimiters and unknown constructs preserved for the supported source-preserving policy; all writes use current source session/history and stale projection refuses. Same display is never proof of same source. Mount/selection-only/preview does not dirty the document. No offset borrowed from parsed display text is used as source offset.

**Tests:** byte goldens (B1/M6-inspired, generic fixtures), reversed selection across hidden/atomic segments, insert/delete around hidden delimiters, resize/bidi affinity, IME within supported region, source/rendered mode switching and undo; compare edit elsewhere leaving raw unsupported block untouched. Unsupported combinations reveal source rather than normalize silently.

## E8 — public API compatibility and release-quality ledger

**Depends:** relevant E2/E3/E4/E6/E7 gates; stable API ratification is separate from local integration.

**Accepted local implementation (2026-11-06):** [E8 recipes, compatibility and certification ledger](native-document-editing-e8.md). E7 is committed as `2aa9e28`; the principal accepted E8 and requested its closeout commit. This closes the bounded local deliverables, not native/platform/performance certification or stable-API ratification.

- [x] Publish actual source/structured/projection recipes, extension ownership, teardown and event scheduling docs; six new executable core/UI doctest recipes plus the native compositions.
- [x] Compatibility fixtures for legacy `Buffer`/`Editor`/`edit_buffer`, root facade exports and isolated UI feature-off/on builds; no new mandatory GPU/parser/runtime/app dependency. **Clarification:** core already requires its preview parser; the desktop facade already requires render/GPU. GPU-free core/UI composition needs no parser setup, but parser-free core is a separate unimplemented dependency split, not a claim of E8.
- [x] Maintain native/platform/IME/AT and performance ledger, explicitly marking unsupported/unverified cells and narrowing parity claims. One read-only independent review checked evidence provenance and the behavioral path; the reciprocal source/tree attachment gap it independently identified is fixed and regression-tested.

**Acceptance:** documented migration does not lose source bytes/history/selection; stable names follow reviews at E1/E5/E7. Existing core/facade APIs are not renamed or silently changed. Broad checks only when warranted and capacity is safe; owner/sender review and governed integration required. No release/publish implied.

## Separate future Medousa adapter undertaking (not a framework milestone)

Refresh app-core and consumer code first. Adapter translates document/revision identity and full-text `ApplyEdit` today, suppresses echoes, flushes final edits, coordinates composition/mode switching and surfaces external dirty conflicts. It must decide **late-save acknowledgement**: current `MarkSaved` replaces content/baseline and clears dirty without expected revision, so newer edits require serialization/holding or an evolved app-core contract. Do not add a hidden dependency from rndr to solve it.

Acceptance for that future scope: native pane typing → app dirty → undo/redo → save captured revision → edit during save → late ack preserves newer edit → external clean reload / dirty conflict, multi-pane ordering, stale async provider and foreign embed undo. Test actual app-core semantics then; fake-host tests in E1 are not integration proof. Cursor's ongoing extraction can proceed independently.

## Compatibility / migration plan

1. Add experimental session APIs and UI composition additively. Keep existing `Buffer`/`Editor`/raw operations and `edit_buffer` contract intact for legacy entities: raw Buffer mutations are history-free, while `edit_buffer` records legacy undo/redo. There is no history-import API; E8's explicit fork preserves the old entity/history and copies exact bytes plus valid directed selection into a new managed owner.
2. Decide opt-in managed-session attachment. Reject simultaneous legacy and session input ownership on the same entity; explicit one-time migration/copy must identify which history/selection survives. Do not mirror two independently writable buffers.
3. Port a generic gallery first, not every host. Evaluate source-backed Blooey fixtures as reference only. Migrate Medousa only in its separate authorized project.
4. Promote stable API after real source/structured examples and fidelity experiment. Any eventual deprecation needs written replacement, versioning and tests, not silent root export changes.

## Risks and decisions for the user

| Decision / risk | Recommended next choice |
|---|---|
| Too much architecture before a usable pane | Ratify E0 decisions, then authorize **E1 only**, with source surface and fake host as the first demo. |
| Shared machinery becomes a universal model | Keep typed source/tree models and history implementations; share envelope/native geometry where justified. |
| App-core evolution stalls framework | No app dependency in E0–E7; keep adapter as separate scope. |
| Snapshot/gap/layout costs dominate | Measure first; adopt incremental storage/index/history only where evidenced. |
| Composition/focus/async corruption | Reject stale/composing mutations and design lifecycle before extension explosion; native gates cannot be inferred. |
| Fidelity promise overreaches serializer | Keep explicit source-preserving vs normalized rich policy; opaque fallback and E7 gate. |
| API too early / plugin conflict | Experimental namespace, deterministic command/provider lifecycle, review at source/tree/projection boundaries. |
| Multi-pane undo ambiguity | Choose document-owned history and invoking-view restore policy at E0; don't improvise later. |
| Platform surface larger than budget | Choose macOS first; keep Windows/Linux cells visible and unverified until exercised. |

### Immediate continuation prompt

“Let's review E0's source transaction, history and composition decisions, choose the first native platform/performance envelope, and authorize the E1 source-session + rndr_ui surface + fake-host demo. Keep Medousa integration and structured implementation out of that first slice.”

This is a suggested next request, not authority to execute it now.

## Exploration completion / verification record

- Bounded source/reference inspection and test-target inventory completed; revisions and anchors linked above. No fresh build/tests/native run or performance measurement. No production editor behavior changed.
- Documentation validation passed: three Markdown files with balanced fences and eight relative links; README epic entry point present; all 51 manifest pins matched their exact Git revision/blob SHA-256. Inspected reference working bytes remained identical. `git diff --check` and `git diff --cached --check` passed; staged scope was exactly README plus these four design artifacts. This does not validate pseudocode as a real API.
- Only local isolated rndr documentation changes are intended. No Medousa/Blooey changes, extra agents, external issues, publishing, push, self-approval or merge.
