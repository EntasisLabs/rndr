# SINGLE combined independent re-review — code findings resolved; visual gate blocked

## Exact scope and recommendation

Complete candidate: **46dcee2e9945027b554252832e93c1d8c9a546f1**.
Focused source: **d9d9c3c998e67c8da332167cc7f179e4d373508b**.
Prior reviewed candidate: `39b049feb32b50a7000bcb60ead62e16ee4fe8e2`.
Accepted product baseline: `57c30a3242efcea7d54a6083251575ac04f1c603`.
Undertaking: `work-8edd6bffbd5f264c66f0ffd4aef16464`.

**Recommendation: F1–F4 are resolved in source and independent behavioral replay.
No new blocking code defect established. Full combined acceptance/integration
remains withheld because actual independent pixel inspection is still blocked.**
This is one combined review of the opt-in editor repair, DataTable,
PropertyInspector and connected gallery—not independent acceptance of a subset.
Owner retains sender review and native integration responsibility.

HEAD and tracked cleanliness were independently checked before and after testing.
There is no source difference between the focused source commit and complete
candidate. The correction changes only seven source/test/doc files, with no
new DataTable implementation changes relative to the previously reviewed candidate.
The previous combined report and the correction report, audit.py,
audit-receipt.json and visual-status.json were read. Prior API/architecture
assessment stands with the corrections below; no blanket legacy migration is
implied. No source fixes, commit, merge, approval, apply, push or Blooey operation
was performed. Only new evidence and temporary reviewer test targets were written;
all temporary targets created by these runners have been removed.

## Findings and disposition

### F1 — prior High / P1: RESOLVED

`crates/rndr_ui/src/data.rs` PropertyState/reconcile/set_draft/commit now retain
outstanding requested values across later edits. A value-only reconciliation
matching an outstanding request drains that request and older requests, advances
the committed baseline and preserves the later draft/editor state. Different
value replacements and contract changes still reset draft and typing history.

Original end-to-end reproduction passes unchanged in intent: gallery title
`Brand refresh`, queued `1, Enter, 2` commits host value `Brand refresh1` and
retains draft `Brand refresh12`. The maintained added control also checks exact
caret/anchor preservation, undo returning to `A1`, and unrelated external `B`
replacement clearing history. Multiple outstanding `A1` then `A12` requests,
later invalid draft, and subsequent readonly contract replacement pass.
Inspector does not mutate host records; the connected gallery consumes requests
and reconciles its application-owned model in Update.

### F2 — prior Medium / P2: RESOLVED

`crates/rndr_core/src/field.rs` adds opt-in FocusBoundary on the existing
canonical focus transition path; `data.rs` installs it on each property root.
Departure walks old/new ancestry and notifies roots left, before the next queued
input. Window blur supplies next=None even when logical focus stays on a button.

Both original Boolean and Commit-control window-blur reproductions pass.
Maintained choice/custom descendant blur and departure controls pass, alongside
text-focused blur and legitimate same-property navigation. Custom child ->
Commit -> custom child retains its draft; leaving or window blur cancels without
host commit. Existing GenericInteraction owns focus/activation and blur input
suppression; this is not a second dispatcher or a field-only workaround.

### F3 — prior Medium / P2: RESOLVED

The original `x, Tab, Tab, Tab, Shift-Tab, Shift-Tab, Enter` reproduction passes:
actual property departure cancels before returning to Commit, so stale `Ax`
is not submitted. Same-property editor/Commit/Cancel navigation is retained.
PostUpdate containment remains a fallback, not the queued chronology mechanism.
Historical six collection reproductions, two activation-policy controls and
12 maintained collection chronology regressions also pass in the fresh full gate.

### F4 — prior Medium / P2: RESOLVED

`transaction_key` copies `Input.ime_enabled` with preedit/commit into the
canonical `ime::text_frame` per-key filter. The original public Input test with
ime_enabled=true, ime_commit=`日`, keys=[`n`] now yields exactly `日`.
This proves the public Input snapshot seam, **not physical OS IME behavior**.
Existing IME confirmation/submission controls remain passing.

### I1 — informational host protocol limitation, not a new blocking defect

Acknowledgement identity is a **value match**, not an opaque request/revision ID.
A host replacement equal to an outstanding requested value cannot be distinguished
from acknowledgement. Reproduction in new `reviewer_ack_contract.rs`: request
`A1`, edit draft to `later`, reconcile host `A1`; draft stays `later`. Explicit
`cancel` then restores host `A1`. Hosts requiring forced replacement with the
same matching value must explicitly cancel/reset rather than expect inference.

A second new probe requests `A1` then `A12`, reconciles latest `A12` (preserving
`later`), then supplies retired `A1`; that later value is treated as external
replacement and resets draft to `A1`. Hosts own request ordering, stale-response
rejection and model validation; the inspector is not an asynchronous transaction
protocol or compare-and-swap engine. Before values are snapshots of host state
when each request is issued, not an invented serialization guarantee.
These limits fit the documented matching-outstanding-value contract. No opaque
identity, rejection callback or automatic conflict merge is claimed. Outstanding
requests require host handling or cancellation; the new Vec is not a bounded
remote transaction queue.

## Regression promotion and added controls

All five original failing assertions are promoted to the existing three suites
with identical event order and expectations. `promotion-check.json` compares
function bodies after whitespace/rustfmt trailing-comma normalization; assertions
and literals were also inspected manually. Original fixtures/logs were not edited.
Four additional maintained controls cover caret/history plus external replacement,
multiple delayed acknowledgements plus invalid drafts/contract change, choice
blur/departure, and custom-descendant within-property navigation/blur/departure.

FieldTransactions remains opt-in; legacy unopted fields remain unchanged.
Historical legacy failures are retained as prior evidence and were not rerun or
relabelled passing here. No Blooey adoption or blanket editor repair is certified.
Table limits remain 256 rows x 32 columns, passive cells, single/no selection,
declarative widths, no drag resize/spreadsheet editing/virtualization. Inspector
limits and host ownership remain documented, including no accessible SetValue,
persistence or application undo implementation. No new DataTable-specific blocker
was established during this re-review.

## Independently executed commands and results

All run from the isolated checkout with:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
bash evidence/data-components-next/independent-combined-review/reproduce.sh evidence/data-components-next/independent-rereview-corrections/focused
bash scripts/verify-data-editors.sh evidence/data-components-next/independent-rereview-corrections/verification
python3 evidence/data-components-next/review-corrections/audit.py
```

Each top-level command exits **0**. Audit was rerun after all testing and source
binding checks. Logs below are NEW, not reused worker results. Offline/locked,
two jobs, incremental/debug disabled by existing env, compatible cache reused.
No dependency refresh, cleanup or extra capture run. Disk: ~4.5 GiB at start,
~4.7 GiB at final observation.

Original focused replay: **16 data + 10 field + 6 gallery = 32 passing executions**,
all exits 0. Full bounded gate: **717 passing executions across 10 commands**,
all exits 0. Maintained focused suites are **20+6+10=36**, contained within the
608 core/paint/UI executions, not extra unique coverage. New acknowledgement
probe command runs 5 passing executions (2 new probes plus 3 promoted controls),
exit 0. These totals overlap and must not be summed as unique tests.

| New log | Exact command | Exit | Passing executions |
|---|---|---:|---:|
| `focused/data.log` | `cargo test --offline --locked -p rndr_ui --test reviewer_data -- --nocapture` | 0 | 16 |
| `focused/field.log` | `cargo test --offline --locked -p rndr_ui --test reviewer_field -- --nocapture` | 0 | 10 |
| `focused/gallery.log` | `cargo test --offline --locked -p rndr_ui --test reviewer_gallery -- --nocapture` | 0 | 6 |
| `verification/core-paint-ui.log` | `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui` | 0 | 608 |
| `verification/facade-fallback.log` | `cargo test --offline --locked -p rndr --lib --no-default-features` | 0 | 1 |
| `verification/facade.log` | `cargo test --offline --locked -p rndr --lib --features observe` | 0 | 1 |
| `verification/fallback-renderer.log` | `cargo test --offline --locked -p rndr_render --lib --no-default-features` | 0 | 25 |
| `verification/formatting.log` | `rustfmt --edition 2021 --config skip_children=true --check crates/rndr_core/src/access.rs crates/rndr_core/src/field.rs crates/rndr_core/src/interaction.rs crates/rndr_core/src/lib.rs crates/rndr_render/src/a11y.rs crates/rndr_ui/src/collections.rs crates/rndr_ui/src/controls.rs crates/rndr_ui/src/data.rs crates/rndr_ui/src/lib.rs crates/rndr_ui/tests/data_editors.rs crates/rndr_ui/tests/data_editors_gallery.rs crates/rndr_ui/tests/field_transactions.rs crates/rndr_ui/examples/support/data_editors_gallery.rs crates/rndr/examples/data_editors_gallery.rs` | 0 | 0 |
| `verification/gallery-build.log` | `cargo build --offline --locked -p rndr --example data_editors_gallery --features observe` | 0 | 0 |
| `verification/gallery-fallback.log` | `cargo check --offline --locked -p rndr --example data_editors_gallery --no-default-features` | 0 | 0 |
| `verification/historical-six-and-activation.log` | `cargo test --offline --locked -p rndr_ui --test combined_original_six --test combined_activation_policy` | 0 | 8 |
| `verification/native-observe.log` | `cargo test --offline --locked -p rndr_render --lib --features observe` | 0 | 74 |
| `verification/whitespace.log` | `git diff --check` | 0 | 0 |

Additional command:
`cargo test --offline --locked -p rndr_ui --test reviewer_ack_contract reviewer_ -- --nocapture`
exit **0**, 5 passes (`ack-contract-retry.log`). Fixture is preserved in this
directory and was copied to a fresh temporary target only for execution.

Reviewer harness mishap, not a product failure: the first promotion comparator
incorrectly considered rustfmt trailing commas a semantic difference and aborted
before creating the acknowledgement fixture. The following shell lacked errexit
and attempted the missing target, producing exit 101 in `ack-contract.log`.
That log is retained, not overwritten. After accounting for formatting and
manually verifying unchanged intent, the retry used fail-fast setup and the
actual fixture passed. A source search also encountered unavailable `rg`; its
relevant search was repeated with grep. Neither is counted as candidate proof.

`results.json` records exact independent log commands/exits/counts/SHA256s.
The full formatting command is recorded there and in verification/formatting.log.

## Provenance and preservation: independently verified

Initial independent audit output equals the saved worker audit receipt exactly.
Final audit passes again. **3,396 preexisting files / 66,869,705 bytes** are
unchanged, including **3,354 tracked files / 66,628,094 bytes** checked against
baseline Git blobs and **42 untracked owner/reviewer files**. All **112 correction
bundle files** validate against bundle.sha256.json. None of the previous reports,
fixtures, retry logs, owner directories or captures was deleted or rewritten.

All **131 native source manifest entries** match current bytes AND Git source
commit d9d9c3c. All **24 PNG/metadata pairs across 12 states** validate, native
runs exit 0. Native binary SHA256:
`7957800360fefe9d0b37d2b142ddb1b96cde6146b50004712817a23fc5546aee`.
Native receipt SHA256:
`b63b5c9f4df557ebf2e7c602819be667e0c425c772366d225da8d01a97449049`.
No source changes after focused source commit. The worker gate's earlier HEAD
record does not replace exact source-byte validation; this independent full gate
records complete candidate HEAD directly.

Evidence: `audit.json`, `final-audit.json`, `integrity-summary.json`, plus the
unchanged correction manifests/receipts. Existing three untracked review/owner
directories remain; this review adds only independent-rereview-corrections/.
Tracked worktree remains clean at exact complete candidate.

## Independent visual sign-off: BLOCKED

Direct cognition_image_view calls (not a parallel wrapper) were attempted on
refreshed frame-2 PNGs for **all 12 states**: light, dark, narrow,
narrow-inspector, compact-hc-200, compact-hc-200-inspector,
compact-hc-200-bottom, invalid, readonly, disabled, empty, workflow-bottom.
Repeated retries still returned metadata with ok=true and vision_available=true,
without inspectable pixels in this reviewer context. **Actual independent pixel
coverage is zero.** No visual correctness, clipping, contrast or legibility
claim can follow from these receipts, geometry or provenance. No new visual
defect is alleged without pixel evidence. The visual gate remains OPEN.
`visual-review.json` names the attempted files and precise coverage/limitation.

Captures are native Metal/Parley **pre-present GPU surface copies**, with
presented_confirmation=false, not compositor/presentation proof. Even working
pixel inspection of these captures would not prove compositor output or physical
input. Real OS IME, live AT, physical DPI and Windows/Linux remain unvalidated.

## Owner next action

No further F1–F4 code correction is requested on this evidence. Obtain actual
independent inspectable pixels for this exact bound capture matrix, then complete
the full combined review decision. Keep acceptance/integration withheld until
that remaining gate is genuinely resolved; this report does not approve/apply
anything and does not transfer the owner's native lifecycle responsibility.
