# Combined DataTable + PropertyInspector review candidate

**Implementation delivery only — not sender acceptance, independent review,
approval or native integration.** The candidate is the commit containing this
report; its full OID is supplied in the terminal delivery. Parent audit:
`132258c535b7c75dcc7256b1eea89e913eaaed15`; original integrated source baseline:
`57c30a3242efcea7d54a6083251575ac04f1c603`.

The revised assignment was followed: repair editor prerequisites and continue
straight through **both** reusable components and their joint gallery, with no
intermediate prerequisite-review pause. The sender retains responsibility for
one combined independent review and any subsequent native integration.

## Delivered behavior and architecture

* Canonical `FieldTransactions` is an **opt-in** `TextField` seam under the
  existing `GenericInteraction`: rich keys are edited at their chronological
  focus target, Submit snapshots precede later queued text, repeat Enter does
  not request another commit, captured Escape reaches a typed cancel hook, and
  focus loss notifies synchronously. Existing edit/history/clipboard/caret/IME
  machinery remains authoritative; a whole field-input stage is not replayed
  per key. Explicit owner `reset_field_draft` clears local history/caret/preedit.
* `rndr_ui::data::DataTable` composes `ListView`, canonical header controls,
  layouts and scrolling. Stable row/column identity retains passive custom cell
  subtrees through reorder/sort/resize. Host sorting is requested, never hidden.
  Row focus, selection, activation and keyboard navigation use the existing
  collection contract. Table/Row/ColumnHeader/Cell roles and supplementary header
  descriptions reach the native accessibility bridge without replacing values.
* `PropertyInspector` groups explicitly described text/number/boolean/choice
  properties and caller-owned custom editors. Local raw/parsed/error drafts,
  deterministic validation and explicit identity-bearing commit/cancel events
  are separate from host values. Text/number use canonical fields; boolean/choice
  use existing controls. Focus transitions, external reconciliation, disabled /
  read-only states, object switches, removal and unmount are handled and tested.
* `data_editors_gallery` is a standalone synthetic table-to-inspector host. It
  applies sort/commit requests to an application-owned eight-record model and
  exercises selection transitions, invalid drafts, resize, reset/remove/filter,
  empty, disabled/read-only, light/dark, density and both UI/reading text scales.
  Narrow/scaled panes and toolbar stack, with ordinary nested scrolling.
* Compiled public composition examples and a full ownership/API/limitations
  guide are in `docs/data-editors.md`. Fresh-directory verification and real
  native capture scripts are maintained alongside the code.

## Exact final verification

The authoritative passing run is **`verification-final-3/`**, not earlier
iterations. Every command records its expanded invocation and `EXIT=0`.
Environment: offline/locked Cargo, two jobs, no incremental or dev/test debug
info, existing compatible cache:

```
/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target
```

| Command / gate | Result |
|---|---:|
| `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui` | 599 passing executions, including documentation tests |
| Original six reproductions + two activation-policy probes copied byte-for-byte into temporary test targets | 8 passing |
| `cargo test --offline --locked -p rndr_render --lib --features observe` | 74 passing |
| `cargo test --offline --locked -p rndr_render --lib --no-default-features` | 25 passing |
| `cargo test --offline --locked -p rndr --lib --features observe` | 1 passing |
| `cargo test --offline --locked -p rndr --lib --no-default-features` | 1 passing |
| Native observe gallery build / fallback gallery check | Both pass |
| Targeted rustfmt check / `git diff --check` | Both pass |

**708 passing executions total**, not 708 unique tests: some suites execute in
more than one feature configuration. There are no compiler warnings in the final
command logs. Full exact commands, log digests, **90 test-source hashes** and
extant compiled test executable hashes/paths are in `audit-receipt.json` and
`test-source.sha256.json`. Ephemeral rustdoc test binaries are not claimed as
retained executable artifacts. Raw logs retain their output whitespace; an
evidence-local `.gitattributes` disables whitespace lint for these logs only,
without changing receipt bytes or production-source checking.

New maintained suites include:

* `field_transactions.rs`: **9 tests**, including all **five saved boundary
  cases**, plus IME suppression/deduplication, readonly selection/copy, history
  reset and synthetic/released/repeat admission.
* `data_editors.rs`: **13 tests** of public component identity, sort/keyboard /
  accessibility projection, validation, exact commits, safe selection changes,
  object/history reset, grouping/custom composition, disable/read-only, bounds,
  invalid atomic mounts, stale handles and lifecycle cancellation.
* `data_editors_gallery.rs`: **5 tests** of real host model wiring, nonzero and
  correctly aligned text/cell geometry, nested responsive scrolling, every
  synthetic state including disabled, and independent reading-scale line boxes.
* Native accessibility bridge regression preserves cell text while exposing
  Table/Row/ColumnHeader/Cell roles and live header descriptions.

The existing **12 maintained chronology regressions** remain unchanged and pass.
The original six and two policy fixtures are copied by the verification script
without editing their evidence sources and removed only from the temporary test
locations afterwards.

### Five saved boundary reproductions: compatibility policy

The historical baseline probes remain byte-identical. The maintained field
suite adapts those cases to the new **public canonical opt-in marker**, retaining
snapshot/target/repeat assertions. Captured Escape now uses the typed Cancel
notification, not a captured field leaking into tree-navigation dispatch.
**Unopted legacy fields intentionally retain their previous behavior.** Running
old ignored baseline probes without the marker is not a test of the new API and
is not claimed to pass. The inspector uses the canonical seam, not a private
input replay or app-specific editor. This explicit compatibility choice is part
of the combined review surface.

## Native visual evidence

Authoritative final matrix: **`native-final-4/`**, built from the exact **131-file
source manifest** in that directory. Binary SHA-256:

```
c236f2b15821d4a3572b86aefe5a2189f319ecff2896bc54ed9447250ffb5d2d
```

**12 states, 24 captured frames**, Metal + Parley on macOS. All preset processes
started/captured/shut down normally and observation cleanup passed. Binary,
source, PNG bytes/dimensions/SHA and metadata hashes are verified; frame revisions
increase. The assistant inspected the actual `frame-2.png` pixels for all twelve
states. `visual-inspection.json` records those paths/hashes and findings.

States: light, dark, 380-point narrow, narrow scrolled to inspector, compact /
high-contrast / 200% UI **and reading/code text**, scaled inspector, scaled lower
inspector, focused invalid numeric draft, read-only, disabled, empty/no selection,
and lower Boolean/Choice workflow editors.

**Honest origin limit:** the repository protocol returns
`source=live-surface-copy-before-present` and
`presented_confirmation=false`. These are actual native GPU pre-present surface
copies, not HTML/mock screenshots, but they **do not prove after-present /
compositor or physical monitor output**. Preset state is synthetic; no physical
keyboard/mouse, real OS IME or live AT operation is claimed.

## Findings fixed during implementation

Native pixels exposed empty table cells caused by Fill/Hug height/width cycles;
these are fixed with intrinsic cell sizing. A later native inspection exposed
fractional-width snapping that wrapped percent signs into a clipped second line;
aligned passive labels now reserve a guard point using the same active canonical
text engine after theme resolution, with a maintained geometry regression.
The full disabled gallery preset then exposed a forbidden programmatic selection
setter on a disabled collection. The host now preserves the already-reconciled
selection rather than invoking that setter; all preset reconciliations are
maintained regressions. Enlarged reading text has its own line-box regression.

Earlier captures, failed evidence-gate assumptions and test runs are retained
as **intermediate/unaccepted debugging evidence**, not substituted for the final
passing matrix. In particular `iteration-*`, `native-final`, `native-final-2`,
`native-final-3`, and earlier `verification-*` are not final source bindings.
The initial evidence wrapper mistakenly assumed a presented confirmation that
the actual protocol does not provide; the final wrapper/docs correctly enforce
and disclose the pre-present contract.

## Preservation and scope

The audit compares **all 3,073 historical tracked evidence files / 57,991,081
bytes** to their exact blobs at `132258c`; all match, including the earlier
535-file component evidence collection, blocker reproductions and preserved
unaccepted experiment. No historical evidence was rewritten.

Both owner reproduction directories remain **untracked and untouched**, not
silently absorbed into this candidate:

* `evidence/data-components-next/owner-editor-gap-check/`
* `evidence/data-components-next/owner-editor-gap-check-retry/`

Their **8 files** are bound by `preserved-owner-snapshot.json`; this is honestly
an end-of-implementation preservation snapshot, not a claimed start-of-turn one.
The auditor verifies them again after sealing. A final dirty status consisting
only of these pre-existing owner directories is intentional; no clean-worktree
claim hides them.

No broad cleanup was performed. Compatible cache was reused for bounded builds.
One shell block sourced the default environment without explicitly retaining the
shared-cache override and regenerated local build outputs; this was corrected,
and that regenerable directory was not broadly cleaned. Disk stayed sufficient
for authorized work; post-seal observed free space was **4.5 GiB** (not a benchmark).
Blooey and remote targets are untouched. No credentials, paid service, remote
push, publication, deployment, approval or integration was used.

## Deliberate limits / combined-review attention

* Table is **bounded, not virtualized**: 256 rows × 32 columns; host pagination
  required. Single/no selection and passive cells only. No spreadsheet editing,
  multi-selection, pinned/grouped columns or automatic large-data engine.
* Column resizing is declarative and demonstrated interactively; **no pointer
  drag handle or autosizing**. Long fixed-width content is clipped, without an
  ellipsis/tooltip promise; narrower viewports require scrolling.
* Inspector bounds: 128 properties / 32 groups / 64 choices; deterministic typed
  drafts, not a schema/reflection/persistence or application-wide undo system.
  Custom editor controls remain explicitly application-composed.
* The accessibility dispatcher still deliberately returns **UnsupportedSetValue**
  for canonical editors. Safe focus/activation/projection is tested, not live AT.
* Real IME, physical input/clipboard, accessibility and Windows/Linux remain
  hands-on validation gaps. Native images are pre-present, as detailed above.
* New semantics are opt-in; legacy unopted behavior is not silently replaced.
  Independent sender review should explicitly assess this compatibility policy.

The sender should independently review the **whole candidate once** (canonical
fixes + both components + gallery + evidence), then decide follow-up changes and
native integration. This worker has not made that decision.

## Reproduce

From this checkout, with an explicitly selected compatible `CARGO_TARGET_DIR`:

```sh
source scripts/data-gallery-env.sh
bash scripts/verify-data-editors.sh /tmp/rndr-data-editors-fresh-verification
bash scripts/capture-data-editors.sh /tmp/rndr-data-editors-fresh-native
python3 evidence/data-components-next/combined-candidate/audit.py
cargo run --offline --locked -p rndr --example data_editors_gallery --features observe
```

Use fresh output directories for the first two commands. The auditor checks the
saved final artifacts without rebuilding; its exact cached binary check assumes
the recorded build cache/executable still exists. Rebuilding for another checkout
must create fresh capture bindings rather than pretending to reproduce the old
binary hash.
