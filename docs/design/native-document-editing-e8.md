# E8 — editing compatibility, recipes and certification ledger

Status: bounded local implementation accepted by the principal on 2026-11-06 for closeout commit. E7 was accepted and committed
as `2aa9e28`. E8 is additive documentation/compatibility hardening, not a release,
stable-API ratification, platform certification, or Medousa migration. The native
and performance acceptance gates below remain open, not silently checked off.

## Public entry points and runnable recipes

| Authority | Public API | Executable recipe / composition |
| --- | --- | --- |
| Legacy entity | root `Buffer`, `Editor`, `edit_buffer` | `rndr_core::editing_recipes` explicit-fork recipe; `source_edit.rs`, `editing_compatibility.rs` |
| Exact source | `editing::SourceDocuments`, `SourceSession`, `SourceBatch` | `rndr_core::editing_recipes` source recipe; `rndr_ui::editing_recipes` retained source recipe |
| Structured tree | `structured::StructuredDocument`, typed `Transaction`; `structured_native::StructuredDocuments` | core structured recipe and UI retained tree recipe |
| Staged projection | `source_projection::Projection`, checked `ProjectionTarget` | core projection recipe and `source-session --bin projection` |
| Retained chrome | `rndr_ui::editing::EditorSurface`, `rndr_ui::structured::StructuredSurface` | two executable UI mount/update/destroy/remount recipes |

The recipes are Rust doctests inside their crate packages, not copied Markdown
pseudocode or files outside a package. 0.4.0 publishes `rndr_ui` with these
recipes included. Run `cargo test -p rndr_core -p rndr_ui --doc`.
All six new recipe blocks execute; none is `ignore` or `no_run`.

`rndr` reexports core namespaces without renaming legacy root types. With its
optional `ui` feature, semantic components remain under `rndr::ui`; they do not
shadow legacy `Button`, `TextSelection`, `Buffer` or `Editor`. Source/tree APIs
retain their experimental namespace/status. The E7 bounded staged contract is
supported, but there is no general inline Markdown surface or stable serializer.
Existing names are not deprecated or silently promoted here. 0.4.0 publishes
the crates. It does not ratify a 1.0 API. Source and structured namespaces
stay experimental.

Native examples (fake host only, no document disk writes):

```sh
cargo run -p source-session
cargo run -p source-session -- --extensions --split
cargo run -p source-session --bin structured
cargo run -p source-session --bin projection
```

## Migration without hidden data loss

1. Existing legacy consumers may stay unchanged. Raw Buffer insertion/deletion
   remains history-free; `edit_buffer` retains its legacy entity-owned history
   and exact byte-range signature. It refuses both kinds of managed document.
2. **No legacy undo-stack import/export exists.** `attach_source` replaces the
   entity's Buffer from the session and removes its legacy history. It is not a
   lossless in-place migration. Do not describe attachment as history transfer.
3. For explicit opt-in, copy exact current bytes and a grapheme-valid directed
   selection once into a new session and a new view. Keep the original legacy
   entity/history until the host/user explicitly retires it. The executable fork
   recipe and compatibility test preserve its Undo/Redo, BOM/CRLF/opaque bytes
   and reversed selection. The new document starts with new identity and empty
   history. This is a fork, not a synchronized pair of writable buffers.
4. Managed panes share a document only through attachment; per-view selection
   and scroll are independent, source history is document-owned. Never mirror
   two writable owners or raw-write their disposable Buffers. Source/tree
   attachment now refuses competing authority in **both** call orders before
   mutation; a regression exposed the previously missing source-after-tree guard.
5. Source-to-tree conversion is **not** offered as fidelity-preserving migration.
   `plain_text()` is a copy fallback, not a source codec. Source-preserving staged
   projection uses the existing source/history; unsupported edits reveal source.
6. Host save state is not core document persistence. Capture document/revision/
   bytes, and acknowledge only that snapshot; never reload a late save over new
   edits. The fake-host tests demonstrate the contract, not app-core integration.

Arbitrary scalar endpoints can snap to interactive graphemes by design; exact
selection-copy claims apply to valid grapheme boundaries, not arbitrary bytes.
Unmounting a view preserves its managed document/history; it does not export the
history into a legacy Buffer. Scroll is view state, not a migrated history field.

## Host scheduling, events and extension ownership

`App::update` runs Startup once, then Input → Update → PostUpdate → Layout.
Chrome is idempotently installed; `UiPlugin` composes the same dispatcher, not a
second keyboard/history owner. Source/structured editor input and publication
run in Chrome's PostUpdate editor pass. Host mutations are synchronous, but
visible projection updates follow that pass (structured retained style projection
also runs in Layout). An Update consumer must not assume it already sees native
editor edits from the current frame. Consume after the editor pass, e.g. a host
Layout system, or on the following frame. Registration order matters within a
stage; do not recursively pump `App::update` from callbacks.

Source `ChangeEvent`s are a document-owned FIFO, **not** the transient generic
`Events<T>` stream. `commit` returns a change and also queues it; those are the
same logical notification, not two edits. There are no callbacks inside commit.
Use one host drain owner and fan out immutable notifications to other consumers.
Real content-changing commits, undo, redo and reload advance revisions;
selection/preview and identical writes do not. Refusals never mean an edit occurred. Events are bounded
at 256 entries (not bytes); drain regularly or handle `EventBackpressure` without
silently dropping edits. History is bounded too; unlimited undo is not promised.
Structured trees do not expose the source FIFO: inspect their own revision/state
through their typed API. Do not invent a common source/tree notification type.

`SourceExtensions` owns provider registration IDs and independent request streams.
The host drives `refresh` at input/update boundaries and drains its own worker
results on the UI thread. Request tickets capture document/revision, attachment,
selection and lifecycle policy; transport/executor choice is the host's job.
A result is checked at admission **and** consumption. Higher priority wins;
equal priority uses registration order, not reply arrival. Provider removal
retires its pending work/marks, `cancel_view` explicitly cancels view work, and
late replies refuse. Cancellation is cooperative; never assume worker termination
from a cancellation flag. See [E4](native-document-editing-e4.md) and the public
`source_extensions` recipe contracts; no mandatory LSP client/runtime is added.

Completion popup is passive: it does not take focus or IME ownership. Native
chronological completion keys stay behind IME precedence. Do not add a competing
focus dispatcher. Folding/search/decorations retain exact source coordinates;
display offsets and UTF-16 columns must be explicitly converted/validated.

## Lifecycle and teardown

- Source documents outlive their attached views. `detach_source` removes view
  state without deleting history. Explicit source-store close/removal is not
  currently exposed; drop the host-owned store when that document set is retired.
- One native view per structured document is supported. Detach/destroy before
  `StructuredDocuments::remove`; removal refuses an attached tree.
- Retained handles own internal chrome/attachment. Use `destroy`, not raw despawn
  of their internals. End/cancel composition before teardown; propagate refusal.
  Destroy/remount changes entity identity but retains the managed owner/history.
  A document change under an existing key requires explicit destroy/remount.
- Retained source props control **document-wide** readonly, defaulting to false.
  Reuse the current policy on remount; don't accidentally re-enable every pane.
- Structured rich clipboard is app-local with a plain-text match fallback, not a
  general OS rich interchange format. Foreign-child mount intents are requests
  to the host, not execution authority or parent-history commits.
- Retire provider work and retained embedded-child sessions through their owners.
  Parent/child histories stay separate; foreign/stale child commits refuse.
  Inert images/embeds never imply network fetching or code execution.
- `UiPlugin` systems/resources live for the App lifetime; uninstall unsupported.
  No filesystem autosave, OS clipboard polling loop or extra runtime is installed
  by a headless surface. The runner owns native clipboard/IME/AT/presentation.

## Dependency and feature compatibility

Headless core/UI/paint builds need no GPU/window, async executor setup, language
service transport, Medousa or app-core dependency. Core has a **pre-existing
mandatory `pulldown-cmark` dependency** for read-only Markdown APIs; parser-free
core is not true today. E8 adds no new dependencies or manifests, and staged/tree
editing requires no parser registration. The epic's literal parser-free wording
cannot be certified without a separate compatibility-preserving feature split.
Do not remove preview APIs or change defaults just to manufacture that claim.

The desktop `rndr` facade intentionally depends on the renderer/GPU stack even
with default features off. Use core/UI for GPU-free consumers. `ui` is optional
on the facade, not a flag removing the renderer. Feature-off builds are not proof
of a parser-free or GPU-free facade. `tokio`, language hooks and shaping/runtime
opt-ins remain as documented in each crate manifest.

## Certification / parity ledger

Evidence levels are deliberately different: automated contracts, public-fixture
render inspection, principal desktop signoff, and native protocol certification.
A screenshot/test count never substitutes for the last category. Historical
receipts below remain attributed to their milestone, not relabeled as E8 reruns.

| Surface / capability | Automated / reviewed evidence | Native certification status |
| --- | --- | --- |
| Exact source/session + legacy API | Source, multiview, extension, migration, history and facade fixtures | Principal source/extension demo acceptance; not universal platform parity |
| macOS basic structured UI | E6 interaction regressions + principal 2026-11-05 desktop visual/behavior signoff | Accepted bounded demo; dedicated native IME/clipboard/AT matrix unverified |
| Staged projection | E7 byte goldens and 70,416-attempt matrix, checked targets + principal acceptance | Native replacement field used; not inline Markdown/codec or full IME certification |
| Wrap/bidi/Unicode geometry | Builtin and Parley shaping, selection, resize and AccessKit bridge tests | OS endpoint affinity and screen-reader announcements unverified |
| macOS Japanese/Chinese/Korean/dead keys | Synthetic composition admission/lifetime/input-order tests | Each actual IME candidate/commit/cancel/focus-switch cell unverified |
| macOS clipboard | Core queued cut/copy/paste and exact source policies | OS plain-text round trip, candidate clipboard and cross-app integration unverified |
| macOS VoiceOver | AccessKit text ranges/selection/caret geometry bridge | VoiceOver reading, selection and readonly announcements unverified |
| Native outside-window drag | Timed autoscroll/capture cancellation model | Outside-window backend delivery not universally certified |
| Windows / Linux source/tree/IME/clipboard | Shared headless contracts only; no E8 native runs | Unverified (including NVDA, Orca, X11 and Wayland variants) |
| Structured multipane / inline Markdown / production LSP | Outside implemented subset | Unsupported here, not merely awaiting certification |
| Medousa persistence/echo/save races | Generic fake host only | Separate undertaking; actual integration unverified |

### Performance provenance and remaining gate

[E3](native-document-editing-e3.md) records 21-sample nearest-rank p50/p95 probes
on Apple M4, macOS 27.0.1, Rust 1.98.0, release profile, 2026-11-05. Its repeated-
paragraph shaped 1 MiB fixture observed edit+layout p95 **3.337 ms**, resize (two
CPU frames) p95 **3.548 ms**. The 10 MiB stress fixture observed **33.230 ms**
and **25.230 ms** respectively. These are historical CPU shaping/paint-command
measurements, **not native key-to-present latency** or E8 remeasurements.

E7 projection builds/trial-splice reparsing use owned snapshots and work linear
in source bytes/segments; they are not an incremental document-scale editor or a
certified memory/latency budget.

Per-view paragraph cache has an 8 MiB charged retention budget, not an RSS cap;
full assembled rows still scale with document size (~184.5 MB capacity at that
10 MiB fixture). Source history is bounded at 16 MiB including splice metadata;
oversized snapshots may not retain undo. Events are count-, not byte-bounded.
Native single-huge-paragraph latency, total memory ceilings, source-vs-tree-vs-
projection scale and Windows/Linux performance remain unverified. E0's proposed
16 ms typing / 50 ms resize targets were never ratified as native measurements.
No blanket document-size or throughput guarantee follows from bounded glyphs.

For every new native cell, record OS/backend, machine, Rust/build/features, font/
scale, fixture and exact command, actions and expected/observed outcomes, date,
reviewer and reproducible evidence location. Test each IME independently, readonly
and editable, focus switch/teardown, undo/redo and delayed host changes. AT checks
must exercise actual range actions and announcements, not only tree snapshots.
Keep captures private/public-fixture-only and out of source commits.

## Reproduce local compatibility gates

Run serially with bounded jobs; artifacts go under ignored `target/`, not source.

```sh
CARGO_INCREMENTAL=0 cargo test -j 2 -p rndr_core --test editing_compatibility
CARGO_INCREMENTAL=0 cargo test -j 2 -p rndr_core -p rndr_ui --doc
CARGO_INCREMENTAL=0 cargo test -j 2 -p rndr --no-default-features --test editing_compatibility
CARGO_INCREMENTAL=0 cargo test -j 2 -p rndr --no-default-features --features ui --test editing_compatibility
CARGO_INCREMENTAL=0 cargo test -j 2 -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session
CARGO_INCREMENTAL=0 cargo check -j 2 -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
CARGO_INCREMENTAL=0 cargo check -j 2 --workspace
CARGO_INCREMENTAL=0 cargo clippy -j 2 -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session --lib --bins -- -D warnings
RUSTDOCFLAGS='-D warnings' CARGO_INCREMENTAL=0 cargo doc -j 2 -p rndr_core -p rndr_ui --no-deps
cargo fmt --all --check
git diff --check
```

These isolated facade invocations matter: a combined workspace build can unify
optional features and hide a feature-off regression. The broader regression
invocation is warranted by the changed shared attachment admission, not a claim
that every crate or platform is certified. Fresh outcomes and independent review
are recorded below.

### Fresh local verification — 2026-11-05

- Combined affected invocation above: **1,127 passed, 0 failed, 5 opt-in probes
  ignored**, 143 suites. Includes the six new executable recipes and four new
  core compatibility cases; do not add the standalone recipe rerun to this total.
- Isolated facade invocations: UI off **2/2**, UI on **3/3** passed. These five
  executions are separate from the combined affected total (two common fixtures
  intentionally run twice, under distinct feature sets).
- Strict production Clippy, core/UI/paint feature-off check, workspace check and
  core/UI generated docs with `RUSTDOCFLAGS=-D warnings` passed. The stricter docs
  gate uncovered six pre-existing unresolved legacy/extension API links; those
  links were qualified explicitly, without lint suppression or behavior changes.
- Formatting and tracked/new-source whitespace gates passed across all 13 E8
  candidate paths (generated captures excluded). Final targeted compatibility,
  recipe and strict Clippy reruns also passed after the doc-link-only corrections.
  Commands/logs are local under `target/e8-*`; generated artifacts are not source.
- One read-only independent reviewer checked attachment symmetry, migration,
  scheduling, dependency manifests, historical measurements and certification
  provenance. It independently found the source-after-tree admission gap; the
  host reproduced/fixed it and verified both orders. Review statements were
  reconciled against code and executable behavior: `edit_buffer` does record
  legacy history, unlike raw Buffer mutation. No new native OS test or new visual
  capture was performed/claimed for this documentation/compatibility slice.

E7 is committed. The principal accepted E8 on 2026-11-06 and requested its
closeout commit. This closes the bounded documentation/compatibility slice;
no release, publication, push, merge or native certification is implied.
