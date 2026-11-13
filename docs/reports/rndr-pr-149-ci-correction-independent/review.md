# PR149 CI correction — independent review

## Verdict: PASS, bounded CI-repair recommendation

Reviewed exact candidate **`2b38dd72cbce034d15fcf5600d37761e38fb2e97`**,
evidence-only child of qualified source/tooling
**`e228c824618aec25b7f8657d1569472925486b24`**, itself directly over previous
independent FAIL review `e89d0bff32ea94941063af11c496162b5fe5a261`.
All prior actionable findings are closed for this bounded review; no new
blocking defect reproduced. This supersedes the prior FAIL disposition for
this corrected revision only; the original report and its evidence are intact.

**This is not Rust 1.100 CI-green certification.** Recommend owner assessment
and sealed native review, then authorized integration onto `rndr-flow-f0-f5`
only, push of PR149 and fresh remote matrix inspection. Owner retains all of
those decisions/actions. No push, integration, merge, main edits, production
corrections, installs, broad rebuild/cleanup or guard/warning/CI weakening here.
This directory contains only appended independent review/evidence.

## Findings and closure

### 1. Four downstream deprecated calls: CLOSED

Compared complete Git trees and the exact production diff, not just the
implementation's path list. The correction replaces exactly four
`.fetch_update(` tokens with `.try_update(` in:

- `crates/rndr_flow/src/lib.rs` (authority allocator), once;
- `crates/rndr_flow/src/topology.rs` (topology freeze), once;
- `crates/rndr_liquid_ui/src/lifecycle.rs` (single/range serials), twice.

The corrected blobs equal the parent blobs with ONLY those bytes replaced.
All 598 tracked nonhistorical Rust files were independently read from the
candidate Git tree; no `fetch_update` token remains. The frozen baseline under
`evidence/` is not compiled workspace source and remains unmodified. This closes
the concrete Rust 1.100 deprecation prediction, not every possible future lint.

### 2. Semantics, overflow and ordering: PASS

Integer `fetch_update` forwards its closure and both orderings directly to
`try_update`. Authority still uses AcqRel success / Acquire load and failed CAS;
its checked-add closure refuses MAX without increment and returns old-plus-one
only after success, so the later addition cannot wrap. Error::Exhausted remains.
Topology uses the same orderings and checked-add, constructs the RAII lease only
after success, and retains AcqRel decrement and participant/owner invalidation.
Its owner acquisition and all callers are unchanged. Single/range serials keep
Relaxed/Relaxed, return the old value, refuse checked overflow without partial
reservation, and preserve the existing zero-count behavior. There is no stronger
synchronization, uniqueness, lifetime or admission guarantee than before.

Fresh existing tests passed authority MAX-1/MAX, serial MAX-1/MAX and range
all-or-nothing refusal. Freeze paths/transitions and lifecycle consumers passed.
No new synthetic topology-MAX fixture or exhaustive CAS/memory-model proof.
Pure arithmetic and byte-preserved call contracts support that narrow branch.

### 3. Actual try_update MSRV evidence: CLOSED

Read the supplied corrected `msrv-api.txt` and independently decoded installed
std HTML source. The integer macro's ACTUAL `pub fn try_update` has
`#[stable(feature = "atomic_try_update", since = "1.95.0")]`, FnMut over
`$int_type`, the weak-CAS loop and old-value Result. Its deprecated compatibility
method directly forwards the three arguments. `static.txt` includes the correct
integer excerpt and document SHA256, not AtomicBool or the unrelated update API.
MSRV remains 1.95. This is source metadata evidence, not execution on 1.95/1.100.

### 4. Dirty-source failure propagation: CLOSED

Authorized driver change is exactly `sh -c` -> `sh -ec` for the environment gate.
A failed initial `git diff --exit-code HEAD -- crates Cargo.toml Cargo.lock` now
stops that command rather than being masked by later `df`. The monitored run()
retains nonzero status, records it and sets the driver's aggregate failed flag;
it deliberately continues later gates, then exits nonzero. This is not a promise
to abort all subsequent qualification on the first gate failure.

Read and reran the fixture regression: clean passes; staged AND unstaged tracked
source modifications fail before rustc/df. The bounded correction runner uses the
corrected monitored function and an independently explicit `sh -ec` environment
command. Git/blob checks independently establish source identity here. The gate
is not a cryptographic seal or a general detector of every untracked input;
source auditors additionally reject untracked crates. No such input was found.

### 5. Nonregenerating manifest / compiler evidence: CLOSED

Read the entire two-tool diff and both current auditors. Original repair audit
now compares the recorded manifest with hashes of ORIGINAL SOURCE Git blobs;
it never generates/relabels history as corrected-HEAD evidence. Its output
explicitly says original repair, not current HEAD. The separate correction audit
checks current source against e228c824 and its new 589-entry manifest.

Both auditors freshly pass. Independent fault-injected READ probes rejected:
(1) corrupt original manifest, (2) corrupt corrected manifest, and (3) a Clippy
log with its successful completion body removed while summary/exit stay zero.
Path write methods were forbidden during these probes. Historical bytes were
never dirtied, even transiently; before/after hashes match. See probe-audit.py/txt.
Auditors check exit suffixes and Clippy bodies now, not summary alone. These are
useful consistency checks, not signatures or independent compiler execution.

## Overall original repair and preservation

Reinspected baseline-to-original-candidate production/test diff and original
review. Its freeze API rename is equivalent for the same reasons above. The
structured mark traversal enumerates before skip, preserving absolute indices;
validated ordered endpoints bound last below blocks.len(), so last+1 cannot
wrap and take/skip yields the original inclusive range. Slicing, endpoints,
mark updates, selection and history semantics remain unchanged. Fresh composer
coverage passed forward/reverse multi-block and same-block selection/history.
The two-barrier freeze test deterministically holds all eight leases through
count assertions; a broken assertion can strand peers, a retained diagnostic
limitation rather than a current count race. All three freeze tests passed.

Independent complete-tree verification establishes that EVERY previously tracked
path at e89d0bf is unchanged except those three rename files and the two expressly
authorized qualification tools. No old report, test assertion, harness, receipt,
manifest, workflow, lockfile or toolchain file was rewritten/deleted. The candidate
is an evidence-only child, with no non-doc differences from qualified parent.
Both independently checked 589-file manifests match their own Git revisions;
current source is 583 baseline-identical / six changed files. Historical bytes
are preserved, not retroactively certified for a new source revision.

Read the full correction report, tools, runner, support receipts and complete
qualification log bytes using independent checks. All 14 recorded correction
gates have matching zero exits/log suffixes, correct test totals, local compiler
identity and disk samples. Seven Clippy command configurations match CI flags;
all are local 1.98 evidence. Original qualification's 16-success/one disk-floor
refusal repeat remains historical and is still NOT an aggregate pass. The
original full-core/renderer/probe evidence was not relabeled as freshly rerun.

## Fresh independent checks and disk

Existing shared F5 target selected explicitly; jobs=1, incremental=0, dev/test
debug=0. Fresh `summary.tsv` has ten monitored zero-exit gates:

| Check | Result |
|---|---|
| Exact-source environment/Git | PASS, candidate 2b38dd72 |
| `cargo test --offline --locked -p rndr-flow --no-default-features` | 144 passed / 0 ignored |
| `cargo test --offline --locked -p rndr_liquid_ui` | 115 passed / 2 inherited ignored |
| `cargo test --offline --locked -p rndr_core --no-default-features --lib freeze::tests` | 3 passed |
| `cargo test --offline --locked -p rndr_core --no-default-features --test structured_composer` | 9 passed |
| Correction test-gates.py | PASS clean/dirty gate + manifest rejection |
| Original historical auditor | PASS |
| Correction auditor | PASS |
| `cargo clippy --offline --locked --workspace --all-targets -- -D warnings` | PASS |
| `cargo fmt --check` | PASS |

Independent static checker is saved as `verify.py` with complete result
`static.txt`; separate three-corruption probe also passed. No compilation lines
in fresh Cargo receipts: existing artifacts reused (Cargo admission 0.07–0.74s).
Consumer's full 300-cycle resource test took 87.91s and passed; the >60-second
status was not mistaken for a hang/failure. No selective skip was added.

Disk before review: 4,534,328 KiB free (~4.3 GiB). Fresh monitored gate minimum
before/after: 4,522,364 KiB; 3-GiB monitor unchanged, no blocker emitted. The
preserved harness's higher 4.25-GiB guard was untouched and not invoked. Disk
variation includes unrelated host activity; this is not storage attribution.

## Limits

Local Rust/Cargo/Clippy 1.98 cannot establish Rust 1.100 GitHub CI green. No local
1.95/1.100 compiler execution, new toolchain install or fresh remote CI. Fresh
review reran workspace Clippy only; the other six corrected Clippy configurations
were independently receipt-audited, not rerun by this reviewer. Full core,
renderer and old adversarial/seam harnesses were not freshly rerun on corrected
source; historical passes remain qualified as historical. Nine inherited ignored
tests remain unrun overall (two encountered in fresh consumer run, seven core
outside the selected freeze run). All-targets Clippy is not all-feature runtime
execution. Linux/Windows cfg and native libraries, workspace/all-feature tests,
package/docs, browser/WebView/GUI/IME, durability, performance and backend exit
are not certified. Existing resource/panic exclusions, F6/application migration
and prior F0–F5 limitations are unchanged. No new actionable blocker found within
this bounded repair/review; remote stable compiler and platform matrix remain
required owner verification, not an omitted local success claim.
