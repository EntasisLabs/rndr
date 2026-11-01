# rndr_ui A0/A1 implementation checkpoint

Human approval: conversation entry 124 (2026-11-01) approved the council direction and whole A0 → A1 → A2 undertaking. This assignment implements A0+A1 only; A2 requires owner-led independent A1 API/cardinality review. The adjacent canonical proposals are byte-for-byte copies, including their historical held wording; this record, not an edited proposal, records approval.

Baseline: `291ab05493a59732f891dd762bf90c64e79a48d0`; isolated Forge worktree initially clean. Disk preflight: 5.2 GiB available; no existing worktree target cache. Builds use jobs=2, debug=0, incremental=false and offline resolution; reuse the resulting local target, no broad cleanup.

Full canonical read before source edits: Specification lines 1–339, Review Guide lines 1–70. Initial combined output was truncated; bounded subsequent reads recovered missing sections, especially 2/4/5/8/9. SHA-256:
- Specification: `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b`
- Review Guide: `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`

No pending work adoption; no Shell installation, app lifecycle, parsers, Blooey, native materials, publishing or integration. Native platform acceptance is not inferred from headless results.

## Historical implementation verification (superseded by correction section below)

A0+A1 implementation is complete for independent review; the whole undertaking is NOT complete and A2 has not been started. Source commits, in order:
- `4976d58019365356d821bf262c7ac188fb686507` — unchanged approved proposals, approval/baseline record, initial reproduced failures.
- `eccc4b99bcaa5beb116b76d6f0c392d6abaf74af` — reusable additive core interaction/semantic prerequisites and API decision candidate.
- `4576494108c2a7276b5b99e1a998401b5e272b92` — reproduced mixed-input/readonly/lifecycle hardening.
- `27d36125ef70940bcf2782b3b5d0e292a1250ac5` — actual-display-list-driven nested wrapped-Hug fix and final readonly input boundary.
- `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1` — distinct A0 library, lifecycle/style/tokens, passive gallery and headless evidence.

Final verification: core 387 passed, paint 62 passed, UI 13 passed, actual AccessKit adapter 1 passed; no failures in final logs. Umbrella `rndr --all-targets --no-default-features` check passed. Six full-document gallery states each emit 1,147 actual quads including 1,135 glyph quads; 32-combination containment/glyph-completeness matrix passes. Historical failing reproductions are retained and explicitly named, not presented as current failures or hidden.

Canonical originals and portable copies were hash-checked again after implementation and still match the approved digests above (`evidence/ui-design-digests.txt`). Native matrix, source/evidence paths, exact commands, ownership/font limits and review gate are in `A0-A1-checkpoint.md`, `A1-api-decisions.md` and the crate README. Final cache 912 MiB, disk still 4.4 GiB free; no cleanup or permission expansion. The final documentation-only handoff commit follows these source commits. Owner retains independent acceptance, A2 continuation and integration.

## Historical recovery reconciliation — supersedes initial implementation run only

Recovery entry HEAD was `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1`; the one modified and four untracked handoff files were preserved and reconciled, not reset or discarded. Owner removed the inaccessible old-worker observation prerequisite. Native fenced execution succeeded; no distinct executor, lease denial or loss was observed. No raw HTTP/credentials/old-worker interruption was used.

The complete approved Specification (1–339) and Review Guide (1–70), API/cardinality decision record, tracking and checkpoint were reread. Canonical repository copies still match the two approved SHA-256 digests above. Initial fresh suites passed, then audit reproduced two additional lifecycle defects and fixed them separately:
- `fc6f622eec4411b103a33a45168103c02b2dde7e`: revalidate tab relation after host reparent/component removal on every use; no old-group activation/stale native relation/exposed orphan panel after synchronization.
- `d1610de48e4f249ad83788161d25368bdd935534`: require owned title Label before Card/Callout lifecycle updates; malformed handle rejected without partial recipe mutation.

Historical recovery totals: **core 388, paint 62, UI 14, AccessKit adapter 1 passed**, zero failed. Umbrella all-targets/no-default-features and GPU-free normal dependency graph passed. Six regenerated actual display lists matched preserved goldens byte-for-byte; 32-state containment/glyph matrix passed. Targeted rustfmt passed. Historical failing logs remain explicitly labelled. Do **not** run `evidence/recovery/run.sh` as-is: it overwrites archived logs. Archived hashes are in `evidence/recovery/logs-sha256.txt`; that historical candidate's baseline inventory is `evidence/ui-changed-files.txt`.

Recovery disk stayed at 3.3 GiB free with existing local target 912 → 920 MiB; no broad cleanup. That A0+A1 candidate subsequently received independent **REVISE** at `f7aa9703b7258e5ed546fc8436b3676f2705d9b0`; owner accepted the REVIEW deliverable only. Native window/screenshots/AT/preference/font/DPI cells remain UNVERIFIED.

## Bounded A1 corrections after independent REVISE — prior 01da candidate

Starting checkout was clean at exact `f7aa9703b7258e5ed546fc8436b3676f2705d9b0`, same worktree/undertaking. Full approved Specification, Review Guide, A1 decisions and checkpoints were reread; approved hashes above still match. Scope remains A0+A1, preserving all A0 and recovery fixes. No A2, approval, integration or push.

Corrections: preserve ordered activation edges through same-frame focus changes; resolve every Space up without stale unmatched arms; preserve hotkey ownership with cancellation-only markers, strip private edges before legacy delivery; rebuild the compatibility shim/text from the remaining unconsumed rich queue. Owner-directed FIFO64 now counts **requests**, preserving independent same-target keyboard/pointer/AT activations without inferred deduplication. `AlreadyPending` and `coalesced` are explicitly removed from the new, not-yet-accepted A1 APIs. Legacy public structs/press/Fired semantics are unchanged. Exact limits/scheduling are in `A1-api-decisions.md`.

Five initial regressions failed original production source before fixes. The final 14-test fixture produced **11 failures / 3 existing compatibility guardrails passing** against an actually rebuilt exact original snapshot; corrected source passes all 14. Existing lossy-contract expectations were revised with rationale, not suppressed. Evidence includes original standalone reviewer sources/logs, before/after receipts, source identities and retained invalid cached runs explicitly distinguished from verification.

Authoritative full run `evidence/a1-corrections-20261101/fresh-verified-run/`: **402 core + 62 paint + 14 UI + 1 AccessKit adapter tests passed**, zero failed/ignored; offline locked umbrella all-targets/no-default-features check and GPU-free normal dependency tree passed; six passive display lists byte-identical to archived goldens; original recovery/gallery hashes and archived-evidence diff passed; targeted rustfmt passed. All commands/statuses in that directory and its parent README. Cache collision during counterfactual builds was resolved by preserving/quarantining only the colliding core-library fingerprint, not broad cleanup or a second build target. Disk stayed **3.3 GiB free**; final target **964 MiB**.

Receipts/hashes for that prior correction are under `evidence/a1-corrections-20261101/`. Full correction inventory is `correction-changed-files.txt`; full baseline inventory is `baseline-changed-files.txt` there. Historical `evidence/ui-*` and `evidence/recovery/*` are unchanged. New run script refuses existing output directories. Owner retains independent exact-candidate re-review/acceptance and all A2/integration decisions; native windows/screenshots/OS AT/preferences/fonts/shaping/physical DPI remain **UNVERIFIED**.

## Second bounded A1 correction — current candidate, not accepted

Started clean at `01da8acdc57240cd536234d062933e232b033eec`. Read the complete approved Specification (1–339), Review Guide (1–70), decisions, checkpoint and tracking. No canonical proposal changes. Independent review `/tmp/rndr-rereview-01da.3eseqx/REVIEW.md` is accepted only as a review deliverable; implementation remains REVISE until fresh review/owner acceptance.

P2 repair in `hotkey/registry.rs`: private Space/Enter ownership now follows a matching registered chord or an active guard, not the mere presence of a registered base code. Classify before releasing the guard to preserve owned release cancellation across changed modifiers. No FIFO64/cardinality/API/scheduling/legacy behavior change; no A2.

Permanent new `a1_hotkey_ownership.rs` has 11 tests, with same/split-frame positive Space+Enter matrices, logical/physical keys, Alt/Ctrl registrations, matching release/chord and active-guard stale/synthetic/repeat cancellation, post-guard fresh activation, traversal/focus and legacy/editor controls. Identical fixture rebuilt against exact starting source via direct rustc: **5 fail / 6 pass**, exit101; corrected explicit rlib: **11 pass**, exit0. Full source hashes and actual candidate rustc invocation are recorded; no snapshot-cache counterfactual selection, no second Cargo target and no invalidation needed. All 14 earlier correction tests are byte-unchanged and pass.

Fresh final run: **413 core, 62 paint, 14 UI, 1 AccessKit adapter passed**, no failures/ignored. Original independent adversarial fixture **4 pass**; original probe/mixed outputs remain correct. Offline locked umbrella all-targets/no-default check, GPU-free normal dependency graph, six new byte-identical passive galleries, preserved prior-evidence hashes/diff and rustfmt pass. `evidence/a1-ownership-20261101-2t23Vj/` records exact commands/statuses, candidate/start identities, reviewer report/repro/log copies and SHA manifests. Original /tmp review, old invalid cached logs and other archives are untouched. One initial partial evidence run stopped on a wrong expected-count assertion in the harness; preserved separately, with unchanged fixture and corrected harness in the final run.

Existing target 969 MiB; disk 3.2 → 3.1 GiB free. Jobs=2, dev/test debug=0, incremental=0, offline --locked throughout. No network, broad cleanup, approval, integration or push. **Fresh independent re-review and owner A1 acceptance still pending; A2 blocked.** Native windows/screenshots/capture-blur integration/OS AT/preferences/native fonts-shaping/physical DPI remain **UNVERIFIED**. See the current checkpoint and new evidence README for inspected-versus-rerun distinctions.

## Owner-scoped A1 acceptance and A2 continuation — 2026-11-01

The owner assessed and accepted independent review `peer_assignment_3d5848681e54678ff513a9f9818f8f63cf960b17c5443863f7fb252a24cf093a`, terminal receipt `peer_terminal_dce00ddd1ee89eb962e54edb6cd799f7d0f64d6d04c0847a9c18d85cd045ed96`. The accepted exact A0/A1 checkpoint is **04642fe03f7603f228eaf9b83611641a95676e8d**, HEADLESS/core-contract scope, including the API/cardinality decisions and FIFO64 distinct requests. The owner explicitly lifted the A2 gate within the original human whole-implementation authorization. Historical pending/blocked sections above remain historical; this new section supersedes their gate status only.

This is **not** integration, native certification, A2 acceptance or whole-project acceptance. The owner retains fresh independent A2 review, corrections and native integration. No unrelated work is adopted. Entry worktree was verified clean at that exact SHA. Approved proposals remain unchanged (Specification c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b; Guide 8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0).

The original independent review directory `/tmp/rndr-independent-04642.hmZHsh/` is untouched. Durable small report, independent probe and manifests are copied without replacement into `evidence/a2-20261101-mZVyJv/accepted-a1-review/`. REVIEW.md retains SHA256 **b738135187bdbe6f215b090589836d5731f65b4b052f5986c45825b168eaadd2**. The bounded P3 fixture correction explicitly sets code=None in the logical-only branch and asserts shape distinction; no A1 production rewrite is made for that nit. Additional logical-only cases are promoted from independent.rs into a separate permanent fixture; original prior14 remains unchanged.

A2 implementation and verification results are recorded separately in `A2-components.md` and the new A2 evidence directory. All prior passive goldens, historical reports, expected-failure/invalid-cache logs remain preserved. Native windows/screenshots/capture-blur/OS AT/preferences/native fonts-shaping/physical DPI remain **UNVERIFIED** unless a later exact receipt says otherwise.

## A2 R1 continuation — actual text-content fit, unaccepted review candidate

Started clean at exact `92e98004d886a9ccfc3d2d05eaf090b591e10cc9` in the same authorized worktree. Full canonical Specification, Review Guide, A2 components, A1 API, checkpoint/tracking and fresh independent REVIEW/COMMANDS/probes were read, with bounded reads recovering combined-output truncation. Fresh review `/tmp/rndr-a2-fresh-review-4TfleE` is accepted as a review deliverable ONLY; A2 remains unaccepted. Its complete original manifest was verified, and exact compact review/probe/log copies and provenance are preserved in new `evidence/a2-r1-content-fit-20261101/`.

R1 continuation replaces label-box arithmetic with the existing core layout/text routines and public content-width metric on actual snapped tentative rows. Every surviving row meets the unwrapped-content contract. Monotone veto/revalidation and saved/restored scroll state preserve same-update reflow and caller ownership without core/API/dependency changes. See `A2-components.md` for contract and cost. Original32 UI tests remain byte-unchanged. Previous5 boundary stimuli were demonstrably incorrect (original fixture passes old UI, fails three boundary assertions on corrected UI); originals and failure evidence remain preserved. Corrected5 now inspect final text line/glyph geometry. The identical final new4 content/line-geometry tests pass 0/4 (four failures) on old exact UI and 4/4 on final directly rebuilt UI. A hidden-ancestor/malformed/scroll guardrail also caught an intermediate missing-rectangle panic; the failure and final conservative fallback are preserved.

Final direct-source UI41, reused identity-verified core411/paint62/adapter2, unchanged conformance13/adversarial5/geometry64 and unchanged fresh-review probes4 pass. Offline locked Cargo548 passes as supplemental/repeated coverage; exact overlap/count qualifications are in REPORT.md, not additive unique-test claims. Diagnostic175-case fractional survey changes13→0 printed mismatches; it is not175 assertion tests. Eight galleries are deterministic across two processes and byte-identical to starting92e98004; all six passive goldens and all historical evidence are preserved. Strict UI clippy and workspace formatting pass; all-target core clippy has two pre-existing test warnings, preserved without unrelated cleanup.

Build identity uses a new bounded direct-rustc output, not Cargo Fresh inference, after entire relevant unchanged source/dependency verification. Existing Cargo target only, offline --locked/jobs2/debug0/incremental0; no cleanup, network, native launch, approval, integration or push. Complete commands, reports, standard SHA256 manifest and source/blob/artifact identities are sealed with the tested source unchanged. Owner retains fresh independent review, A2 acceptance and integration; native windows/screenshots/OS AT/preferences/fonts/shaping/IME/physical DPI/platform remain **UNVERIFIED**.
