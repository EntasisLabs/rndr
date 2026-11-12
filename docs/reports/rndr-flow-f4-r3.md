# F4 scoped R3 correction — owned discarded-output destruction

## Authority and exact provenance

Same authorized F4 undertaking, implementing-coder correction only. Sender
accepted the independent FAIL evidence, **not candidate correctness**. R1/R2
are preserved, not reopened or reinterpreted. This is not F4 acceptance,
approval, integration, publication or deployment.

- Starting clean review-only HEAD:
  `5b648bb723263064f1096bfc2aeeb31f14813d4f`, immediate parent reviewed candidate
  `218e06c9998296a87f6393e182f03f9e40e97b92`.
- **R3 production candidate:** `896ed5e4c294caa83550257226541cda245825cf`,
  immediate parent `5b648bb723263064f1096bfc2aeeb31f14813d4f`.
- Unchanged accepted F3 / `main`:
  `f16e5572d35450d586dd4f10e00591fa36fb2b04`.
- The commit containing this report adds only this report and fresh qualification
  receipts to that production candidate. Its final exact HEAD and immediate
  parent are returned in the terminal handback, after a full exact-HEAD rerun.
  It cannot embed its own OID without changing that OID.

All historical reports, reproducers, qualification scripts/auditors and raw
receipts remain byte-unchanged, including both independent FAILs. The new audit
compares **every pre-existing `docs/reports` path** against starting HEAD, not
only a selected subset of the old evidence. F0/epic, accepted-F3 budget proofs,
R1 timer/receipt destruction ordering and R2 outcome transitions are unchanged.

## R3 finding, narrow impact, and source correction

The saved public probe proves a channel-controlled actual producer blocks while
feed queue cleanup panics on the owning thread. Cleanup is caught and returns
before the producer is released. Aggregate status is Panicked, cleanup attempted,
exit unacknowledged, and one work slot remains charged through repeated pumps.
Only then is the producer released to return a successful **nontrivial** output.

`Tracked::poll` caught future polling and future destruction separately, but its
known-Panicked branch substituted `Err(Panicked)` and implicitly destroyed the
unused successful output **outside both catches**. Its destructor asserted no
active unwind and then panicked. That second, sequential unwind escaped the
flow wrapper instead of returning the promised normalized failure.

**This did not demonstrate a leak, fabricated exit acknowledgment, stale write,
data loss or false aggregate success.** Actual future exit was acknowledged,
aggregate Panicked remained correct, and capacity eventually reclaimed. The
independent review's narrower finding is retained exactly. This is not the
excluded true panic-during-active-unwind/process-abort case.

The **sole production change** is five added lines (four comment lines and one
statement) at `crates/rndr_flow/src/work.rs:173–179`: explicitly destroy the
suppressed `value` inside its own `catch_unwind` before returning `Err(Panicked)`.
The value moves once into that catch, so implicit scope exit cannot destroy it
again. The aggregate is already Panicked, the maximum evidence rank; catching
this owned discard needs no new status mutation or outcome downgrade.

`drop_inner` still destroys the actual user future under its separate catch and
records its real returned/unwound exit at the existing point. No acknowledgment
is moved earlier, and no observer-side ack setter is introduced. In owned task
paths the pre-poll reserved receipt remains live across future and suppressed
output destruction, and across a published failure envelope until buffer drain.
For `ProducerExit::track`, an acknowledged **user-future exit** is not a join of
the wrapper's remaining instructions, arbitrary output destructors or backend IO.
This correction does not expand that observation into such a guarantee.

Shared-path audit:

- `replace_local` and `replace_background` both await the same `Tracked` wrapper
  inside the existing reserved completion, including injected executors.
  Their core outer catches can otherwise hide the escape by losing the failure
  envelope; new task tests require that envelope actually be published.
- Successful user-future destruction can itself panic after returning the output.
  It is caught before suppressed-output destruction starts; both normal and
  panicking discarded outputs are now tested in that composition.
- Explicit Failed/Panicked, actual poll panic followed by normal/panicking future
  destruction, and pending-future cancellation/drop use the same existing
  normalization/drop path. Drops observe no active unwind in these tests.
- A custom executor may retain its actual Job then panic during spawn. Its known
  aggregate panic cannot become an exit ack. The reservation remains until the
  actual held Job is polled/dropped and the real receipt is released.
- `ProducerExit::run` returns unit and has no suppressed non-unit success here;
  its closure-result versus aggregate-status distinction remains unchanged.
  TimerCompletion's R1-owned destruction catch and R2 evidence rank are unchanged.
- Returned, nonsuppressed successful outputs are still caller-owned, not silently
  destroyed by the library. No new promise is made about arbitrary caller-owned
  destructor panic, panic payload destructors, true nested field double-panic,
  `panic=abort`, process kill or crash.

## Deterministic permanent regressions and red/green

New evidence only: [`rndr-flow-f4-r3-evidence/`](rndr-flow-f4-r3-evidence/).

1. **First**, on clean starting HEAD, ran the byte-unchanged saved
   `rndr-flow-f4-rereview-evidence/reproduce.sh`, redirected to
   `red-reviewer.txt`. It produced the expected **exit 101, 2 passed / 1 failed**,
   `output_panic=true normalized=false`. Historical source/logs were not edited.
2. The same two byte-unchanged saved reviewer suites were temporarily installed
   and executed after the fix: **3 + 3 passed**, `green-reviewers.txt`.
   Disposable copies are removed by the new probe runner's trap. Both suites are
   also rerun in every full qualification, not merely replayed from this log.
3. `tests/f4_discarded_output.rs` promotes all three latest saved probes into
   permanent coverage: actual channel-controlled producer, normal output control,
   and native-identity timer destructor control. The first cleanup catch is
   complete before releasing the producer; output destruction asserts no active
   unwind and exactly one output drop. Blocked actual work remains unacknowledged
   and charged, then actual exit and eventual reclaim are required.
4. `tests/f4_output_branches.rs` adds two matrix tests with **56 exact unique case
   coordinates**, not 56 separate test functions:
   - **11** direct tracked-future branches: nontrivial Ok, explicit Failed and
     Panicked, actual poll panic, Pending; normal/panicking future destruction;
     and panicking output destruction when the library suppresses success.
   - **45** task cases across non-Send-capable local, default background,
     retained custom Job, and retained custom Job with caught startup panic.
     They cover the same Ready/failure/poll-panic/Pending branches, normal or
     panicking user-future destruction, and suppressed-output panic/control.
   - Future destruction observes `exit_acknowledged=false`; actual task receipt
     count is one inside destruction. Exactly-once drops, pre-exit pending charge,
     published receipt charge, actual exit and eventual zero WorkCounts are
     asserted. Normal returned-output control proves the library does not
     eagerly destroy a returned success. No sleeps/private status injection,
     fabricated producer ack or newly implemented executor/runtime is used.

`green-development.txt` is the initial narrower branch development run;
`branches-expanded.txt` is the final 56-coordinate development run. They are
not substituted for the committed-source full qualification below.

## Fresh full qualification on the exact committed production candidate

All **22/22 gates passed**, with zero exits, against `896ed5e…`:

```sh
sh docs/reports/rndr-flow-f4-r3-gates.sh docs/reports/rndr-flow-f4-r3-evidence/qualified
python3 docs/reports/rndr-flow-f4-r3-audit.py docs/reports/rndr-flow-f4-r3-evidence/qualified
```

The runner refuses an existing destination and monitors the **4.25 GiB** stop
margin. Every Cargo command uses existing target only, offline/locked,
`CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2`. No new target root, release build or broad
GUI build is used. The full script records exact commands/exits/disk readings.

| Executed test suite / gate | Fresh result |
| --- | --- |
| Flow feature-off, including doctests | **78 passed**, 0 ignored; five new permanent test functions |
| Core feature-off | **831 passed**, 7 ignored |
| Liquid parser | **16 passed**, 0 ignored |
| UI / native flow consumer | **115 passed**, 2 ignored |
| Facade editing compatibility, feature-off / Liquid-on | **2 + 2 passed**, executed, not just compiled |
| Both unchanged independent reviewer probe suites | **3 + 3 passed** |
| F4 lifecycle | **360 unique cycles / 720 actual exit acknowledgments**, seeds 1/7/42, 120 cycles each |
| F1/F2 lifecycle | **300 unique teardowns**, complete **19-counter** vectors equal stable per-World baseline, zero external pending acknowledgments on **every row** |
| F3 budget tests | **All nine unchanged** tests pass; full 4,096-slot/1,024-command proof remains **33,375 charged one-unit visits**, not a latency measurement |
| R1/R2 matrices | **12 timer / 20 real channel-controlled producer** orderings all pass |
| R3 matrix / promoted tests | **11 tracked + 45 task coordinates**, promoted panic/control and native identity control all pass |
| Strict lint | Flow feature-off all targets; UI all targets `--no-deps`, both `-D warnings`, pass |
| Format / whitespace | `cargo fmt --all --check`, working/staged `git diff --check`, pass |
| Feature/dependency boundaries | Feature-off flow/facade and Liquid facade lib checks, normal/build/reverse graphs, protected-source checks pass |
| Exact-source audit | **25 refreshed fingerprints** match current bytes and exact candidate Git blobs |

The F4 cycle test asserts full `Counts == baseline` and `WorkCounts == default`
after every cycle; raw selected-field lines are not relabelled a full vector.
The F1/F2 log actually prints all 19 counters; the auditor checks fixed per-World
baselines across all rows (one global listener, all other baseline counters zero),
unique cycles and zero external pending each time. The new auditor extends rather
than changes the prior full-vector/source auditor and verifies every R3 matrix
coordinate and both unchanged probe suites. Its in-run final audit naturally
precedes insertion of its own summary row; the complete summary has 22 rows.

Committed raw results and exact-source fingerprints:
[`qualified/summary.tsv`](rndr-flow-f4-r3-evidence/qualified/summary.tsv),
[`qualified/exact-source-audit.txt`](rndr-flow-f4-r3-evidence/qualified/exact-source-audit.txt),
[`qualified/source-digests.txt`](rndr-flow-f4-r3-evidence/qualified/source-digests.txt).

After the evidence/report-only commit, full exact-final-HEAD qualification is
rerun into **`target/f4-r3-exact-head/`**, followed by the auditor. Its actual
terminal outcome and exact HEAD/parent are returned in the handback rather than
claimed by this pre-rerun report. The production source/fingerprint set does not
change in that evidence-only descendant.

## Scope, limits, disk and handback

No core/facade/UI production changes, manifests/lock changes, new runtime,
executor implementation in production, Tokio, F5/F6, SDK/Medousa migration,
main integration, push, deploy, approval/apply/merge. No unrelated deletion.
The initial full run's minimum recorded free space stayed above **7.5 GiB**, well
above the 4.25 GiB runner margin and 4 GiB floor; exact readings are committed.

The nine ignored tests remain **unrun**, not passed: `snapshot_storage_accounting`,
`editor_scale_costs`, `scroll_and_fold_stability`, `unique_line_editor_costs`,
`markdown_virtual_cost`, `markdown_idle_shape_cost`, `model_costs`,
`liquid_stream_cost`, `liquid_density_cost`. No release/performance, GUI/real-IME,
backend termination, durability or whole-application certification is claimed.

Hosts must keep producer guards with actual producers, drain the actual owning
Worlds/runtime buffers, retain receipt/clone charges, and keep blocked work
pending. A cancellation flag or quiet pump is not an exit/join. Existing native
writer authority, synchronous admission/commit, F1 staging/F2 split-CRLF and F3
observation/depth-proportional final-destruction caveats remain. Budgeted registry
visits do not bound callback/destructor/executor duration. True simultaneous
panic-during-unwind remains excluded; the sequential panics repaired here are not
reclassified into that exclusion. Tests use controlled orderings, not an exhaustive
interleaving proof or a background thread-pool/OS certification.

**Worker delivery only.** Sender retains assessment, fresh independent exact-head
review and sealed native integration. Historical FAIL remains FAIL for its exact
old candidate; new green implementing-worker qualification does not approve F4.
