# A2 R1 continuation — snapped text-content fit correction

## Result and exact scope

**Implemented and verified; ready for owner-arranged fresh independent review. A2 is NOT accepted.** No approval, merge, integration, push, native launch or whole-project acceptance was performed.

- Clean entry: `92e98004d886a9ccfc3d2d05eaf090b591e10cc9`.
- Tested implementation commit: **`91c008bdc1054f42f67dcd5d45729290ddb4ffa4`**.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
- The subsequent evidence-only child commit contains this report/seal; it does not change tested source. Obtain its exact SHA from the delivered HEAD/Git log.
- Full approved Specification/Review Guide, A2 components, A1 API, checkpoint/tracking and fresh independent REVIEW/COMMANDS/probes/survey/logs were read. Combined-output truncation was recovered with bounded reads. Canonical Specification/Review Guide are unchanged.

Five changed files: UI `controls.rs`, corrected `a2_toolbar_allocation.rs`, new `a2_toolbar_content_fit.rs`, current A2 contract and tracking. **No core/API/dependency/lockfile change.** All 32 original UI tests are byte-unchanged. Owner retains orchestration, sender review, A2 acceptance and integration.

## Review provenance and preserved failures

Original independent directory `/tmp/rndr-a2-fresh-review-4TfleE` is untouched. Before reuse/copy and again after verification, all **216/216** original manifest rows were checked against original bytes. Exact requested identities:

- REVIEW SHA256 `d617301815a616cc3ea53f5b688e9f75debe5b3495a6d82d46837d85ecceb95d`.
- COMMANDS SHA256 `b4eb7d0bb062a23233c1007f23206d217e1501407a64758bc29ac676e56f66fb`.
- Original MANIFEST SHA256 `85ff2e5f84ad365c93a336095f902d9e04f8f465fc3e1c9c0ef4b50707e4e039`.

`independent-review/` preserves exact compact report/probes/survey/runners/logs/identity inventories with provenance. Its copied **original** manifests still describe original artifacts (some binary artifacts deliberately are not copied); validate those manifests at the original directory, not as a claim that every original binary exists in the compact copy. This directory's own top-level standard manifest covers every copied file.

The review was accepted as a **review deliverable only**, not implementation acceptance. Its R1 continuation is the correction target. Old historical evidence, old invalid-cache/failing logs and original six passive goldens were not rewritten. `history-before.sha256` and `preservation-audit.json` prove all **961** previously tracked evidence files unchanged.

## General correction, not an epsilon or repro special case

The old decision checked a speculative equal-share label rectangle. At width1696/defaulttheme/type1/scale1, LONG measures826; a label rectangle826 has text content802 after core's24-point inner inset and produces two `TextLayout` lines. At logicalscale1.75, snapping already reduces the label rectangle to825.7143 before that inset. Nested layout padding and text content are different constraints.

After the current UI style/type cascade, the resolver now:

1. Retains the historical conservative sum as an **additional veto**, not a fit proof. Known visible controls that pass it become tentative rows; unknown/malformed visible content stacks. Hidden direct children are excluded; empty behavior is unchanged.
2. Runs **core's existing** `layout_propagate` and `apply_text_layout`. It inspects actual snapped label geometry and uses the public authoritative `field_text_width` with the installed engine's `unit_advance`.
3. Keeps a row only when **every** label has finite allocated width, actual content width at least its unwrapped natural width and actual text layout of at most one line. There is no new inset/snapping constant, guessed epsilon, changed allocation policy, hidden action or hardcoded label.
4. Vetoes to Column monotonically and reruns measurement if any veto could change ancestor geometry. With N tentative nonempty rows, at most N+1 measurement walks occur before App's ordinary final layout. Usually one or two suffice. Every update starts afresh, permitting same-update row→stack and stack→row on width/type/label changes.
5. Saves/restores complete caller Scroll state before each tentative walk and after resolution. A rejected short row must not prematurely clamp a previously valid scroll offset. App's final layout alone commits the final clamp.
6. Treats missing geometry conservatively as not proven to fit. A never-shown hidden ancestor has no Rect; reveal is reconsidered using actual geometry within that update. This was independently exercised during this repair: the intermediate implementation's unwrap panicked, the added guardrail caught it, and the final fallback passes.

No parallel layout engine, input/focus manager, event dispatch, intent drain or editor mutation was added. Existing core routines compute geometry/text caches, and App's final walk refreshes them. The tradeoff is additional whole-world layout/text measurement walks, bounded above; no native performance claim or benchmark is made. Avoiding core/API changes also avoids an unnecessary core/dependent rebuild under tight disk conditions.

## Permanent regressions and true negative controls

**Authoritative final negative-control pair:** `fresh-baseline-final/` vs `fresh-final-final/`, linked to verified starting UI vs freshly direct-built final UI. `inputs.sha256` proves identical final fixture/probe bytes on both sides (only UI rlib differs).

| Suite / purpose | Exact starting UI | Final exact UI | Evidence / classification |
|---|---:|---:|---|
| New permanent content-fit fixture | **0 pass / 4 fail**, exit101 | **4 pass**, exit0 | `content-tests.log`; true runtime failures, not a compile failure |
| Unchanged independent fresh probes | **2 pass / 2 fail**, exit101 | **4 pass**, exit0 | Integer inner-content and fractional snapping failures both reproduced unchanged |
| Unchanged diagnostic fractional survey | 175 configs, **13** printed `MISMATCH` | 175 configs, **0** printed `MISMATCH` | Diagnostic survey, **not175 assertion tests**; its harness reports four tests on each side and is not added as unique coverage |

The four permanent tests inspect final **actual `TextLayout` content/line/glyph geometry** against the engine's unwrapped line/glyph layout, not a predicate that merely repeats production arithmetic. Coverage:

- Integer repro plus exact/below/above true pixel boundaries (1742/1744/1746), both densities, repeated both directions.
- Logical scales1/1.25/1.5/1.75/2; type1/1.1/1.25/1.5/2; comfortable/compact; fractional ancestor/toolbar padding and origins; width sweeps around old and true boundaries with final actual geometry assertions on surviving rows (50 base configurations, many updates, not thousands of separately counted tests).
- Same-update label/type/width transitions in both directions; canonical focus/identity/actions preserved.
- Never-shown hidden ancestor, reveal, malformed owned label, narrow stacked content, preservation of scroll offset and return to a wide unwrapped row.

The earlier five correction tests remain five, with stronger actual content/line/glyph assertions. They cover unequal labels, mixed Button/IconButton/SourceAction, 100%/200% type, densities, width/type/label changes both ways, focus/names/actions, empty/single/hidden/unknown content, narrow geometry and toolbar padding. Original malformed-control tests also remain unchanged.

### Why the five-test boundary expectations changed

The historical boundary helper omitted core's inner text content inset. The new helper derives the test stimulus's text inset through public `field_text_width`; **correctness is asserted from final actual layout**. The old “boundary−0.25 must stack” assertion was also invalid at integer scale because that amount can round **up** to the same snapped pixel allocation. Below samples now lose one allocated point per control (2 points for two children,3 for three); exact and above remain checked. Fractional tests inspect actual final geometry rather than assuming raw unsnapped arithmetic is authoritative.

`original-allocation.rs` is the exact starting fixture. Rebuilt unchanged against old UI it passes5/5; against final exact UI it passes2/fails3 at those demonstrably wrong boundary assertions (`original-boundary-final-exact/run.log`, exit101). This is an expected historical-assertion failure, **not a failing corrected final suite**. The prior original baseline failures in old sealed R1 evidence also remain untouched. No expectation was silently weakened to accept wrapped rows.

## Final verification and non-additive counts

Authoritative final outputs: `exact-ui-final/`, `rest-final/`, `fresh-baseline-final/`, `fresh-final-final/`. `suite-counts-final.json` lists every assertion-suite receipt. No failed/ignored tests in final corrected assertion suites.

| Verification | Result | Identity / overlap qualification |
|---|---:|---|
| Direct rebuilt current UI | **41 pass** | Original32 + corrected5 + new4; complete current library, tests and headless examples built directly from source |
| Unchanged core binaries rerun | **411 pass** | Identity-verified original exact binaries; **not** reported as newly rebuilt core |
| Unchanged paint binaries rerun | **62 pass** | Same qualification |
| Actual AccessKit adapter rerun | **2 pass** | Actual adapter tests, not OS AT certification |
| Existing independent conformance probes rerun | **13 pass** | Same verified binaries/sources as original independent review; separate from tracked test suite |
| Prior adversarial fixture | **5 pass** | Unchanged source recompiled against final exact UI |
| Prior geometry fixture | **1 pass over64 configurations** | Unchanged source recompiled;64 configs are not64 tests |
| Unchanged fresh independent probes | **4 pass** | Newly compiled against final exact UI; negative control above |
| Cargo core/paint/UI tests + render library tests | **548 pass** (524+24) | Supplemental/repeated: includes the above tracked tests,10 doc tests and22 other render tests. Do **not** add548 to the direct counts as unique tests |
| Explicit final correction repeat | **5 pass** | Repeats corrected5 already in UI41; not5 more unique tests |
| All-target umbrella/no-default check | Pass | Source/build compatibility, not native launch |
| Native gallery source check with accesskit | Pass | `cargo check` only, no window |
| GPU-free normal dependency graph | Pass | Existing core-only normal UI graph |
| Workspace formatting + targeted formatting | Pass | Final exact source |
| Strict UI all-target clippy | Pass | `-D warnings` |
| All-target core/paint/UI clippy | Pass with2 pre-existing core-test warnings | Strict aggregate attempt failed on unchanged `a1_wrapped_hug.rs` lines79/125 (`needless_borrows_for_generic_args`). Failure and normal warning logs preserved; no unrelated cleanup |

Direct tracked suite total411+62+2+41=**516 executions**, with explicit reuse/rebuild distinctions above. Initial/intermediate full runs, repeat probes, repeat galleries and Cargo are **repeated executions, not additional unique tests**. The diagnostic survey is not a correctness proof by its exit code; the real unchanged assertions and actual-layout permanent tests supply that proof.

### Intermediate/failed attempts retained, not final evidence

The first seal preflight also caught an evidence-copy omission: original standard manifests exclude themselves, so the original MANIFEST needed an explicit exact copy in `independent-review/`. This failed preflight created no seal; its record is preserved, and final preflight validates the original216 rows/hash and exact copy before sealing. No tested source changed.

- Initial content-fixture compile and baseline0/4 pass logs are retained; final expanded fixture's authoritative baseline/final pair is named above.
- `exact-ui/` stops because a test attempted to import core's private text-origin constant. The test was corrected to use the public content metric; this compilation failure is not a negative-control success.
- `rest/` stops on the aggregate strict pre-existing core test lint warnings; `rest-v2/` completes with correct warning qualification.
- `initial-final/`, `exact-ui-v2/`, `rest-v2/`, `fresh-baseline/`, `fresh-final/` are superseded by the final guardrail version, not final source certification.
- `intermediate-hidden-failure/` shows3/4 pass and the hidden-ancestor missing-Rect panic. `superseded-controls-before-hidden-guard.rs` is exact SHA256 `e1b3da0f9aea4b29e056de2dc991a26994969b9ca1470efc6d0869f5d4886142`; `superseded-content-fit-initial.rs` is exact SHA256 `dddfc01e02d94a78de74e18a19b3c72535ac7acdf9166b3cecc20c04a63847b3`. These snapshots were recovered by exact reverse substitutions and verified against already recorded hashes, not invented replacements.
- Earlier `original-boundary-final-ui/` refers to the intermediate v2 UI; final exact repetition is explicitly `original-boundary-final-exact/`.

## Build identity, budgets and artifact linkage

**Cargo Fresh is not the identity authority.** Shared fingerprints can collide. Before direct reuse, all147 relevant non-UI source rows were checked against original hashes, actual current files and exact starting Git blobs. The79 original binary inventory entries,3 original rlibs and5 explicit dependency rlibs were hash-verified. Original3 includes an old UI rlib that was verified but **not** used as final UI. The fresh review's22 UI source rows were checked against exact starting Git and its newer3-rlib inventory checked, identifying the actual starting UI negative-control artifact. Full inventories and row-by-row checks are in `reused-input-identities/` and `reuse-validation-final.json`.

All changed code was rebuilt in **new bounded direct-rustc output** under `/tmp/rndr-a2-content-fit-UJYjbC/exact-ui-final`, with explicit core/paint/dependency paths, freshly compiled entire UI library, all UI tests and both headless examples. No stale UI artifact was called rebuilt. Core/paint did not change; their validated binaries/rlibs were reused and labelled as such. Fresh probes and prior adversarial/geometry fixtures were newly compiled against the final rlib.

Key SHA256:

- Starting UI: `67c2e5b4fe93182d92ff494753955541fc7ed1ae16235899e0db4b9b42e86494`.
- Final rebuilt UI: **`eed8660f1f578c7f57f3cf797c0c2cfe6ddeb43efb34b14e5f14d8e272e508d9`**.
- Unchanged reused core: `6155aae82332f2c878291d72e492ea426d904f734d4c8f59ab01a87e206ae4fc`.
- Unchanged reused paint: `c2aea38975caa70ac72e1e38aa6695b99c110d0cf012191d14f1bcd2c68a9ab3`.

`source-before.sha256`, the direct runner's before/after full UI source hashes, `changed-source-blob-identities.json`, full before/after relevant Git blob inventories, explicit library receipts, probe input manifests and `all-build-artifacts.sha256` link exact source to retained artifacts.66 built artifacts from all attempts are hashed, including failed/superseded test binaries. Binaries remain at their explicit temporary locations; compact logs/runners/identities/galleries are committed here. The historical source/binary receipts record their actual paths, including spaces.

Disk preflight:3.0GiB free, existing target1.3GiB. Final full rerun preflight2.74GiB; end approximately2.6GiB free, target still1.3GiB; bounded temp evidence/builds approximately332MiB. All Cargo commands use existing target, offline `--locked`, jobs2, dev/testdebug0, incremental0. No second large Cargo target, broad cleanup/removal, network, secrets or production operations. New reproducible runner stops below its2GiB safety floor rather than cleaning unrelated data.

## Gallery and ownership reconciliation

Eight operating gallery cases each ran in **two independent generator processes**, and each generator also asserts identical repeated paints. All17 files (16 display/semantic-geometry files plus inventory) are byte-identical between processes and to the fresh independent exact starting92e98004 gallery. **No new gallery geometry change** was introduced by this continuation. The earlier established R1 gallery differences remain historical, not concealed by replacing their files. `gallery-reconciliation.json` records per-case identities and access/label counts. Six regenerated passive display lists compare byte-for-byte against preserved passive goldens.

Actual gallery semantics/targets/focus+selection/editor status are therefore unchanged; original32 operating/lifecycle/paint/editor/source-adoption tests also pass unchanged. Mode switching, editor identity/selection/history/Undo/Redo remain caller/core-owned; canonical focus/actions, source locators and host validation were not rewritten. The new regression additionally checks focus/action cardinality and scroll preservation through reflow.

## Gate and limitations

This is a scoped headless correction result, **not self-approval**. Next stage is fresh independent review arranged by the owner, followed by owner A2 acceptance/integration decisions. Prior accepted review deliverables do not accept A2.

Native windows/screenshots, actual OS pointer-capture/blur delivery, OS AT/VoiceOver/Narrator, preferences/forced colors/materials, native fonts/shaping, OS IME, physical DPI, packaging and macOS/Windows/Linux platform acceptance remain **UNVERIFIED**. No native launch was attempted. Headless platform-enum/core/editor tests do not certify those native cells. Additional whole-world measurement work is bounded but native performance remains unmeasured.
