# Second bounded A1 ownership correction — evidence and provenance

**Candidate only; not A1 acceptance or A2 clearance.** Same authorized rndr UI undertaking; no approval, integration or push. Starting tracked tree clean at `01da8acdc57240cd536234d062933e232b033eec`. Full approved Specification (lines1–339), Review Guide (1–70), decisions, checkpoint and tracking read before edits. Approved proposals unchanged.

## Finding and bounded correction

Independent re-review deliverable accepted as review, implementation **REVISE**: `/tmp/rndr-rereview-01da.3eseqx/REVIEW.md`. P2: merely registering Alt+Space, without a chord match/backend fire/active guard, converted ordinary Space up to synthetic cancellation. Same-frame and split-frame focused modern Button both lost their activation; no-registration controls passed.

`registry.rs` changes only the private Space/Enter ownership predicate. Matching registered chord **or active guard** owns an edge; live base-code registration alone does not. Ownership is evaluated before release clears a guard, preserving cancellation when modifiers have already changed. No change to guard receive-order/synthetic/repeat rules, core activation, scheduling, FIFO64, overflow, drain validation, canonical-producer requirement or legacy/editor policy. One production file, new permanent fixture, three design records and new evidence only.

FIFO64 remains distinct intentional requests in FIFO order up to64, explicit overflow with no eviction, no inferred dedup/coalescing, no modern→legacy Fired fanout. No unrelated cleanup/A2/native integration work.

## Authoritative results

Latest complete run: `identity-verified-run/`. Earlier complete `final-run/` gives the same results. Commands/statuses (including expected starting-fixture exit101) are in each `status.txt`.

| Rerun | Outcome |
|---|---|
| `cargo test --offline --locked -p rndr_core -v` | **413 passed**, zero failed/ignored, including 11 new tests and all14 prior correction tests |
| `cargo test --offline --locked -p rndr_paint` | **62 passed**, zero failed/ignored |
| `cargo test --offline --locked -p rndr_ui` | **14 passed**, zero failed/ignored |
| `cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests` | **1 passed**, 22 filtered; headless actual adapter, not OS AT |
| `cargo check --offline --locked -p rndr --all-targets --no-default-features` | exit0 |
| `cargo tree --offline --locked -p rndr_ui --edges normal` + forbidden GPU/runtime grep | exit0; GPU-free normal graph |
| `cargo run --offline --locked -p rndr_ui --example passive_gallery -- NEW_OUTPUT/gallery` + `diff -rq evidence/ui-gallery NEW_OUTPUT/gallery` | exit0; all **6 byte-identical** display lists |
| preserved recovery/gallery/prior-correction hash checks + archived-evidence diff | exit0; prior evidence unchanged, including invalid cached historical logs |
| targeted `rustfmt --edition 2021 --check` registry/interaction/old+new correction fixtures | exit0 |
| original independent `adversarial.rs`, explicit candidate rlib | **4 passed**, exit0 |
| original `probe.rs` and `mixed.rs`, explicit candidate rlib | exit0; one Tab+Space intent, no unmatched-release intent; legacy Tab+Enter and separate-frame Enter Fired("legacy") |

Environment: existing worktree target only, offline --locked, `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. Initial disk3.2GiB free; final3.1GiB; target969MiB. Small unique `/tmp/rndr-a1-ownership-*` direct-rustc source/binary directories retained, not a second Cargo target. No network/secrets/raw HTTP/custody workaround or broad cleanup. No invalidation was needed this time.

## Identical fixture counterfactual / build identity

Permanent fixture `crates/rndr_core/tests/a1_hotkey_ownership.rs` is copied byte-identically as `fixture.rs` and in each run. SHA256 `4699893bc4f7509860d1c9d88bdcb641414be9d5bc8c64c4b2842d2ea2fc28cd`.

- New11 tests cover same/split-frame logical+physical Space/Enter, Alt/Ctrl registrations of same/other base keys, no-registration controls, matching chord/release cancellation, active guards with stale/synthetic/repeat edges, fresh post-guard activation, traversal/focus cancellation, legacy Enter/editor Space. Existing original14 fixture remains byte-unchanged (SHA256 `7adf72bcf44793dc7da042d6589aadd87360ba6b056412fcc84498497173464c`).
- Starting exact01da source exported via `git archive`, source hashes recorded in `starting-sources.txt`, compiled **directly via rustc** with explicit existing dependency rlibs. No Cargo workspace/snapshot fingerprint selection. Exact commands, rustc identity, output library hash and unique paths recorded. No checkout/source switching.
- Same fixture compiled via `rustc --test ... --extern rndr_core=EXPLICIT_RLIB` twice. **Starting: 6 pass / 5 fail, exit101. Corrected: 11 pass, exit0.** Both nonmatching ordinary Space same/split positives fail starting source; traversal and two post-cancellation/post-guard positive tests also fail on their fresh ordinary Space gesture. Enter/no-registration and other compatibility controls pass starting source. These are expected historical failures, not outstanding candidate failures.
- Actual candidate Cargo library rustc invocation is recorded in `initial-focused.log` (not just Cargo Fresh). It compiles current `crates/rndr_core/src/lib.rs` after the registry edit. Candidate rlib: `target/debug/deps/librndr_core-99fc9f1d1448f44b.rlib`, SHA256 **`3073d3d06f9684c75b2504e18d9f36ea0147db3554106a929c6eb083aa1cbb90`**. Initial/final source manifests and rlib hashes match; hashes also match after counterfactual/suite/gallery execution. Source registry SHA256 **`eee07c4df86fe363823c9855726727792d527dbaa432dc3595c59a9ca3e07233`**.
- `run.sh` now checks those source/artifact hashes and the actual initial rustc invocation even if a repeat suite reports Fresh. If future source/toolchain changes invalidate identity, stop and establish a fresh actual build; do not accept a snapshot-cache collision. If necessary, preserve/quarantine only the exact colliding fingerprint and rebuild verbosely; never broad-clean. Library hashes depend on toolchain/path/configuration; source/fixture hashes and explicit build commands provide portable provenance.

Reproduce from worktree root into a **nonexistent** output directory:

```sh
sh evidence/a1-ownership-20261101-2t23Vj/run.sh /tmp/rndr-a1-ownership-NEW-UNUSED-PATH
```

The runner refuses existing output directories. Do not run old recovery scripts that overwrite archived logs.

## Evidence custody / inspected versus rerun

`reviewer/REVIEW.md`, `adversarial.rs`, `adversarial-expanded.log`, `identity.txt`, `status.txt` are byte-for-byte copies from the independent `/tmp/rndr-rereview-01da.3eseqx/` directory, which remains intact. Copied report SHA256 `8c63fe2537047f836cb06e7288103429a985e6ae74ae4ac400a5c387fc1c893f`; repro `581c5d2097672b49f8fea710afcca15d4e4831e439899c56f23cd5769143f013`; failing log `fab97125a099355382fc5bb1e7e55f06d79a9c39dac22e29914363fe1a7444d4`. `reviewer-copy-verification.log` records source/copy hash equality. Probe/mixed copies come unchanged from committed prior-correction evidence.

`initial-focused.log` is a successful focused candidate build,11pass, with actual rustc invocation. `verified-run/` is an **incomplete harness attempt**: core413pass and starting fixture6pass5fail were correct; runner had guessed9pass2fail and stopped at its own expected-count grep. No source/fixture was changed; only the harness count was corrected. `final-run/` completed all checks. Runner subsequently gained explicit source/artifact identity fences, exercised successfully in `identity-verified-run/`. No new run relies on an invalid cached library. All partial logs retained, no overwrite/normalization.

Read/inspected prior design/checkpoints, original14 correction fixture, interaction dispatch, keys matching (including matching releases), guard lifecycle helpers and existing hotkey test cases. The full rerun includes existing hotkey/editor/IME/history/source-edit tests. The earlier14 fixture's **f7aa9703 11fail3guardrail** counterfactual was independently demonstrated in the accepted review report; it was **inspected, not repeated against f7aa9703 in this correction**. This correction's counterfactual is against exact starting01da. Historical reports/logs are provenance, not a new independent review of this correction.

`source-docs-sha256.txt` identifies current source/tests/design records. `files-sha256.txt` hashes new evidence excluding itself. Inventories separate focused delta from baseline scope. All prior evidence is untouched and hashes/diff verify that; earlier archived invalid cached runs remain clearly invalid, not acceptance evidence. Git commit identity is returned in the handoff (not recursively embedded in its own evidence). Full staged `git diff --check` exits2 solely for raw Cargo/test logs' extra blank line at EOF, including the copied independent failing log. `staged-whitespace-observation.txt` preserves those diagnostics; source/docs-only diff check and targeted rustfmt pass. Raw logs were deliberately not normalized.

## Outstanding gates and risks

**Fresh independent exact-candidate re-review and owner A1 acceptance pending; A1 NOT accepted; A2 blocked.** No reviewer/worker self-approval, integration or push. Owner retains sender acceptance.

Native windows/screenshots, capture-blur integration, OS AT (VoiceOver/UIA/NVDA/Orca), OS preferences, native fonts/shaping and physical DPI remain **UNVERIFIED**. Display-list goldens/headless AccessKit are not native optical/AT certification. Host must call cancellation on real capture loss/blur and use a canonical producer per physical activation. FIFO remains bounded with explicit backpressure; legacy Fired remains single-slot. No claim of complete usable component library or platform parity.
