# E7 — supported source-preserving staged projection

**Accepted direction and desktop signoff, 2026-11-05.** The principal approved
E7 and requested solidification beyond the experiment. The supported outcome is
an additive, reusable **staged projection layer** and native example, not a
general inline Markdown editor. The contract below is backed by executable
recipes and regression gates. This does not publish a release, ratify all editing
APIs, or certify native platform protocols.

## Decision

Adopt **source-authoritative segment projection** for the supported subset.
Every accepted operation is a revision-checked `SourceBatch` against the existing
`SourceSession`; source owns dirty state, events, selection, composition admission
and undo. The projection never serializes a rich tree or replaces the whole source
buffer to apply a local edit. Two visually equal documents need not have equal
source bytes. Stale maps refuse, including after undo restores identical text.

See the [independent policy comparison](native-document-editing-e7-comparison.md)
for the executable byte model and M6 evidence boundary. Preserved-block custody
can keep untouched raw blocks but may normalize a changed block. Whole-tree
structured normalization is a third, separately named policy. Neither is a silent
fallback for this contract. Actual external M6 code was not executed here.

## Supported subset and explicit limits

`rndr_core::source_projection` is opt-in and additive. It recognizes a deliberately
small **line grammar**, not full CommonMark:

- Literal single-line text, including Unicode, bidi text, spaces and trailing
  spaces; existing line endings are copied exactly.
- One nonempty, nonnested `*emphasis*` pair on a line. The two original delimiters
  are zero-width map segments; their bytes and spelling are retained.
- Unindented `- [ ] `, `- [x] ` and `- [X] ` task prefixes. The display uses a
  portable ASCII task atom. An explicit toggle splices exactly the status byte;
  uppercase `X` stays uppercase while untouched, and a new check uses lowercase `x`.
- Backtick/tilde fenced blocks with at least three matching markers and up to
  three spaces of indentation. Unsupported fences, their contents, line endings,
  and unterminated fences through EOF are one opaque source-reveal atom.
- Other unrecognized lines become opaque source-reveal atoms. This is **not** a
  parser for multiline HTML, nested Markdown, frontmatter, list ownership or a
  cross-line Markdown grammar. Do not use it to infer the semantics of a complete
  Markdown document. Escapes, links, images, inline code, underscore/nested
  emphasis, headings, tables, indented content and structural edits belong in the
  source pane. No resource is fetched or host syntax executed.

A rendered replacement must fit wholly in **one literal segment**, without
crossing a task/opaque/hidden segment. New control characters, line breaks and
syntax characters refuse. A trial splice/reparse must also retain the complete
sequence of nonliteral regions and their exact raw bytes: deleting all text from
`*word*`, introducing a list prefix, or creating a new unsupported line refuses
before source mutation. Selecting/deleting any existing CR/LF byte in rendered
mode refuses (including half of CRLF); use source mode for structural edits.
Empty and BOM-only documents have a legal literal insertion target.

All bytes outside the accepted splice remain exact, including BOM, CRLF, raw
unknown regions, emphasis delimiters, whitespace and other blocks in the same
source document. This is a byte-splice guarantee for this supported policy, **not**
a guarantee that arbitrary Markdown can be edited in rendered mode.

## Position and clipboard contract

A `Projection` captures document identity, monotonic revision, exact source text,
logical display text and a complete monotonic source partition. A
`ProjectionSegment` carries both source and display byte ranges and a `Text`,
`Hidden`, `Task` or `Opaque` kind. Each directed endpoint carries explicit
`Affinity::Before` / `After`. At a hidden delimiter the same display byte can map
to either side of the original syntax. Reversed selections retain direction.
Interior atomic offsets request source reveal, never proportional source mapping.
`ProjectionSelection::inward` is convenient for selecting literal content; it is
not an assertion that every native visual caret has the same affinity.

Only UTF-8 scalar endpoints are accepted by the headless map. Native editor
selection uses the existing shaped/grapheme machinery. Resize and bidi hit tests
resolve to **logical display bytes first**, then go through the segment map.
There is no `source_offset = display_offset` shortcut. Logical syntax affinity is
not the same as bidi/soft-wrap visual affinity; this layer does not certify
all native visual boundary motions.

Clipboard policy is explicit:

- `CopyPolicy::Source`: exact captured source bytes between the mapped endpoints,
  including internal hidden syntax and raw opaque blocks. Endpoint affinity
  controls whether adjacent delimiters/BOM are included. Whole-source copy uses
  Before at display start and After at display end. Partial opaque endpoints
  reveal/refuse. Validate against the live session when using a live selection.
- `CopyPolicy::Rendered`: display text only, including task/opaque labels; this
  is not round-trippable Markdown and is never silently reparsed on paste.
- In the workbench, ordinary native Copy in the left pane copies rendered text;
  copy in the right pane copies source. Paste into the replacement field is
  plain draft text, admitted only on Apply. No structured-fragment codec is used.

`build`, mapping, copying, capture, selection, preedit, mount, source reveal and
preview switching do not dirty the source or add undo entries. Identical
replacements preserve redo. Session readonly/composition/backpressure admission
still applies. A cached projection also checks exact captured source bytes so a
forged snapshot with a matching identity/revision is not silently accepted.

## Reusable host API and lifecycle

The native example now consumes `ProjectionTarget`, not a demo-only wrapper.
The executable module recipe in `rndr_core::source_projection` is the minimal
host composition:

1. Build an immutable `Projection` from the source snapshot.
2. Resolve shaped selection to logical display bytes and explicit syntax affinity.
3. `capture(&source, selection)` validates live identity/revision/**exact bytes**
   and that the target is editable. `original_text()` seeds the native field.
4. Keep replacement text/preedit local. Dropping a target cancels without writes.
5. `target.replace(&mut source, text, origin)` rechecks live authority and current
   readonly/composition/event-backpressure admission. Unsupported input requests
   source reveal. Refusal does not consume the target or silently recapture it.
6. After a real source mutation, rebuild the projection and explicitly recapture;
   every clone of the old target stays stale, including after undo. Identical
   replacements do not change revision, selection, events, history or redo.

`point_from_source` / `selection_from_source` provide the inverse bridge for the
**same captured revision**. Representable scalar source endpoints round-trip
exactly through the display map, including both sides of hidden delimiters/BOM.
Interior atomic endpoints request source reveal instead of approximate snapping.
This does not map old selections through edits or restore native visual affinity.
`copy_live` checks source authority for either clipboard policy; plain `copy`
explicitly reads historical captured data. `replacement_batch` is a lower-level
snapshot planner, **not live admission**; prefer checked `replace`/target methods.
`ProjectionError` supports standard `Display` / `Error` with the underlying
source-admission error retained as its cause.

### Cost and ownership boundary

Build and trial-splice/reparse are O(source bytes + segments), not incremental.
Projection and target captures own snapshots; cloning them copies captured data.
Captures are host-owned and should be dropped on document teardown or explicit
recapture. No worker, async subscription, event consumer or second history is
created. The host must drain the source event stream; backpressure refuses a
write rather than dropping events. This layer makes no document-scale latency
or memory-budget claim. Hosts needing incremental inline projection must add
measured invalidation/selection/IME bridges before broadening the contract.

## Native example and composition

```sh
CARGO_INCREMENTAL=0 cargo run -p source-session --bin projection
```

1. Start with the seeded `quiet` target and replacement `calm`, then **Apply
   replacement**. The source pane changes `*quiet*` to `*calm*` without touching
   delimiters, trailing spaces, CRLFs or raw fences. **Undo source** restores exact
   original source bytes.
2. Select a different literal region in the left pane and **Capture target**.
   The target retains its original projection/revision. Compose/type in the
   native replacement field, then Apply. To delete, use an empty draft; illegal
   empty emphasis or structural deletions reveal source instead.
3. **Toggle task** toggles the task at the rendered caret's line, or the first
   task if the caret is elsewhere. It is an explicit mapped command, not a claim
   of native inline checkbox hit-target interaction.
4. Select an opaque region and **Reveal source** (or try to capture/edit it).
   The exact raw source region is selected in the right pane, without writing it.
5. Capture a target, edit the source pane, then Apply the old target: **stale
   refusal**, no guessed rebase. Recapture explicitly. Source Undo also advances
   revision, so old targets do not revive when the text looks the same again.
6. **Read preview** toggles document-wide readonly mode; navigation and copy
   remain available. Plain source remains visible. There is no second writable
   rendered document or hidden rich serialization history.

The workbench deliberately uses **staged selected-region replacement**, not direct
inline WYSIWYG typing. Native field preedit/candidate geometry and draft undo use
the existing field machinery. Source is unchanged until Apply, which is one host
source transaction. The headless `ProjectionComposition` also supports a local
preedit and one `EditOrigin::Ime` commit. Dropping it cancels without mutation.
Neither path treats display/preedit offsets as source offsets.

During native field or source IME composition, commands to apply, reveal or switch
preview refuse until native commit/cancel. External source edits may occur while
the replacement field composes: the field is only a local draft. Its old source
target becomes stale, and Apply refuses after composition ends. Capture a new
target explicitly; never automatically replay a stale draft. No platform event
stream is synthesized by mode switching. The workbench keeps source selection
history authoritative; after source revisions its passive rendered selection is
scalar-clamped, **not** a promise of exact visual caret restoration through undo.
A production inline projected surface would need a richer selection/IME bridge
and platform acceptance before broadening this policy.

## Verification and acceptance

New E7 coverage includes byte goldens, hidden delimiter affinities, directed and
reversed selection across atoms, exact task toggles, no-op/redo preservation,
insert/delete at emphasis edges, structural/newline refusal, empty/BOM-only
source, malformed/stale/wrong-document/forged maps, shared source undo/redo,
readonly, source reveal, local composition and external-change refusal.
Native workbench tests drive the real replacement field IME path and Parley
shaping, including a logical interior Arabic caret resolved through actual shaped
geometry at two viewport widths. This is not OS IME/AT or full bidi boundary
certification. Native clipboard protocol certification is likewise separate.

### Original experiment verification (before solidification)

The original accepted experiment had these receipts; promotion checks below
must not be confused with these historical totals:

- `CARGO_INCREMENTAL=0 cargo test -p rndr_core -p rndr_paint -p rndr_ui -p source-session`:
  **987 passed, 0 failed, 3 ignored**, 133 result suites including doctests.
- `CARGO_INCREMENTAL=0 cargo test -p rndr_render`:
  **74 passed, 0 failed, 2 ignored**, 7 result suites.
- Combined: **1,061 passed, 0 failed, 5 ignored**, 140 result suites. These are
  this candidate's actual package-scope receipts, not reused E6 totals. The 20
  new E7 cases are 13 mapping/admission, 2 independent custody goldens and 5 native
  workbench integration cases.
- Strict production `cargo clippy -p rndr_core -p source-session --lib --bins -- -D warnings`
  passed; workspace check and core/UI/paint `--no-default-features` check passed.
- `cargo fmt --all --check` and `git diff --check` passed.
- Generated and visually inspected the final public native fixture at 1200×820:
  `target/e7/projection-final.png`, original PNG SHA-256
  `677e1ea06b561ad07eaf44d748a1c6e1337b74dce4946e8819c31650f5cb0edd`.
  Native/offscreen paths share `demo_app()` and `ParleyText::new()`.
  Verified legible task-state fallback, rendered emphasis, readable opaque labels,
  both source/rendered panes, replacement field and reachable command/status area.
  A tool's bounded image rendition may have a different PNG digest from this
  original file; the hash above comes directly from the captured disk artifact.

### Solidification gates

`projection_contract.rs` adds six promotion tests plus the executable public
module recipe. Coverage includes exact inverse source endpoints, captured target
lifecycle/clones, stale/forged map and live-copy refusal, current readonly and
full event-queue admission, standard error integration, and an exhaustive
**70,416-case endpoint/replacement matrix** over Unicode/BOM/CRLF/task/emphasis/
opaque/empty inputs: **1,912 admitted and 68,504 refused**. Every admitted
operation is checked against its exact planned splice and undo/redo; every
refusal leaves bytes, revision, selection, events and history unchanged.

Final solidification verification (single combined package invocation, including
feature unification from the native renderer):

- `CARGO_INCREMENTAL=0 cargo test -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render -p source-session`:
  **1,117 passed, 0 failed, 5 ignored**, 142 result suites including doctests.
  This is the current invocation's total, not a claimed delta from the separately
  invoked historical package groups above.
- Strict production Clippy (`rndr_core` + `source-session`, `--lib --bins`,
  `-D warnings`), workspace check, and core/UI/paint feature-off check passed.
- Formatting and whitespace checks passed. The native fixture was regenerated
  and visually inspected at 1200×820: `target/e7/projection-solidified.png`,
  original file SHA-256
  `91930bd74a1f5d6fd98eec1a622027f788d1f0b285c0f1fb169c3aaf3a073412`.
  Both panes, selected emphasis, atomic fallback labels, replacement field and
  all command/status controls remain legible and reachable. The bounded tool
  rendition can have a different PNG digest from the original disk artifact.

Generated captures/logs live under ignored `target/e7/` and are not committed.
The pre-existing untracked `examples/create-view/target/` is left alone. The
principal's desktop acceptance closes the original E7 experiment gate; native
OS IME/clipboard/AT certification and a complete inline projected widget remain
separate. No release, push, merge or commit is implied.
