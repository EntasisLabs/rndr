# F3 correction: incremental retirement metadata

Date: 2026-11-12 UTC. Work `work-901f2ce85e86e30b96529a611e423516`.
Status: **corrected candidate, locally qualified; fresh independent review and
sender acceptance still required. Not approved or integrated.**

Current correction parent: `b3bf25bb1095ada20694dbc28816934479b4f8a2`.
Original F3 candidate: `fa5a03b76975068e015cc5f373baadaff1b703fe`.
Accepted `main`: `9f36c97271a3d2e7ead3049afdec89753ef990c3`, unchanged.
The correction is the commit introducing this report; resolve its full identity
with `git log -1 --format=%H -- docs/reports/rndr-flow-f3-correction.md`.
The worker's terminal handback supplies exact HEAD and cleanliness verification.
No push, deploy, native approval, merge or integration was performed.

## Follow-up: atomic lease refusal (R1) and internal ancestry caveat (R2)

The independent review of exact `b3bf25bb` requested changes for R1 (P2).
Its review deliverable was accepted by the sender, **not the project**.
The native seal still pointing at superseded `fa5a03b` is not evidence of this
correction or of `b3bf25bb`; no seal/approval/integration action was taken here.

R1 reproduced: an inactive lease occupied the only slot; with lease serial MAX,
`lease()` reclaimed that slot before returning Exhausted, changing exact leases
from 1 to 0. F0 §3 prohibits partial mutation before exhaustion refusal. This
was an inherited defect, not a newly introduced failure in incremental pumping.

The narrow fix preserves stale/foreign/invalid validation first, then preflights
`lease_serial.checked_add(1)` without changing the counter. Exhaustion refuses
before any capacity reclamation. Capacity refusal likewise consumes no serial.
Only successful checks permit inactive-slot removal, counter commitment and
insertion. No public API/type, pump algorithm, F4/core/UI/Liquid/Medousa behavior,
dependency, runtime or facade adoption changed.

Five permanent private-seam tests in `src/lease_admission_tests.rs` cover:

1. Exhausted admission with a retained inactive slot (exact 1/0 leases/active).
2. Exhausted admission with a retained active slot; token remains held and
   document close still refuses InUse.
3. Exhausted mixed inactive/active storage (exact 2/1), retaining both entries
   and the active presentation's validity.
4. Nonexhausted Capacity refusal does not consume the final serial; after lease
   drop the replacement succeeds with serial MAX, replacing just one slot.
5. Exhaustion retains stale/foreign scope/document and invalid document-as-owner
   validation precedence.

Every refusal checks exact Counts, all three identity counters, retained lease
keys/scope/document/token identity, diagnostics/dropped telemetry, FIFO commands,
dirty flag, traversal and lease cursors, plus absence of callback invocation.
The original 15 tests, nine budget proofs and three retained reviewer probes
remain unchanged.

The isolated appended reviewer regression is retained unchanged in
[`f3-refusal-evidence/reviewer-regression.rs`](f3-refusal-evidence/reviewer-regression.rs).
[`f3-refusal-probe.sh`](f3-refusal-probe.sh) builds isolated source copies, never
modifying candidate source. The original prefix is exactly `git show b3bf25bb`
plus that appended test; **before exits 101** with leases 0 versus expected 1.
The fixed prefix plus the same test **exits 0**. See `private-before.log`,
`private-after.log` and the exact compiler/runner commands in `private-command.log`.
These are injected-counter tests, not fabricated public IDs or a near-MAX stress
claim. The expected historical failure is not counted as a passing gate.

R2 (P3 advisory) is documented in README and Liveness rustdoc, not redesigned:
external tokens/retained owners hold crate-owned strong-parent `Life` Arc ancestry
after registry Counts reach zero. Last-token release can recursively destroy
O(depth) internal nodes, including from a cleanup callback capture, even with no
custom user Drop. External retained metadata is outside Counts. This is distinct
from ordinary child-first registry removal, which retains the parent's registry
reference. The review demonstrated neither stack overflow nor a hidden
callback-free full scan. No stack-safe or hard-real-time token-drop promise is
made. Metered coordinator work and synchronous observation/admission exclusions
remain exactly as qualified below. Stronger token reclamation needs separate work.
The review's allocator probe/logs were inspected; they were not rerun for this
documentation-only clarification and are not claimed as fresh qualification.

### Fresh qualification of this follow-up

[`f3-refusal-gates.sh`](f3-refusal-gates.sh) reuses the earlier scoped runner with
a separate output directory, preserving previous evidence. All **19** entries in
[`f3-refusal-evidence/gates.tsv`](f3-refusal-evidence/gates.tsv) return zero:

| Fresh rerun | Result |
| --- | --- |
| Focused feature-off flow suite | **29 unit tests + 3 retained reviewer probes**, 0 doctests. |
| Targeted work-accounting tests | **9/9**; unchanged wide/deep 4,096-scope proofs each 24,577 units, full 4,096-lease table 8,192 units. |
| Retained reviewer probes separately | **3/3**. |
| Lifecycle proof | **100 exact seven-count baseline vectors**, cumulative callbacks exactly 3 per cycle, **300 total**; final root reclamation asserted. |
| Focused strict all-target Clippy / workspace fmt | Passed (`-D warnings`, `--all-targets`; `fmt --all --check`). |
| Headless build/graphs | Feature-off flow build; no UI/Liquid/render/Tokio dependency or reverse adopter. |
| Targeted compatibility | Core/UI/paint no-default check; facade editing no-default **2/2**, UI **3/3**, Liquid **2/2**. |
| Documentation / whitespace | Strict flow no-deps docs; complete staged diff whitespace check. |
| Preservation | Original `docs/reports/f3-evidence` unchanged from fa5a03b; prior correction evidence and F4 preparation unchanged from b3bf25bb. |

`environment.log` records pre-commit parent HEAD and six SHA-256 source/test
hashes; those hashes bind the tested content, not the parent OID. Qualification
uses `CARGO_INCREMENTAL=0` and existing artifacts, with approximately **12 GiB**
free before/after. No clean/full rebuild or competing coder grant was used.

The broad original qualification remains **inherited, not rerun**: broad
Liquid/UI/core/render/source-session suites, workspace check, broad Clippy,
multi-package docs and additional facade lib/doctest configurations. Targeted
facade tests do not establish comprehensive compatibility. The historical
incremental correction narrative and gate results below describe `b3bf25bb`,
not additional fresh gates of this follow-up.

F4 preparation/caveats are preserved without F4 implementation. F1 staging cost,
F2 raw-push split-CRLF, platform/GUI/performance/concurrency/IO acknowledgement
and arbitrary-destructor/double-panic limitations remain. Registry settlement is
not producer/backend quiescence. Sender retains fresh independent review,
acceptance and exact native integration responsibility; this is only a corrected
candidate, not approval, acceptance or integration. Nothing pushed or deployed.

## Historical incremental correction (b3bf25bb)

### Finding and scoped correction

The earlier review established lifecycle correctness but qualified the scan
cost. That qualification did **not** satisfy F0 §5 incremental retirement:
`order()` built a full postorder using registry-wide child searches; each
callback restarted that construction; death reconciliation and lease sweeps
also scanned globally. The sender requested project changes, not acceptance.

This correction removes those pump-path scans. Implementation remains confined
to F3 ownership metadata. No F4 adapters, lanes or completion gate, no core/UI/
Liquid production changes, new executor/runtime, dependency, facade export or
Medousa change is included.

### API changes and exact observation boundary

- Add `Coordinator::pump_work(budget: usize) -> PumpWork`. Zero is a strict
  no-op. It performs at most the supplied work units and at most the existing
  `Limits::cleanups_per_pump` callbacks, without invoking `counts()`.
- Add payload-free `PumpWork` accounting: `units`, `metadata_visits`,
  `lease_visits`, `restarts`, `commands`, `callbacks`. The non-callback work categories
  obey `units = metadata_visits + lease_visits + restarts + commands`.
  `callbacks` is a subset of metadata visits, not extra units. Empty cursor,
  table-wrap and callback-cap retry attempts are charged too.
- Preserve `pump() -> Counts` and the existing `Limits`, `Counts`, identity,
  owner, error/refusal and callback signatures. The compatibility wrapper does
  at most **65,536 retirement units**, then **explicitly invokes an unbudgeted
  exact synchronous `counts()` observation**. It is not a total-duration bound.
  Hosts needing bounded retirement call `pump_work` and decide separately when
  to pay for exact observation. The work limit may be reached before the
  callback cap; a pump does not promise a particular number of callbacks.
- `counts()` still reports exact physical retained scopes/cleanups/lease slots,
  active leases, logically pending scopes/cleanups, and queued commands. It
  reflects unpumped token drops, not cached scan results. Its worst-case cost is
  O((scopes + leases) * depth + cleanups), plus ordered-table lookups. This cost
  is not charged to retirement. It is not an atomically transactional snapshot
  against concurrent external token drops, as with the previous API.
- Lease admission no longer sweeps the whole lease registry. At capacity only,
  a synchronous exact observation searches for one inactive slot to reclaim,
  preserving successful replacement admission after a dropped lease rather than
  introducing spurious Capacity. This can cost O(leases * depth), plus table
  lookups; it is not a budgeted retirement operation. Other inactive physical
  slots remain charged until swept. `close_document` also makes an explicit
  exact synchronous lease observation. Neither invokes callbacks.
- Ancestor-aware admission/state/liveness observation remains synchronous
  O(depth), plus lookup. `resume` also examines that scope's remaining cleanup
  kinds. These APIs do not claim a retirement-work or real-time bound.

Exact counts cannot be replaced by counters updated only during pumps: owner
and lease drops deliberately signal external flags without borrowing the
registry. The explicit API split retains exactness instead of hiding an
observation scan behind a nominally bounded return value. At root shutdown,
hosts must drain **all physical counts, including leases and queued commands**,
not merely pending scope/callback counts. README usage now demonstrates that.

### Traversal, progress and preserved invariants

- Index children by immutable creation identity and cleanups by checked
  registration serial at admission. Ordered tables replace repeated full-child
  searches and vector `rposition`/shifting. Index storage remains O(scopes +
  cleanups). A scope still has exactly one immutable parent.
- Resume an iterative Enter / cleanup-entry / Exit cursor. Each step visits at
  most one node/edge or cleanup entry. Ineligible lifetime entries while
  suspended are charged one at a time. Empty dead scopes are reclaimed in Exit,
  so callback-free retirement does not depend on a cleanup queue entry.
- Owner drop or first explicit disposal only sets the already-admitted terminal
  flag and a coalescing atomic dirty flag. Admission closes immediately through
  the unchanged ancestor chain. No identity counter or queue allocation is
  needed for logical death or eventual retirement. Repeated disposal and a
  later drop of that already explicitly disposed owner do not repeatedly reset
  traversal. Stale/foreign/document refusal and checked exhaustion are retained.
- A dirty signal causes a **charged** root restart before more callbacks, so a
  newly dead ancestor's earlier children precede parent cleanup. Suspension also
  requests immediate revalidation: a child suspended between pumps must precede
  an already waiting parent. Creation/registration instead request a later pass,
  preventing unrelated live admission churn from continually preempting work.
- Independently resume lease selection and **one ancestry edge per lease step**.
  Traversal and lease steps alternate, including across one-unit pumps. Lease
  progress is not restarted by death/suspension revalidation. Physically removed
  scopes are sufficient evidence of death; lease sweeping never needs a global
  scope scan. A retained lease token on a dead scope does not pin its slot.
- Cleanup FnOnce is removed before invocation, outside all registry borrows.
  Reverse registration order, child-first/creation-ordered siblings, non-Send
  captures, error/unwind consumption and continuation remain. FIFO commands are
  each charged and revalidated after the callback **before any next callback**;
  tiny budgets may defer this to a subsequent pump. Full queue refusal is
  explicit. Diagnostic and command rings are preallocated at construction, not
  grown while retiring. Diagnostics retain bounded eviction/saturating telemetry.
- No incremental API path calls ancestor `Life::live()`, exact `counts()`, a
  full cleanup/child iterator scan or global death/lease reconciliation. Work
  after a restart is charged as it is revisited; no full scan is hidden inside
  the restart unit.

For finite admitted work/ordering changes, positive-budget pumps and callbacks
that return or unwind, there is no starvation: every finite FIFO command batch
is drained, each stable traversal/cleanup cursor advances or consumes one
callback, and the independent lease cursor advances to the end and wraps.
Each admitted callback is consumed once; terminal signals and required restarts
are finite. A later call renews the callback cap, so even budget=1 makes eventual
progress. Live registration churn alone does not restart the current pass.

A traversal pass costs O(scopes + cleanup entries) units; a lease pass costs
O(leases * (depth + 1)) units. Each unit does a constant number of ordered-table
operations, each O(log admitted capacity). A death/suspension restart may repeat
an entire traversal prefix in the worst case; those visits count against future
budgets. No fixed completion bound is claimed under endlessly introduced
ancestor deaths/new suspensions or a host that stops pumping. Arbitrary callback,
destructor, allocator/deallocator and platform duration cannot be bounded by
these units. **These are work-accounting/progress guarantees, not real-time
latency or performance equivalence claims.**

## Deterministic regressions and local self-review

All 15 previous unit tests remain unchanged and rerun. The three public probes
from accessible ignored `target/f3-review/probes.rs` are now permanent tests at
`crates/rndr_flow/tests/reviewer_probes.rs` (same cases/assertions, normalized
formatting). Original ignored reviewer artifacts were not edited.

Nine additional unit tests in `src/budget_tests.rs` establish:

1. At default capacity, both a 4,096-scope wide tree and 4,096-deep chain reclaim
   without callbacks in **24,577 charged units**, including the measured restart,
   using budget=1. Owner drop occurs between pumps. Zero-budget calls do nothing;
   a one-unit restart does not eagerly reclaim metadata.
2. A full 4,096-lease table releases with no callbacks and one-unit pumps;
   measured **8,192 units** in the test scenario. Counts show physical retained
   leases separately from zero active leases before sweeping.
3. Simultaneously saturated scopes, per-scope cleanup slots, leases, command
   queue and diagnostics reclaim fully; **512 callbacks** are consumed once,
   full queue refusal is explicit and diagnostic eviction does not stop work.
4. Command-caused and owner-drop-caused ancestor death during a child's
   suspended callback both trace `[20, 10, 11, 30]`, preserving the newly dead
   earlier child, reverse registration and parent-last order.
5. Lease validation on a 256-deep scope resumes across pumps and catches an
   ancestor dropped during the scan; shared document lifetime survives.
6. A pending Activity callback behind 4,095 retained Lifetime entries requires
   individually charged visits, not a bulk `rposition` scan. Full later disposal
   consumes all remaining callbacks and reclaims storage.
7. Repeated live cleanup registration cannot starve an in-flight dead subtree.
8. Callback unwind with a full queue leaves exact queued counts at the budget
   boundary; one-unit command drains preserve the foreign-request error order
   before any next callback.
9. A child newly suspended between pumps precedes its waiting parent's cleanup.
   Self-review reproduced `[2,1]` before the suspension revalidation correction;
   permanent test now passes `[1,2]`. See `suspension-order-before.log` for the
   retained expected failure (exit 101), not a final gate pass.

All tests check deterministic work/count/order invariants, not wall-clock limits.
The initial strict Clippy test-style failure is also preserved, not concealed;
final linting passes without suppressions.

## Tests actually rerun on corrected source

[`f3-correction-gates.sh`](f3-correction-gates.sh) records exact commands and
individual statuses in [`f3-correction-evidence/gates.tsv`](f3-correction-evidence/gates.tsv).
Checked-in command output is unchanged except for trimming terminal empty lines
so text evidence passes the staged whitespace gate; exit statuses are retained.
**All 19 final runner entries returned 0** (including environment, graph/vector
and original-evidence audits). Source SHA-256 values in `environment.log` bind
these runs to the corrected file contents. The log records parent HEAD because
qualification preceded the correction commit; source hashes, not that parent
OID, identify the tested working content.

| Actual rerun | Result |
| --- | --- |
| Focused flow suite | **24 unit + 3 permanent reviewer probes passed**; 0 doctests. |
| Work-accounting proofs | **9/9** targeted budget tests, exact charged-unit evidence above. |
| Public reviewer probes separately | **3/3** passed, including FIFO/unwind/full queue, dropped ancestor and suspension. |
| Lifecycle proof | **100 cycles / 300 callbacks**; audit checks every exact baseline vector and final root reclamation is asserted by the test. |
| Strict focused Clippy | `--all-targets -- -D warnings` passed. |
| Formatting | `cargo fmt --all --check` passed. |
| Headless | Feature-off flow build, normal/build dependency tree and inverse graph passed; sole direct runtime dependency remains feature-off core; no UI/render/Tokio or reverse adopter. |
| Targeted feature-off compatibility | Core/UI/paint no-default check passed. |
| Facade editing compatibility | No defaults **2/2**, ui **3/3**, liquid **2/2**, all passed. |
| Documentation | Flow no-default docs with `RUSTDOCFLAGS=-Dwarnings` passed. |
| Whitespace / preservation | Diff check passed; original committed `docs/reports/f3-evidence` is byte-for-byte unchanged from `fa5a03b`. |

Existing artifacts were reused with `CARGO_INCREMENTAL=0`, no target clean or
full rebuild. Disk was checked before work and final reruns, approximately
**12 GiB free** throughout. No competing coder grant or build was started.

### Inherited, not rerun for this correction

The original broad Liquid/UI/core/render/source-session suites, workspace check,
workspace-wide and feature-off broad Clippy, multi-package docs and additional
facade lib/doctest gates remain original-candidate evidence under
`docs/reports/f3-evidence`; they were **not** rerun here. This report does not
promote those historical results into new-candidate qualification. The original
report is retained with an explicit supersession notice. F1 whole-stream staging
and F2 raw-push split-CRLF limitations remain unchanged.

## Remaining boundaries and handoff

This is local macOS headless qualification, not cross-platform/all-feature CI,
GUI/native interaction, backend IO, full performance or Medousa readiness.
Orderly shutdown still requires explicit pumping before Coordinator Drop;
retained external owners/liveness/rejected captures are client responsibilities.
Cleanup attempts and zero registry counts do not imply worker/producer exit or
backend acknowledgement. Abort/process loss and arbitrary destructor/double
panic recovery are not promised. Exact observation/admission calls remain
synchronous as disclosed above.

README and F4 readiness now describe the work/observation split and future F4
accounting obligations; no F4 behavior is implemented. Sender retains project
responsibility and will arrange fresh independent review before any native
integration decision. This worker has not accepted or integrated its own result.
