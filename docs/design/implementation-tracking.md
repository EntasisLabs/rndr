# rndr_ui A0/A1 implementation checkpoint

Human approval: conversation entry 124 (2026-11-01) approved the council direction and whole A0 → A1 → A2 undertaking. This assignment implements A0+A1 only; A2 requires owner-led independent A1 API/cardinality review. The adjacent canonical proposals are byte-for-byte copies, including their historical held wording; this record, not an edited proposal, records approval.

Baseline: `291ab05493a59732f891dd762bf90c64e79a48d0`; isolated Forge worktree initially clean. Disk preflight: 5.2 GiB available; no existing worktree target cache. Builds use jobs=2, debug=0, incremental=false and offline resolution; reuse the resulting local target, no broad cleanup.

Full canonical read before source edits: Specification lines 1–339, Review Guide lines 1–70. Initial combined output was truncated; bounded subsequent reads recovered missing sections, especially 2/4/5/8/9. SHA-256:
- Specification: `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b`
- Review Guide: `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`

No pending work adoption; no Shell installation, app lifecycle, parsers, Blooey, native materials, publishing or integration. Native platform acceptance is not inferred from headless results.

## Verified completion of this bounded checkpoint

A0+A1 implementation is complete for independent review; the whole undertaking is NOT complete and A2 has not been started. Source commits, in order:
- `4976d58019365356d821bf262c7ac188fb686507` — unchanged approved proposals, approval/baseline record, initial reproduced failures.
- `eccc4b99bcaa5beb116b76d6f0c392d6abaf74af` — reusable additive core interaction/semantic prerequisites and API decision candidate.
- `4576494108c2a7276b5b99e1a998401b5e272b92` — reproduced mixed-input/readonly/lifecycle hardening.
- `27d36125ef70940bcf2782b3b5d0e292a1250ac5` — actual-display-list-driven nested wrapped-Hug fix and final readonly input boundary.
- `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1` — distinct A0 library, lifecycle/style/tokens, passive gallery and headless evidence.

Final verification: core 387 passed, paint 62 passed, UI 13 passed, actual AccessKit adapter 1 passed; no failures in final logs. Umbrella `rndr --all-targets --no-default-features` check passed. Six full-document gallery states each emit 1,147 actual quads including 1,135 glyph quads; 32-combination containment/glyph-completeness matrix passes. Historical failing reproductions are retained and explicitly named, not presented as current failures or hidden.

Canonical originals and portable copies were hash-checked again after implementation and still match the approved digests above (`evidence/ui-design-digests.txt`). Native matrix, source/evidence paths, exact commands, ownership/font limits and review gate are in `A0-A1-checkpoint.md`, `A1-api-decisions.md` and the crate README. Final cache 912 MiB, disk still 4.4 GiB free; no cleanup or permission expansion. The final documentation-only handoff commit follows these source commits. Owner retains independent acceptance, A2 continuation and integration.

## Recovery reconciliation — supersedes earlier final-run status

Recovery entry HEAD was `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1`; the one modified and four untracked handoff files were preserved and reconciled, not reset or discarded. Owner removed the inaccessible old-worker observation prerequisite. Native fenced execution succeeded; no distinct executor, lease denial or loss was observed. No raw HTTP/credentials/old-worker interruption was used.

The complete approved Specification (1–339) and Review Guide (1–70), API/cardinality decision record, tracking and checkpoint were reread. Canonical repository copies still match the two approved SHA-256 digests above. Initial fresh suites passed, then audit reproduced two additional lifecycle defects and fixed them separately:
- `fc6f622eec4411b103a33a45168103c02b2dde7e`: revalidate tab relation after host reparent/component removal on every use; no old-group activation/stale native relation/exposed orphan panel after synchronization.
- `d1610de48e4f249ad83788161d25368bdd935534`: require owned title Label before Card/Callout lifecycle updates; malformed handle rejected without partial recipe mutation.

Fresh corrected-source totals: **core 388, paint 62, UI 14, AccessKit adapter 1 passed**, zero failed. Umbrella all-targets/no-default-features and GPU-free normal dependency graph passed. Six regenerated actual display lists match preserved goldens byte-for-byte; 32-state containment/glyph matrix passed. Targeted rustfmt passed. Historical failing logs remain explicitly labelled. Reproduce with `sh evidence/recovery/run.sh`; archived log hashes are in `evidence/recovery/logs-sha256.txt`. Full baseline-to-candidate file inventory is `evidence/ui-changed-files.txt`.

Recovery disk stayed at 3.3 GiB free with existing local target 912 → 920 MiB; no broad cleanup. This is an **A0+A1 candidate ready for independent review**, not self-acceptance, A2 clearance or native integration. Native window/screenshots/AT/preference/font/DPI cells remain UNVERIFIED. Final documentation/evidence commit contains the reconciled original handoff files and no further source changes. Owner retains all acceptance, A2 and integration decisions.
