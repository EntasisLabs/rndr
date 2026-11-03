# Combined independent review — Checkbox / SingleSelect / MultiSelect

## Verdict

**Changes requested before implementation acceptance/integration. Two confirmed P2 defects, reproduced by three failing assertions.** No P0/P1 defect established. Existing bounded regression gates pass; they do not cover these failures. This is one combined independent review of the increment, not approval of the implementation or whole project.

Reviewed candidate: `9146cbdccbcb78daa36510f9a975364cf7bea052`.
Baseline: `c9536acba3914a3be39c53c6bffe42a9b9c40c42`.
Implementation/capture source: `19f553c0d57339ee666d9752dedca861331209f1`.
Review-only descendant commit and final clean status are supplied in the terminal handback (this report cannot embed its own commit hash).

Actual baseline→candidate changes were inspected, including the new selection module, canonical dispatch/access policy, popup/layout/focus ownership, inspector adapters, AccessKit/observation projection, gallery, tests, scripts and documentation. `docs/selection-controls.md`, implementation `REPORT.md`, successful gate logs/counts, capture receipt/source manifest and integrity audit were read rather than treating the implementation summary as acceptance evidence.

No production source, implementation tests, documentation or prior evidence was corrected or rewritten. The independent test file is deliberately separate: `crates/rndr_ui/tests/selection_independent_review.rs`. It is **expected to fail on this candidate**; do not ignore it to claim the review descendant has a green full suite. Source corrections require the owner-arranged implementation pass.

## Findings

### F1 — P2: collapsed inspector MultiChoice loses its selected value in actual accessibility projection

**Location:** `crates/rndr_ui/src/data.rs:1184–1190`, specifically the Boolean/Choice-only condition at **1186**. Related path: selector summary initially generated at `crates/rndr_ui/src/selection.rs:608–638`; `crates/rndr_core/src/access.rs:394–401` gives non-text controls an empty `value`.

**Reproducer:** mount an inspector MultiChoice labelled `Channels`, available IDs 1=`One`, 2=`Two`, committed value `Choices([2])`; frame it and inspect `access_tree` for the collapsed editor. Independent test: `multi_inspector_accessible_name_includes_current_value`.

**Expected:** the actual collapsed ComboBox projection exposes both persistent label and current selection through a name/value/state representation usable without opening the popup, as the standalone selector summary does.

**Actual:** `AccessNode { role: ComboBox, name: "Channels", value: "", children: [], expanded: Some(false), ... }`. There is no selected option subtree while collapsed. The inspector overwrites the selector's summary-bearing Semantics and omits MultiChoice from the branch that includes `shown`. This is a concrete core projection failure, not an inference about a physical screen reader. Existing AccessKit projection tests pass but cannot recover information removed upstream.

**Impact / action:** assistive-technology consumers cannot read the current multi-selection from the collapsed inspector trigger. Include MultiChoice's current draft summary in the exposed name/value or equivalent truthful projection; retain the persistent label and selected-option semantics. Recheck draft changes, empty/invalid selection, disabled/readonly, and closed/open states. No live AT behavior is claimed by this review.

### F2 — P2: accepted noncanonical multi-value ordering produces an unchanged-set CommitRequested

**Locations:** `crates/rndr_ui/src/data.rs:921–923` sorts parsed IDs, but **1731–1735** compares the parsed result to the unnormalized host `PropertyValue` using ordered Vec equality. Related dirty comparison: **1704**; reconciliation/ack matching: **1405–1413**. `valid_inspector` at **995–1003** accepts unique available IDs without requiring sorted order.

**Reproducer A (API contract):** mount `MultiChoice([One,Two])` with valid host value `Choices([2,1])`. Without editing, call `InspectorHandle::commit`. Independent test: `noncanonical_host_multi_value_does_not_request_spurious_commit`.

**Reproducer B (ordinary queued keyboard path):** with that same value, focus the editor; queue Enter (open), Enter (toggle first option off), Enter (toggle it back on), Escape (close), Tab (Commit), Enter (request commit). Independent test: `keyboard_roundtrip_same_set_does_not_request_spurious_commit`.

**Expected:** no commit request for an unchanged selected set. API commit returns false for unchanged values, as documented. Numeric canonicalization must be consistent across accepted host values, draft equality, dirty state and pending acknowledgements, without silently rewriting host records.

**Actual:** API commit returns true; the keyboard sequence emits:

```text
CommitRequested { before: Choices([2, 1]), after: Choices([1, 2]) }
```

The selected identities are unchanged. Mount accepts `[2,1]`, parsed/base drafts sort it, and literal Vec/raw comparisons manufacture a change. This is not an out-of-contract malformed fixture: no sorted-order precondition is enforced, and the public docs say IDs canonicalize numerically. It is also not a silent records mutation: the framework emits an unnecessary explicit host mutation request; a host may attach persistence, audit, network or undo effects to it.

**Action:** use a consistent semantic/canonical representation for validation, dirty/unchanged checks and acknowledgements while preserving application record ownership. Retain both independent reproductions and add delayed-ack/reorder coverage. The noted acknowledgement comparison is an affected follow-up path, not a separately demonstrated third failure.

## Independent execution evidence

Environment: retained compatible `CARGO_TARGET_DIR` specified in the request; `scripts/data-gallery-env.sh`; offline locked Cargo; two jobs; incremental=0, dev/test debug=0. Disk remained approximately **6.4 GiB free**, well above the 2 GiB stop floor. No target cache creation, cleanup or unrelated project work.

Reproduce the fresh bounded gate:

```sh
bash evidence/selection-controls/independent-review/run-bounded-gate.sh
```

The script records exact commands/environment policy and executes:

| Fresh independent group | Passed executions | Failed |
|---|---:|---:|
| `cargo test --offline --locked -p rndr_core -p rndr_paint` | 484 | 0 |
| UI lib + selection/scope/gallery, data editors/gallery, field transactions, collections/chronology, foundations, malformed lifecycle | 93 | 0 |
| `cargo test --offline --locked -p rndr_render --lib --features observe` | 76 | 0 |
| **Bounded existing gate total** | **653** | **0** |

Logs: `core-paint.log`, `ui-focused.log`, `native-observe.log`; all exit 0. This deliberately rechecks the affected canonical behavior without repeating every facade/fallback/build/Python gate. The original stored successful logs independently sum to the reported **738 Rust executions** (627+8+76+25+1+1), with **20 Python executions** reported separately. Those are implementation-run counts, not fresh independent executions.

Independent probes:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
cargo test --offline --locked -p rndr_ui --test selection_independent_review
# Expected candidate result: exit 101; 6 passed, 3 failed (two defects).
```

Final fixture log: **`probes-final-inherited.log`**. Earlier development probe runs are retained as `probes.log` and `probes-final.log`; do not add their totals to the final run. The final six passing independent assertions cover: initial last-selected row reveal; End navigation reveal; IME-enabled Space release suppression; inherited readonly invalidation plus outside matching-release swallowing with an independently enabled background checkbox; popup-focused inspector blur cancelling a dirty draft without commit; removed armed option not retargeting its Space release. Three failures establish F1/F2 as above.

Counts are executions, **not unique tests**. Existing gates, implementation runs, earlier probe versions and final probes overlap. Gallery matrix testing includes 24 cases inside one test. No physical input, live accessibility, real OS IME or cross-platform validation is inferred from event snapshots.

## Core/contract assessment and limits

No additional blocking defect was established in the inspected canonical paths: one active anchored popup; viewport-constrained placement after ordinary roots; Scroll-backed options; selected vs focused/highlighted state; queued activation via canonical handlers; stable option identities/removal; explicit authoritative remount boundaries; Escape/Tab/outside behavior; inherited disabled/readonly/modal admission; logical FocusOwner linking to property focus boundaries; policy/window blur/teardown cleanup. Existing tests plus independent edge probes support these statements, not exhaustive correctness.

Standalone controls intentionally keep acknowledged visual state until host reconciliation; `requested` provides queued chronology. Inspector ChangeHandler updates only drafts synchronously, and CommitRequested remains a host decision. The reviewed gallery keeps standalone state separate from inspector committed records; its model writes occur in the explicit host event handler. No silent host-record mutation was found. F2 is an unnecessary explicit request, not demonstrated data loss.

The gallery's standalone and inspector composition, source-defined preset setup and 24-case geometry/semantics matrix were inspected/tested. This does not establish rendering appearance or physical interaction acceptance.

## Capture integrity and visual delivery

Fresh read-only audit command:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 evidence/selection-controls/independent-review/audit.py
```

`audit.json`: all **3,534 baseline evidence files unchanged**; all **3,656 candidate evidence files** still match their candidate Git blobs; all **276 source-manifest files** match disk and source commit; receipt source/manifest/current binary hashes agree; all **12 presets / 24 PNG+metadata pairs** match receipt hashes, revisions and `live-surface-copy-before-present` provenance. Result receipts report capture success. These checks prove bindings/preservation, not appearance or presentation.

Five direct `cognition_image_view` attempts were made, one each on `frame-2.png` in `narrow-inspector`, `compact-hc-200`, `many-popup`, `empty-popup`, and `multi`. Calls returned ok/vision_available plus hashes, dimensions and artifact IDs, but **no inspectable raster reached this reviewer model input**. No visual observations or all-preset sign-off are asserted; no endless retries were made. The owner's independently observed dark-single image is acknowledged as owner evidence, not attributed to this reviewer and not extrapolated to other presets.

Visual inspection remains unresolved for this review. Physical mouse/keyboard, real IME/DPI transitions, compositor after-present confirmation, live screen readers and Windows/Linux also remain open. The 256 standalone/64 inspector retained-option bounds, no virtualization/search/nested menus, and two-state Checkbox limits remain explicit.

## Handback

Recommend an owner-arranged correction pass for **F1 and F2**, retaining these tests, followed by focused verification of corrections and actual pixel inspection through a working delivery path. Do not repeat the whole review merely to obtain another passing happy-path gate. No merge, approval, application, push or whole-project acceptance was performed. Sender retains implementation review and integration responsibility.
