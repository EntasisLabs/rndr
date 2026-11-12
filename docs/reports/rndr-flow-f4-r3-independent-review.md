# Independent exact-candidate F4 review after R3 — PASS

Date: 2026-11-12. Same authorized F4 undertaking; independent review only.

- **Reviewed exact candidate:** `2abc7d32c9029587ac74bea599428545efa8a1e6`.
- **Immediate production parent:** `896ed5e4c294caa83550257226541cda245825cf`.
- **Production parent's parent / preserved R3 FAIL report:**
  `5b648bb723263064f1096bfc2aeeb31f14813d4f`.
- **Accepted F3 / unchanged `main`:** `f16e5572d35450d586dd4f10e00591fa36fb2b04`.

**Verdict: PASS for this bounded exact-candidate F4 review. R3 is resolved;
R1/R2 remain resolved. No new actionable correctness blocker was reproduced.**
This is an independent recommendation for sender assessment, not F4/project
acceptance, native approval, integration or migration readiness. The review
commit adds only this report and NEW independent evidence. Its HEAD is returned
in the terminal handback; it is an evidence descendant, not the qualified
candidate above. The candidate itself was already a report/evidence-only child
of its production parent, with identical production/test bytes.

Historical FAILs remain valid for their exact old candidates. Both original
independent reports, saved sources/logs, original worker evidence, correction
reports and evidence are unchanged. An explicit Git-blob/complete tracked-path
comparison verifies this, not merely a clean production diff.

## Independent method and R3 decision

I read F0, the epic, F4 readiness (including its additive R3 update), both
historical independent FAIL reports/probes, the scoped R3 correction report,
its exact production diff, new regression sources, gates and Python auditors.
I traced the shared wrapper, task reservation/publication, core cancellation,
producer exit, terminal precedence, timer and completion receipt paths. Worker
green results were comparison data, not my verdict: all reported qualification
below was executed freshly in the independent evidence destination.

The **sole production difference** from the R3 FAIL report's tree is five added
lines in `crates/rndr_flow/src/work.rs:174–178`: four comment lines and

```rust
let _ = catch_unwind(AssertUnwindSafe(|| drop(value)));
```

This runs only in the existing aggregate-`Panicked` branch, AFTER the user
future's destruction has returned or unwound under its own catch. It moves the
suppressed `Result<T, TaskFailure>` into exactly one explicit destruction
boundary, then returns `Poll::Ready(Err(TaskFailure::Panicked))`. No implicit
second destruction remains. A second **sequential**, non-overlapping unwind
from successful output destruction is contained; it is not dismissed as
panic-during-active-unwind. The normal successful/explicit-error return branch
is unchanged. Caller-owned successful payload destruction is not newly caught
or promised by this fix.

The correction does not change terminal precedence, acknowledgement timing,
receipt reservation, slot sweep conditions, logical publication/admission,
executor selection or any core/UI/facade implementation. The readiness edit
adds R3 provenance/coverage; it does not remove or weaken the prior contract.

### Fresh historical reproductions and promoted coverage

The gate runner copies both saved reviewer suites unchanged, verifies byte
identity, executes them, then removes the temporary copies:

- Original R1/R2 suite: **3 passed / 0 failed**.
- R3 suite: **3 passed / 0 failed**, including the real channel-blocked producer
  returning a nontrivial success after a fully caught queue-cleanup panic,
  panicking/nonpanicking suppressed-output controls, and actual native-identity
  timer destruction control. Both output destructors assert no active unwind.

The permanent branch tests assert results and actual envelopes, not merely
absence of an outer-core panic. If `Tracked` escaped and core caught it without
publishing the promised failure completion, the task assertions would fail.
Freshly executed: **56 distinct coordinates: 11 tracked + 45 task**. These cover
successful/nontrivial output, explicit Failed/Panicked, poll panic, pending drop,
future-drop panic and sequential suppressed-output panic across local,
background, retained custom Job and panic-startup paths. Separate Rc-local
coverage exercises the non-Send contract. Promoted channel and native-identity
controls also pass. All expected coordinates and exactly-once/receipt/actual
exit assertions are audited independently.

### New independent paused-destructor challenge

Saved source: [independent_r3.rs](rndr-flow-f4-r3-independent-evidence/independent_r3.rs).
It uses public APIs, rendezvous channels and a retained real core `Job`; no
private status injection, sleeps, invented producer exit or backend cancellation.
Six coordinates combine normal versus panicking executor startup, normal versus
panicking future destruction where suppression is applicable, and both output
panic/control variants. **All six pass twice** (one test containing six explicit
coordinates per run).

1. While the actual user future destructor is paused, assert exit is **not**
   acknowledged and the pre-poll receipt is resident. Drop the owning scope and
   repeatedly pump one unit: observer cancellation/retirement still does not
   acknowledge that blocked future or reclaim its one-slot charge.
2. Release that destructor. While the suppressed output destructor is paused,
   assert no active unwind, exactly one future destruction, aggregate Panicked,
   and user-future exit now acknowledged. **The output destructor is still
   running**: this is precisely user-future exit, NOT a wrapper/output/backend
   join. The reserved receipt remains resident, and a new request is refused
   with `Error::Capacity` despite repeated one-unit pumps.
3. Release output destruction, with or without its separate panic. The actual
   Job finishes; both destructors have executed exactly once. Normal-startup
   cancellation suppresses publication and releases its reservation. A retained
   panic-startup Job has no returned cancellation handle: it publishes the
   promised `Err(Panicked)` while the request is ineligible. That is not a valid
   native write.
4. Clone the published completion, transpose a clone, and reject a stale commit
   with **zero writer calls**. After event buffers drain, the last caller-owned
   clone still retains the receipt and slot; only its release permits complete
   reclamation. Final `WorkCounts == default` in every coordinate.

Final logs: [independent-second.txt](rndr-flow-f4-r3-independent-evidence/independent-second.txt)
and [independent-repeat.txt](rndr-flow-f4-r3-independent-evidence/independent-repeat.txt).
The initial compile-only log is deliberately retained as `independent-first.txt`:
my probe used a WorldOwner after consuming it with `bind` (E0382). I corrected
only test setup order before execution. It is not a candidate defect or a failed
runtime test, and no failed evidence is silently replaced. Saved passing probe
stdout has only trailing whitespace/blank EOF lines normalized for the staged
whitespace gate; assertions, panic output, coordinates and totals are retained.

### R1/R2 and adjacent boundaries

- **R1 remains fixed:** timer payload/native identity destruction is inside its
  owned catch and before exit acknowledgement; resident receipt observed during
  destruction. Fresh original probe, native-identity control and all **12 timer
  coordinates** pass, including independent work progressing after panic.
- **R2 remains fixed:** monotonic `Panicked > Failed > Succeeded > DroppedUnknown
  > Running` evidence is unchanged; later producer success/drop cannot overwrite
  a known failure. Fresh original probe and all **20 channel-controlled producer
  orderings** pass, with blocked actual producers unacknowledged and charged.
- Reserved, queued, published and cloned receipts are distinct from eligibility.
  Owner retirement alone is not reclaimability. No leak, fabricated exit,
  capacity bypass or stale native write was found; no such impact is inferred
  from a status field alone.
- The existing authoritative writer gates still validate captured owner/lane,
  actual World and native target/revision/operation, not selected UI state.
  Source/Input/native admission, wrong-World isolation, child-first cleanup,
  owning-World runtime drain, full-queue and one-unit progress regressions all
  execute freshly. Generic writer traits remain host contracts, not automatic
  interception of arbitrary application mutation.

## Fresh exact-candidate qualification

New destination: `docs/reports/rndr-flow-f4-r3-independent-evidence/qualified/`.
All builds use the existing target, **offline + locked**, `CARGO_INCREMENTAL=0`,
`CARGO_BUILD_JOBS=2`; no new dependency/runtime/target, release or broad GUI build.

```sh
sh docs/reports/rndr-flow-f4-r3-gates.sh \
  docs/reports/rndr-flow-f4-r3-independent-evidence/qualified
python3 docs/reports/rndr-flow-f4-r3-audit.py \
  docs/reports/rndr-flow-f4-r3-independent-evidence/qualified
sh docs/reports/rndr-flow-f4-r3-independent-evidence/run-independent.sh
python3 docs/reports/rndr-flow-f4-r3-independent-evidence/independent-audit.py
```

| Fresh execution / audit | Result |
| --- | --- |
| Full qualification runner | **22/22 gates, exit 0** |
| Flow / core / Liquid parser / UI consumer tests and doctests | **78 / 831 / 16 / 115 passed** |
| Feature-off / Liquid facade compatibility | **2 + 2 passed**, actually executed |
| Saved independent reviewer suites | **3 + 3 passed**, unchanged sources |
| New independent paused-destructor probe | **6/6 coordinates pass twice**, 1 test each run |
| F4 lifecycle | **360 unique cycles / 720 actual exit acknowledgements**, seeds 1/7/42 |
| F1/F2 lifecycle | **300 unique teardowns**, **all 19 counters** at stable per-World baseline, **zero external pending acknowledgements on every row** |
| F3 budget regressions | **Nine unchanged tests passed**; 4,096-slot/1,024-command case retains 33,375 charged one-unit visits, not a latency metric |
| R1/R2 ordering matrices | **12 timer / 20 producer cases passed** |
| New R3 branch matrix | **56 exact unique coordinates: 11 tracked + 45 task**, plus promoted controls |
| Source identity | **25 exact source fingerprints**, current bytes equal reviewed candidate Git blobs |
| Strict lint / formatting | Flow feature-off all-targets and UI all-targets `--no-deps`, `-D warnings`; format/diff checks passed |
| Dependencies / protected source | Feature-off flow/facade, Liquid facade, normal/build/reverse dependency and protected-source checks passed |

**Nine ignored tests remain UNRUN** (seven core, two UI), not counted as passes.

The shared runner/auditor logic was inspected rather than trusted by its final
label: before/after disk guards, gate return status, explicit compatibility
execution, exact fingerprints, protected-source checks, and matrix coordinate
checks were read. The independent auditor separately requires all **22 unique**
zero-status rows and each log's success footer, validates exact expected test
sums/coordinates, and compares the new source digests to candidate Git blobs.
It verifies every F1/F2 19-element vector against a stable per-World baseline
(one global listener, all other counters zero), not a scalar or final row.
F4's raw lines print selected counters; complete `Counts == baseline` and
`WorkCounts == default` are assertions in the executed compiled test after each
cycle, NOT an invented full-vector raw log. All acknowledgement coordinates are
checked. It also verifies all prior tracked report paths unchanged and no
protected production/test/manifest/lockfile modification.

Raw commands/status/disk: [summary.tsv](rndr-flow-f4-r3-independent-evidence/qualified/summary.tsv).
Separate review audit: [independent-audit.txt](rndr-flow-f4-r3-independent-evidence/independent-audit.txt).
Shared auditor rerun after completion: [post-worker-audit.txt](rndr-flow-f4-r3-independent-evidence/post-worker-audit.txt).
The in-run shared audit precedes recording its own final summary row; the
post-run independent audit requires the complete 22-row summary.

## Scope, resource limits and recommendation

The initial checkout was clean at the exact candidate. The runner creates its
new evidence directory before its environment log, so that log legitimately
shows that new untracked directory, not a tracked candidate change. Raw environment
records Rust 1.98.0 and Cargo 1.98.0. Recorded gate free-space readings stayed
above **7.54 GiB** (minimum **7,906,628 KiB**), comfortably over the **4.25 GiB**
runner margin and **4 GiB** stop floor; about **7.54 GiB** remains. No unrelated
files/artifacts were deleted. Temporary probe copies under tests are removed;
only new review documentation/evidence is committed. No production fix was made.

Preserved limits:

- Each actual owning World/runtime must drain its own effects and event buffers;
  retained receipts keep charging, and unknown/blocked actual work stays charged.
- Exit acknowledges the wrapped user future/controlled producer, not arbitrary
  wrapper/output/backend/IO termination; UI observation cancellation is not
  authority to stop durable work. The new probe demonstrates that distinction.
- Unwind-capable sequential owned destruction is covered. True panic during an
  **active** unwind, panic-payload destructor panic, panic-abort, crash/forced
  termination and arbitrary caller-owned payload destruction are excluded.
- Fixed seeds/channel schedules are bounded evidence, not exhaustive interleaving
  proof. No total latency/performance claim; synchronous exact observations,
  admission/ancestry work and depth-proportional final retained-token destruction
  remain outside the charged retirement budget.
- No F5/F6, application migration, GUI/native IME/accessibility/window certification,
  backend termination, SDK authorization, durability, release/performance or full
  Medousa-readiness certification. Facade tests may compile existing render
  dependencies; they do not open a GUI or certify native behavior.

**Recommendation: sender may assess this exact F4 candidate as passing the scoped
independent review.** R1/R2/R3 are closed for this candidate; historical FAIL
records remain intact. Final acceptance, any further corrections and sealed
native integration remain with the sender. Nothing was approved, applied,
merged to `main`, pushed or deployed by this reviewer.
