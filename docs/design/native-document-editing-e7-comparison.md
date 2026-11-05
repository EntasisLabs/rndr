# E7 — fidelity policy comparison

Experimental decision record, 2026-11-05. This compares guarantees, not just equal
rendered text. No stable API ratification or native IME/bidi certification follows.

## Evidence boundary

Read the local [architecture §5](native-document-editing-architecture.md),
[epic E7](native-document-editing-epic.md), and [evidence register M4/M6/M7/B1](native-document-editing-evidence.md).
The M6 entry records Medousa revision `aa1aa02e339f354c2554c5f4cf0aee7e79676a8f`:
`markdownSourceDocument.ts` retains raw block text, leading whitespace, identities,
fingerprints, BOM/prefix and newline mapping; changed blocks serialize. M4 is a
separate normalized rich codec. The external implementation is **not vendored in
this governed worktree** and was not re-read or executed here. The comparison
below is an executable **policy model**, not a reproduction or test of M6's codec.
Current rndr source sessions provide immutable snapshots, document/revision checked
atomic source batches, shared undo, read-only/composition refusal and event delivery.
That is the write authority; a projection is derived, never another document owner.

## The three policies

| Policy | Authority and write unit | Exact preservation | Principal cost / boundary |
|---|---|---|---|
| Revisioned segment projection | Source session; validated source splice derived from explicit display/source segments | All bytes outside the approved splice, including syntax within the changed block | Hidden delimiters create ambiguous caret boundaries; opaque/replacement spans need explicit affinity and refusal, not guessed offsets. Grammar safety requires a deliberately small editable subset. |
| Preserved-block custody (M6-inspired) | Raw block records plus edited rich block representation; splice/serialize only changed blocks | Original untouched block records and owned separators/prefix | Changed block may normalize delimiters, spacing and newline conventions. Split/join/move/duplicate operations require stable identities and separator ownership; matching by equal display text is insufficient. |
| Structured normalized serialization | Structured document session plus explicitly named codec | Only codec-specific promises (e.g. separately held frontmatter/opaque bytes), not arbitrary original formatting | A whole-tree codec can rewrite even untouched supported blocks. Must never be advertised as source-preserving by default. |

Custody is useful, not an inferior failed projection: it supports broader rich
operations while making a weaker, block-granular fidelity promise. Conversely,
segment mapping is not a complete Markdown editor merely because its bytes map.

## Reproducible distinguishing fixture

Run the following unchanged with `python3` (UTF-8 source file or stdin). The first
block changes `old` to `new`; the second supported block and an unknown fenced
block are untouched. CRLF, BOM, tabs, multiple spaces, alternate emphasis spelling
and trailing spaces make normalization observable. Each policy yields a distinct
byte golden even when the supported visible words are equivalent.

```python
prefix = b'\xef\xbb\xbf'
changed = b'*old*  \r\n'
untouched = b'__keep__\t  \r\n'
opaque = b'~~~mystery\r\n  @raw  \t\r\n~~~\r\n'
separator = b'\r\n'
source = prefix + changed + separator + untouched + separator + opaque

# Segment policy changes exactly three approved source bytes.
a = source.index(b'old')
projection = source[:a] + b'new' + source[a + 3:]
projection_golden = (prefix + b'*new*  \r\n' + separator + untouched
                     + separator + opaque)

# Illustrative block codec chooses underscore emphasis, LF, no trailing spaces.
# Prefix and separators remain owned raw records in this custody model.
custody = prefix + b'_new_\n' + separator + untouched + separator + opaque
custody_golden = (prefix + b'_new_\n\r\n__keep__\t  \r\n\r\n'
                  + opaque)

# Whole-tree normalized policy additionally rewrites the untouched supported block.
# Explicit codec exception retains opaque bytes and the BOM in this example.
normalized = prefix + b'_new_\n\n**keep**\n\n' + opaque
normalized_golden = prefix + b'_new_\n\n**keep**\n\n' + opaque
assert projection == projection_golden
assert custody == custody_golden
assert normalized == normalized_golden
assert len({projection, custody, normalized}) == 3
assert projection[0:a] == source[0:a]
assert projection[a + 3:] == source[a + 3:]
assert untouched + separator + opaque in custody
assert untouched not in normalized
assert all(result.endswith(opaque) for result in (projection, custody, normalized))
for policy, result in [('projection', projection), ('custody', custody),
                       ('normalized', normalized)]:
    print(policy, len(result), repr(result))
```

Normalization choices above are intentionally explicit and illustrative; neither
Markdown's semantic equivalence nor a particular serializer's current output is
being asserted by these byte tests. A real custody codec may preserve CRLF even
in changed blocks; that does not strengthen its delimiter-preservation guarantee.
A codec that drops unknown blocks would fail even this fixture's explicit opaque
exception. Preservation must be tested per policy, not inferred from a parser.

## Recommended E7 boundary

Choose **source-authoritative segment projection for the narrow experiment**:
hidden emphasis, task marker mutation and opaque unsupported block reveal. Name
it experimental. Publish exactly which syntax/regions support edits and reject
anything else with source reveal. Preserve plain source/read-preview modes.
Do not silently invoke a rich serializer when mapping fails. Block custody is a
separately named future adapter policy, not an implicit fallback or a claim that
M6 has been generically reproduced. Keep E5/E6 structured serialization separately
named too. This choice preserves the strongest useful E7 invariant: the approved
source splice is the entire write footprint.

For copy, expose raw-source and visible-text policy explicitly. The visible form
may omit hidden delimiters and substitute opaque labels; it is not round-trippable
Markdown and must not be reparsed as proof of source fidelity. Raw copy at hidden
boundaries needs documented endpoint affinity. Ambiguous partial opaque ranges
must refuse/reveal rather than return misleading partial source.

## Safety review checklist / integration obligations

- Snapshot identity **and** revision must match before every edit/task toggle.
  An old projection with matching revision from another document is still invalid.
  No-op/mount/copy/selection changes must not dirty or create history entries.
- Validate byte bounds and UTF-8 scalar boundaries for both endpoints before slicing;
  projected byte lengths cannot be borrowed as source offsets. Document directional
  anchor/head independently; reversing selection must not reverse a source range.
- At zero-width hidden syntax, honor each endpoint's explicit Before/After affinity.
  Replacement across delimiters must not remove only half a pair. Insertion of
  markup/newlines may invalidate the supported parse; rebuild or refuse, never keep
  using an old segment map after a successful edit.
- A collapsed caret with opposite affinities must not accidentally become a syntax
  deletion. Same-offset and adjacent-segment precedence require explicit tests.
- Opaque spans are not editable merely because a source mapping exists. Selecting
  across an atomic/opaque block must reveal/refuse unless a separately named whole
  source deletion policy explicitly admits it. Test empty/final/unterminated fences.
- Task toggles must target the validated checkbox marker, not any bracket in text,
  code fences or escaped syntax. Decide preservation of `[X]` spelling and list
  indentation; unchanged bytes remain exact. Rebuild after undo and external reload.
- Readonly, composition ownership, event backpressure and stale-revision failure
  must pass through SourceSession with no content/selection/history side effects.
  Mapping a composition commit is not native IME support: preedit/candidate ranges,
  external replacement during composition, and mode-switch cancel/commit remain
  integration obligations. No whole-buffer adapter rewrite to handle one edit.
- Parse conservatively: escaped/backtick delimiters, nested emphasis, CRLF, BOM,
  multiline inline spans, Unicode and unknown syntax must either be demonstrably
  supported or visible source. A one-line recognizer must not guess at full Markdown.
- Logical byte affinity is not bidi visual caret affinity. Width changes must use
  newly shaped display geometry; segment map byte tests alone do not certify resize,
  bidi, drag selection, keyboard traversal, native clipboard, IME or accessibility.
- Undo should restore exact source bytes through the existing source session. Source/
  rendered mode switches must preserve document ownership and not create a second
  history. Projected selection restoration needs its own explicit policy/tests.

## Verification scope

The Python fixture is a policy-level byte comparison, not an external codec test.
Executed successfully: projection **57 bytes**, custody **54 bytes**, normalized
**48 bytes**; all byte assertions passed.

Added `crates/rndr_core/tests/projection_custody.rs`. Executed
`cargo test -p rndr_core --test projection_custody`: **2 passed, 0 failed**.
The tests exercise exact emphasis splices with BOM/CRLF/trailing whitespace,
untouched unsupported blocks, reversed source-copy mapping, uppercase task toggle,
byte-exact undo, and stale projection rejection after both edits and undo.
The tests import the experimental module through its public registration. During
initial test formatting, rustfmt followed a temporary path-module import and also
formatted the host implementation; this incidental formatting was not reverted
over concurrent host work. No host semantic changes were intentionally made.

### Initial implementation review findings (host action required)

Reviewed the first `source_projection.rs` implementation. Identity/revision/text
validation, conservative opaque fence recognition, directed endpoint mapping,
single-literal edit admission and SourceSession commit ownership are present.
A temporary diagnostic probe (removed afterward) reproduced these edge cases:

1. **Empty source cannot accept its first character.** Build from `""`, then
   `replace(inward(0, 0), "a")` returns `RevealSource(0..0)`: point mapping has an
   empty-source special case but no literal segment satisfies replacement admission.
   Add an empty literal insertion target or document source-only empty editing.
2. **Allowed replacement can escape the supported grammar.** For source `"*a*"`,
   delete projected range `0..1`: commit succeeds, leaves `"**"`, and rebuilt
   projection becomes an unsupported atom. For source `"hello"`, insert `"- "` at
   projected offset zero: commit succeeds, leaves `"- hello"`, and also becomes an
   unsupported atom. Banning syntax only in replacement text does not validate the
   resulting line. Prefer trial-splice/rebuild validation with explicit refusal,
   or document that such edits deliberately transition to source reveal immediately.
   This is not untouched-byte loss, but contradicts an unqualified promise that
   unsupported syntax introductions always refuse before mutation.
3. **CRLF is currently an editable literal segment.** In `"a\r\nb"`, deleting
   projected `1..2` succeeds and leaves `"a\nb"`. This is an explicit selected-byte
   edit, not silent normalization, but decide whether rendered editing should permit
   half-newline edits; native grapheme selection normally treats CRLF as one cluster.
   The scalar-byte API alone does not establish the E7 interactive newline policy.

These are exact observations of the first implementation, not findings against a
later host revision. No final failing diagnostic probe is left in the tree.

### Host reconciliation of early findings

All three findings were fixed and converted into passing regressions:

1. Empty and BOM-only source now have an explicit literal insertion target
   (`conservative_grammar_fences_unknown_and_empty_document`,
   `task_atomic_interior_and_bom_only_insertion_are_explicit`).
2. Trial-splice/reparse compares the exact sequence of nonliteral region kinds
   and raw bytes before admission. Emptying emphasis or introducing an unsupported
   list prefix refuses before mutation
   (`unsupported_crossings_syntax_empty_emphasis_and_newline_edits_reveal_without_mutation`).
3. Any selected CR/LF byte refuses rendered replacement, including half of CRLF;
   structural/newline changes use plain source mode (same regression above).

Also fixed composition target admission: beginning composition validates the
unchanged selected bytes rather than simulating deletion of a complete emphasis
span. The real replacement-field IME path, stale target after external change,
source reveal and shaped interior Arabic caret at two widths have separate native
workbench tests. This is staged replacement, not a general inline projected editor.
The [implementation record](native-document-editing-e7.md) documents that boundary
and final exact verification receipts. Native OS IME/AT, clipboard protocols and
all bidi/soft-wrap boundary motions still require platform acceptance. Do not count
the checklist or prior E6 totals as passed E7 checks.
