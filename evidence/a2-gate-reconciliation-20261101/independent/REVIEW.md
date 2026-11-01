# Independent A2 R1 content-fit re-review

## Verdict — ACCEPT for the scoped HEADLESS R1 correction

**No remaining P1/P2 correctness finding was reproduced in the requested toolbar content-fit/snapping correction. The previous independent P2 R1-continuation is resolved.** This is an independent review recommendation for owner assessment, not A2 acceptance, project acceptance, approval, integration or native certification. The owner retains sender review and all acceptance/integration decisions.

Reviewed exact clean candidate **f427b7474dad99a983115edfad14c5084c9cb253**, evidence-only child of tested implementation **91c008bdc1054f42f67dcd5d45729290ddb4ffa4**, starting **92e98004d886a9ccfc3d2d05eaf090b591e10cc9**. Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.

All review/build/probe files are in **/tmp/rndr-a2-rereview-OgFoQ8**. No repository source/document edits, Git index changes, commits, approvals, merges, pushes, integration, native launches, network/secret/production operations or cleanup were performed. Before/after Git status is empty. Builds used the existing Cargo target only; direct-rustc UI output is a new bounded directory, not a second Cargo target.

## Authority and independent provenance

Read complete canonical Specification (339 lines), Review Guide, A1 API decisions, A2 components, checkpoint and implementation tracking, including historical versus current gates. Initial combined reads were truncated; bounded reads recovered complete sections. Read complete prior independent `/tmp/rndr-a2-fresh-review-4TfleE/REVIEW.md` and COMMANDS.md, new committed REPORT.md/COMMANDS.md and build/reuse/probe scripts before adapting. Audited core App/layout/text routines, plugin schedule, controls, relevant tests and operating gallery/editor callers. Canonical hashes remain Specification `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b`, Guide `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`.

Independently checked all **693** committed candidate manifest rows; manifest SHA256 **ecb6ab2af600f0f453b87f686d3672bd4e43c82fb33ef748c409a1bdd985ca2a**. That integrity check is distinct from correctness: the review below uses fresh builds, unchanged negative controls and additional independent assertions.

## Why the general correction satisfies the reviewed contract

`crates/rndr_ui/src/controls.rs:840–979` retains the old conservative sum only as an additional veto. It does not use that arithmetic as proof of fit. After current control/style resolution, tentative rows are passed through the existing authoritative core layout and text-layout routines. Every direct known visible control must have finite allocated label width, actual core text-content width at least the engine's natural unwrapped width, and actual TextLayout with at most one line. Unknown/malformed children stack, rather than being adopted or hidden. Missing allocated geometry is conservatively rejected. There is no label-specific special case, epsilon, duplicated snapping/inset implementation or changed equal-share allocation.

This resolves both actual failures: the 1696-point integer reproduction has a label box of 826 but only 802 content points for the 826-point label; fractional snapping can reduce it further. The new decision checks the engine's actual allocation/content/lines, not the old label rectangle. Verification compares final TextLayout lines **and glyphs** against unwrapped engine layout. The independent probes do not just reimplement the production fit formula.

Within a measurement loop rows only become columns. Each changed pass removes at least one surviving candidate; with N tentative nonempty rows there are at most N+1 layout/text walks, followed by normal App layout. Every update starts anew, so both row→column and column→row remain possible. Unknown nested toolbar content keeps the outer group stacked; nested known rows are measured normally. Creation-order and repeated-update probes converged.

### App scheduling, caches and ownership

`crates/rndr_ui/src/lib.rs` installs control resolution, style cascade, then toolbar resolution in Schedule::Layout. `crates/rndr_core/src/app.rs:205–264` retains platform/input/update ownership, one text-engine begin_frame, final layout, existing list/markdown width refinement walks and final apply_text_layout. Extra toolbar measurement invokes geometry/text work, not input or Update systems. It does not dispatch host actions, drain intents, invoke begin_frame, replace Focus/Input, or mutate Buffer selection/history. Core layout updates Rect/Clip and Scroll geometry/clamps; apply_text_layout updates text geometry/cache. Engine cache use is real additional work, not a side-effect-free arithmetic query.

The resolver snapshots **all** Scroll components and restores full values before every tentative walk and on exit, so a rejected shorter row cannot irreversibly clamp caller scroll. App's final layout commits the real final clamp. A separate unrelated scroller with an extreme requested offset was checked against an additional authoritative final layout/text walk: Scroll, Rect, Clip, lines and glyphs were identical. Hidden-never-shown, hidden-after-shown, revealed ancestors, removed geometry, missing owned Layout/Label and nonfinite cached Rects were probed. Invalid nonfinite/negative/zero public widths reject without changing the live toolbar. External destructive edits remain unsupported by the documented ownership contract; no unrelated children were adopted.

Existing unchanged operating tests cover focus/actions, mixed Button/IconButton/SourceAction, FIFO cardinality/host validation, source intent, selection/history, mode panels and real Undo/Redo. A new independent test types into the real gallery editor, sets a selection, repeatedly changes width/type/density/fractional scale in the same update while reflowing another toolbar, validates focus and exact Buffer text/selection, checks the typed source-locator payload, and performs actual Ctrl-Z/Ctrl-Y through core input. All pass. No source/editor ownership was transferred to the UI library.

## Five expectation changes — justified, not weakened

All **32 original UI tests in 11 original files** are byte-identical to exact original A2 `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d` and pass against the fresh library. The later five-test allocation fixture changed, not those original32.

Independently compiled the unchanged starting allocation fixture against starting UI: **5/5 pass**. Against fresh final UI: **2 pass / 3 fail**, exit101, retained in `boundary-final/`. The old expected boundary omitted core's inner text-content inset, and a quarter-point below an arithmetic boundary can snap to the same allocated pixel. The corrected stimuli include the public core content metric and use below samples that actually lose an allocated pixel. Exact and above fits remain checked. Crucially, assertions now inspect final actual text content, lines and glyphs; they do not simply accept more rows or remove failing assertions. The permanent content fixture checks true integer boundaries 1742/1744/1746 and fractional origins/scales/type changes. Thus the expectation changes correct an invalid oracle rather than weakening the unwrapped-row requirement.

## Independent build identity — Cargo Fresh is not authority

Ran `validate-reuse.py` before reuse and again after verification. It validates the original independent review manifest (216 rows), **147 relevant unchanged non-UI source rows against hashes/current files/exact starting Git blobs**, **79 original binary inventory rows**, **3 original rlibs**, **5 explicit dependency rlibs**, and the starting fresh-review UI source/rlib inventory. The original UI rlib among the first3 is checked but not used as final UI. Row-level receipts and original inventories are retained in `reuse-validation-final.json`, `reuse-validation*.log`, and `reused-input-identities/`.

Freshly compiled the **entire current UI library, every UI test, and both headless examples** into the NEW `/tmp/rndr-a2-rereview-OgFoQ8/exact-ui` using explicit verified core/paint/dependency paths. No worker UI executable/log was substituted as independent verification. Direct UI source before/after inventories match. Full current source SHA inventories (234 files) and Git blob inventories match before/after and HEAD; full repository HEAD blob inventory is retained. Fresh built artifacts have runner inventories and `all-build-artifacts.sha256` (29 retained binary/rlib artifacts). The whole review has a standard relative SHA256 manifest.

Key SHA256 identities:

| Artifact | SHA256 | Classification |
|---|---|---|
| Fresh current UI rlib | `eed8660f1f578c7f57f3cf797c0c2cfe6ddeb43efb34b14e5f14d8e272e508d9` | Independently rebuilt here; byte-identical to worker artifact only as a comparison |
| Starting92 UI rlib | `67c2e5b4fe93182d92ff494753955541fc7ed1ae16235899e0db4b9b42e86494` | Verified reused negative-control UI, not rebuilt in this review |
| Unchanged core rlib | `6155aae82332f2c878291d72e492ea426d904f734d4c8f59ab01a87e206ae4fc` | Identity-verified reused |
| Unchanged paint rlib | `c2aea38975caa70ac72e1e38aa6695b99c110d0cf012191d14f1bcd2c68a9ab3` | Identity-verified reused |

The seal child has no crates/docs/manifest/lock changes from91c008bd. Core/paint sources are unchanged; their reruns are explicitly **not** called independent fresh rebuilds. Supplemental Cargo results can overlap and can use shared fingerprints; they are not the identity proof for current UI.

## Results and non-additive counting

| Check | Independently observed result | Count qualification |
|---|---:|---|
| Fresh full current UI | 41 pass | Original32 + corrected5 + permanent new4 |
| Verified reused core tests | 411 pass | Rerun here; not rebuilt |
| Verified reused paint tests | 62 pass | Rerun here; not rebuilt |
| Actual AccessKit adapter | 2 pass | Verified reused binaries rerun; not OS AT certification |
| Existing independent conformance | 13 pass | Verified reused probes rerun; not tracked-suite additions |
| Prior adversarial fixture | 5 pass | Unchanged fixture freshly compiled against current UI |
| Prior geometry fixture | 1 pass /64 configurations | Freshly compiled;64 configs are not64 tests |
| Unchanged fresh-review assertions | Baseline2 pass/2 fail → final4 pass | Freshly compiled on both verified UIs; same source |
| New permanent content fixture | Baseline0 pass/4 fail → final4 pass | Same final fixture both sides; runtime failures, not compile failures; final4 already in UI41 |
| Fractional diagnostic survey | 13→0 printed mismatches /175 configs | Four harness tests per side; not175 assertion tests or independent proof by exit0 |
| NEW independent adversarial fixture | 5 pass | New nested/sibling/order, hidden, scroll/cache, malformed, real editor/source tests |
| NEW structural/cost fixture | 1 pass /8 scenarios | Counts text-layout calls, not native timing |
| Cargo tests | 548 pass =524+24 | Supplemental overlapping executions; do not add548 to direct counts |
| Corrected allocation repeat | 5 pass | Repeats five already in UI41 |
| Operating gallery | Eight cases ×two generator processes | All17 files byte-identical to starting92, worker and fresh repeated run |
| Passive gallery | Six files identical | Preserved original passive goldens unchanged |
| Strict UI all-target clippy | Pass | `-D warnings` |
| Aggregate strict clippy | FAIL on two pre-existing core-test lints | Independently reproduced and retained; not a UI failure |
| Aggregate non-strict clippy | Pass with same two warnings | No suppression/cleanup |
| Workspace fmt, no-default/all-target check, native-gallery source check, dependency tree | Pass | Checks only; no native window |

Direct tracked suite executions are **516 =411+62+2+41**, with the reuse/rebuild distinction above. Cargo524 includes core/paint/UI plus10 doc tests; render24 includes the same2 adapter tests plus22 other render library tests. Repeated correction/content/adversarial runs, survey configurations, galleries and initial four-test v1 of the new adversarial fixture are not added as distinct coverage. `cargo-and-reuse-counts.json` retains per-log totals.

Expected failures preserved: baseline fresh-probes exit101 (2 failures), baseline permanent fixture exit101 (4 failures), original-boundary-final exit101 (3 obsolete assertions), and strict aggregate clippy exit101. All final corrected correctness suites pass. Worker historical failing/invalid-cache/intermediate logs were not overwritten or rerun into old output paths.

## Findings, observations and unresolved gates

**No remaining blocking scoped correctness finding.** Prior P2 R1-continuation is closed by the fresh unchanged repros, permanent geometry assertions and independent adversarial results.

**P3 / non-blocking performance-risk observation (not a newly proven correctness defect):** `crates/rndr_ui/src/controls.rs:917–977` performs whole-world layout/text passes every update with candidates, including unrelated labels. Repro: compile/run retained `cost-probe.rs` via `run-cost.sh` in a fresh review output. With100 unrelated labels and0/1/8/32 toolbar candidates, unrelated `TextEngine::layout` calls are100 with no candidates,200 with surviving rows,300 when rows are vetoed. All candidate counts in this probe stabilize in one/two tentative walks; this is not exponential and not one walk per sibling. Structural worst-case is N+1 global walks plus final normal App work. Layout/text caches are intentionally touched, Scroll is restored, no input/editor side effects were found. This cost is disclosed in the contract and is bounded, so it does not invalidate the scoped repair. Native responsiveness/frame-budget acceptance remains an owner/native gate; large documents or custom shaping engines may be more expensive. No native performance claim follows from these call counts or harness durations.

**Existing strict aggregate lint gate:** `crates/rndr_core/tests/a1_wrapped_hug.rs:79,125`, `needless_borrows_for_generic_args`; reproduce with exact strict command in COMMANDS.md. Same source exists unchanged at starting92; two diagnostics preserved. This is pre-existing and outside the UI correction, not grounds to hide a failed strict aggregate command. If owner requires workspace-wide strict lint cleanliness, that gate is still unresolved.

**Native/platform gates remain UNVERIFIED:** real windows/resize/screenshots, actual pointer capture/blur delivery, OS AT/VoiceOver/Narrator/UIA, preferences/forced colors/materials, native fonts/shaping, OS IME, physical DPI, packaging and macOS/Windows/Linux backend acceptance. Headless platform-enum and builtin-engine evidence do not certify these cells. No native launch was attempted.

## Preservation and resources

`audit.json` independently proves all **961** prior tracked evidence files unchanged from starting92, all234 current source files match HEAD blobs, original32 tests unchanged,17 operating files equal across starting/worker/two fresh runs, and six passive files equal original goldens. The committed693-row manifest also validates. Before/after source/blob/status fences match. Review-only output is about150MiB; existing target remains1.3GiB; free space about2.4GiB after review (preflight about2.6GiB). No second large target, broad cleanup or archived-output overwrite was needed. Cargo ran offline/locked/jobs2/dev+testdebug0/incremental0.

**Callback:** scoped ACCEPT recommendation returned to the owner for sender assessment and A2 acceptance/integration decisions. No project state was accepted or integrated by this reviewer.
