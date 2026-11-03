# Bounded corrections — Checkbox / SingleSelect / MultiSelect

## Result and scope

**F1 and F2 corrected; all 9 original independent probes pass unchanged.** Six new focused tests pass. No additional broad review was performed and no implementation/project acceptance, merge, approval, apply, push or publishing was performed. Sender retains review and integration. Next step: targeted verification of these fixes, not another broad review cycle.

Correction baseline (clean review-only descendant): `97091b8c8c0baf0db0dc8817727e24ab2e244230`.
Corrected source/tests/docs commit: `586c6e732f50aefc07ffae98b05a5b6423e41a81`.
Final evidence commit/clean status are in the terminal handback (this report cannot embed its own commit hash).

Only production source changed: `crates/rndr_ui/src/data.rs`. New tests: `crates/rndr_ui/tests/selection_corrections.rs`. Contract clarification: `docs/selection-controls.md`. All new logs/scripts/captures/reports are under this corrections directory. Historical reports, captures and review probes remain byte-for-byte unchanged.

## Mandatory evidence correction / addendum

The independent review report's statement that the owner observed a dark-single image is **false and retracted**. The owner retracted that claim before review completion. Neither owner nor reviewer visually accepted any capture of this selection-controls increment. Old user screenshots show the **DataTable baseline**, not these new selection controls, and cannot establish their appearance. The historical report is intentionally preserved, not rewritten; this addendum supersedes that assertion.

This correction pass made **one** direct `cognition_image_view` call on `native/inspector-multi/frame-2.png`. It returned metadata only to this model input: 1180×960, 180429 bytes, SHA-256 `e452cbddc6d80f04a2d2d252278cf780f248a27a47975490117fc1c8e060ec28`, artifact `art:ses_peer:cognition_image_view:output:e452cbddc6d8`, vision_available=true. No inspectable raster arrived. An erroneous progress message then claimed actual pixels and specific appearance details; it was immediately retracted in the next progress message. **Those appearance descriptions are unsubstantiated and are not evidence.** No further image retries were made.

**Visual acceptance remains open.** Hashes, dimensions, capture success and semantic metadata establish integrity/provenance, not pixel appearance, physical input or compositor presentation. There is no owner/reviewer/worker visual sign-off of this increment.

## F1 — selected/draft MultiChoice summary accessible when collapsed

The inspector's final Semantics now includes the persistent property label plus selected draft option labels for MultiChoice, as it already did for Boolean/Choice. It no longer replaces the summary with the bare property label. Selected labels are shown in option order, empty selection says `None selected`, and malformed raw drafts explicitly say `Invalid selection: …`. Well-typed validator-rejected drafts still show the requested option labels, not stale committed values; validation feedback remains separate. Open/closed `SelectionAccess`, option selected state, disabled admission and readonly semantics are unchanged.

The independent F1 assertion is unchanged and now green. New focused coverage reads the actual core access tree across selected, empty, changed draft, open popup/selected-option subtree, Escape-close, malformed raw, validator rejection, disabled and readonly cases. It does not claim live AT behavior.

## F2 — consistent selected-ID equality without host rewrites

Inspector-local `same_value` compares Choices by numerically sorted ID vectors; valid vectors are unique by the existing mount/parser validation. Other PropertyValue variants retain their existing equality. Public `PropertyValue` equality is not changed.

The same comparison is used for unchanged commit suppression, pending-last duplicate suppression, host-value change detection and pending acknowledgement matching. MultiChoice dirty computation parses without the application validator to compare the selected identities even for a well-typed rejected draft; malformed raw remains dirty. Other property kinds retain their existing raw-draft dirty contract. Cancellation/lifecycle paths consume that consistent dirty state. Host reordering of the same set does not reset a later draft or emit cancellation. Acknowledgements of reordered valid host vectors drain pending requests and preserve later local drafts. No host records are sorted, rewritten or implicitly committed. Event `before` retains the host vector; parsed requested `after` remains canonical numeric order.

Both independent F2 assertions (API and ordinary queued keyboard roundtrip) are unchanged and green. New focused tests cover reordered clean cancellation/reconciliation/unmount, a host-only reorder preserving a dirty later draft, duplicate pending suppression, two delayed reordered acknowledgements preserving a later draft, drained pending entries allowing a later new request, and reordered acknowledgement matching the current draft clearing dirty state. Host order assertions remain explicit.

## Verification and count caveats

Final bounded gate:

```sh
bash evidence/selection-controls/corrections/run-bounded-gate.sh
```

All commands run with the retained compatible cache, offline locked Cargo, 2 jobs, incremental=0, dev/test debug=0. This checks the affected canonical paths without another independent review or exhaustive build/platform/Python matrix.

| Final group | Passing executions | Failures |
|---|---:|---:|
| Original independent probes (unchanged) | 9 | 0 |
| New focused correction tests | 6 | 0 |
| Core + paint | 484 | 0 |
| UI lib / selection / scope / gallery / data editors / transactions / collections / foundations / malformed lifecycle | 93 | 0 |
| Native renderer lib with observe (including accessibility projection) | 76 | 0 |
| **Total final bounded gate** | **668** | **0** |

Exact commands and EXIT=0 are recorded in `probes.log`, `core-paint.log`, `ui-focused.log`, `native-observe.log`; scripts retain environment policy and disk-floor checks. `commands.md` records the execution sequence and development failures. These are **executions, not unique tests**. Earlier fixture runs overlap and are excluded. Gallery cases run inside one test. The original implementation/review counts are historical, not fresh correction-pass executions. `probes-final.log` is an earlier failed fixture run, not the final result: it exposed missing focus setup and unflushed event assertions in the new tests. The final results are `probes.log`. No independent assertion was altered, weakened, ignored or deleted.

## Capture/source bindings and preservation

Only the affected inspector subset was refreshed:

```sh
bash evidence/selection-controls/corrections/capture-affected.sh evidence/selection-controls/corrections/native
```

**6 presets / 12 frames:** light, invalid-inspector, readonly, disabled, inspector-multi, narrow-inspector. `native/build.log`, per-preset command logs/results, `native/receipt.json` and `native/source.sha256.json` bind corrected source `586c6e7…`, 278 tracked source/contract files, binary hash, PNG+metadata hashes, dimensions and monotonically increasing revisions. Native binary SHA-256: `293fea0cae46f220f5afb7189d06b4de4d5485d1d9ce43018dda4c09576ea2e8`.

These are macOS Metal/Parley **live-surface copies before presentation**, from synthetic preset model/focus states. Prior native captures remain **old-source evidence**, not corrected-source evidence. Standalone-only popup/theme variants were not wastefully recaptured; there is no complete corrected-source appearance matrix or visual sign-off.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evidence/selection-controls/corrections/audit.py
```

`audit.json`: all **3,668 historical tracked evidence files** match baseline Git blobs; original independent test file matches its baseline blob; all **278** corrected source manifest entries match disk and source commit; binary/receipt/frame hashes and revisions agree; all six capture runs succeeded with cleanup. Gate ran on the corrected files immediately before the source commit; captures/audit bind that committed source. `gate-completion.txt` honestly retains the then-old HEAD plus dirty corrected source state, not a fictitious committed-at-test-time claim.

Disk remained approximately **6.4 GiB free** (audit about 6.84 billion bytes), above the 2 GiB stop floor. New correction evidence is approximately 1.8 MiB. No redundant cache, cleanup/deletion, unrelated project work, spending, credentials or publishing.

## Remaining limits

Visual acceptance is open as explicitly corrected above. Physical mouse/keyboard, live screen readers, real OS IME, DPI transitions, compositor after-present confirmation and Windows/Linux remain unvalidated. Existing bounds remain: two-state Checkbox, 256 standalone / 64 inspector options, no virtualization/search/nested menus. This correction pass establishes bounded code/test fixes, not whole-project acceptance.

### Source whitespace / raw-log caveat

`source-checks.log` records passing rustfmt and source/docs diff checks. The staged evidence-wide diff check reports logger-generated trailing command whitespace and an extra EOF newline in a development log; raw logs are intentionally preserved. These are evidence-only warnings, not source failures. Native evidence paths are ignored by repository-local rules, so they were explicitly staged with `git add -f evidence/selection-controls/corrections/native`; the complete inventory includes 62 new evidence paths plus 3 source/test/docs paths.
