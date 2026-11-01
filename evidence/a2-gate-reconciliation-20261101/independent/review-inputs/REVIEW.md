# Fresh independent A2 correction re-review

## Verdict: REVISE — one P2 fit-contract defect remains

This is an independent reviewer recommendation, **not code acceptance**, native certification, approval or integration. Owner retains sender review, A2 acceptance and integration. I made no tracked source changes, commits, approvals, merges, pushes, launches or production changes.

- Exact reviewed clean candidate: `92e98004d886a9ccfc3d2d05eaf090b591e10cc9`.
- Exact implementation: `aba338dfa77a50cd92b6b182fe78074584a20841`.
- Full correction diff reviewed from `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d`, retained as `full-correction.diff`.
- Checkout: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
- Review output: `/tmp/rndr-a2-fresh-review-4TfleE` (outside tracked source).
- Clean HEAD/status before and after are retained. Seal delta contains exactly 457 evidence-only additions; implementation/test/docs content matches the reviewed implementation.

## P2 R1-continuation: predicate tests label rectangle, not actual text content width

**Paths/lines:** `crates/rndr_ui/src/controls.rs:880–892`, particularly `required = (natural + insets).max(theme.target_min())` at line 890; missing inner-text contract in `crates/rndr_core/src/font.rs:808–816`, used by label layout in `crates/rndr_core/src/layout.rs:735–748`. Boundary tests repeat the same incomplete model in `crates/rndr_ui/tests/a2_toolbar_allocation.rs:39–70`.

The equal-share repair fixes the original 1,000-point 826/478 mismatch: it now stacks that pair. It correctly accounts for the five nested **Layout** paddings, toolbar padding and gap. However, the label's text engine further subtracts `2 * TEXT_ORIGIN_X = 24` logical points from its rectangle before wrapping. That inset is not a Layout padding and is absent from `required`.

### Minimal independent reproduction

Use `fresh-probes.rs`, test `unwrapped_fit_must_include_text_engine_inner_inset`. Mount UI plugin, a 1,696-point parent and toolbar, then two ordinary buttons:

1. `This is a very long but valid command label with many words`
2. `Go`

Default comfortable theme, text scale 1, logical viewport scale 1. One `App::update()` gives:

```text
INTEGER_REPRO toolbar=1696 scale=1 type=1 axis=Row natural=826
label_rect=Rect { x: 9.0, y: 9.0, w: 826.0, h: 56.0 }
actual_inner=802 lines=2 height=40
```

Actual math: `(1696 - 8 gap)/2 = 844` per control. The nested ring/gap/border/face/label Layout paddings total 18 horizontally, so the label rectangle is 826. Core then lays text out at `826 - 24 = 802`, less than its measured unwrapped width 826. The toolbar nevertheless selects **Row**. The supplied exact-boundary regression asserts this row is fitted because it compares only `Rect.w` with natural width. The real `TextLayout.lines.len()` is 2, not 1. This persists over repeated updates; it is not a stale first-frame artifact.

This is a policy/geometry defect, not an assertion of lost text, unauthorized commands or native clipping. Core grows the wrapped row's height. But the canonical toolbar contract explicitly reserves Row for unwrapped labels fitting every allocation and requires stacking otherwise. Hence this is a P2 acceptance blocker for the requested fit correction, not a visual-preference nit.

**Additional allocation counterexample:** at logical viewport scale 1.75 with the same 1,696-point width, the predicate's unsnapped equal share also overstates the final snapped label rectangle: natural 826, actual label width 825.7143. The separate fractional probe fails before even subtracting the text inset. `fractional-survey.rs` exercises 175 width/type/logical-scale configurations without early exit and records all mismatches. Its passing diagnostic status is NOT acceptance evidence: it intentionally prints mismatches rather than asserting them. Physical DPI/platform behavior is not inferred from logical-scale probes.

**Requested revision:** make the row decision agree with the actual engine content width after all nested padding, text-origin insets and layout snapping. Do not fix only the original long-label width or weaken the contract to allow a wrapped row. Extend tests to assert actual text content/line geometry, not just repeat the predicate's arithmetic. Cover exact/below/above boundaries, fractional logical scales, both densities, type-scale changes and both reflow directions. No source implementation was attempted in this review.

## Independent verification actually rerun

All following executions were performed in this review, not copied from worker success logs:

| Coverage | My outcome | Interpretation |
|---|---|---|
| Offline locked Cargo library targets | 520 passed | Existing target, jobs 2/debug 0/incremental off |
| Offline locked Cargo no-default renderer targets | 24 passed | 2 adapter + 22 other renderer tests |
| Combined Cargo | **544 passed** | 522 comparable + 22 additional renderer; not 544 unique UI tests |
| Direct exact-source library coverage | **512 passed** | 411 core + 62 paint + 37 UI + 2 adapter; repeats Cargo coverage |
| Separate direct conformance probes | **13 passed** | Independently reran validated unchanged original binaries |
| Unchanged independent adversarial probe | **5 passed** | Recompiled against my newly rebuilt final UI |
| Unchanged independent geometry probe | **1 passed, 64 configurations** | Recompiled against my newly rebuilt final UI; not 64 tests |
| New permanent correction regressions on exact original UI | **0 passed / 5 failed**, exit 101 | Independently recompiled and reran against validated original independent UI rlib |
| Same five on newly rebuilt final UI | **5 passed** | These do not check the missing text inset |
| My final independent probes | **2 passed / 2 failed**, exit 101 | Integer inner-text failure + fractional snapped allocation failure; two failures support this one fit finding |
| My diagnostic fractional survey | 4 harness tests complete; 175 boundary configurations | Diagnostic printing, not a zero-defect assertion |

The two passing new probes check nonfinite/negative/too-narrow update rejection without geometry mutation, malformed label layout -> stale/stack behavior, conservative early stacking, focus retention, action identity, and non-destructive toolbar detach. The existing final regressions were inspected for same-update label/font/width changes both ways, comfortable/compact at 100%/200%, mixed icon/source labels, empty/single/hidden/unknown children and toolbar padding. Original A0/A1/A2 controls, editor ownership, nesting, target/name/action behavior are also exercised by the unchanged suites. No additional blocking safety or lifecycle defect was found in that coverage; this is not a claim of exhaustive malformed-world behavior.

The executed runners also passed formatting, all-target clippy with warnings denied, default root and no-default root Cargo checks (all offline and locked). Exact commands and coverage partitions are in `verify-rest.sh`, `verify-ui.sh`, their transcripts and `COMMANDS.md`.

## Build identity: no worker UI executable substitution

The shared Cargo fingerprint is not treated as source identity. I compiled **all final UI modules directly** in a new bounded temporary output, including UI unit/integration test binaries and gallery binary. No worker UI executable was used for the fresh final-UI verdict. I separately repeated the complete final UI rlib compilation after dependency/source verification; with the same archive basename it is byte-identical to the first independent build:

```text
67c2e5b4fe93182d92ff494753955541fc7ed1ae16235899e0db4b9b42e86494
```

This is `exact-ui/current/librndr_ui.rlib` and `confirmation/librndr_ui.rlib`. Every UI source was compared with the exact candidate's Git blob, and before/after UI and full-source SHA256 inventories agree. Rlib, binary, source and runner identities are retained, including final rechecks.

Reuse was limited to `/tmp/rndr-a2-independent-SQwoYo/exact` original independent artifacts after checking the original source/artifact receipts. Independently validated: **147 non-UI source/dependency manifest paths**, byte-equal to current candidate Git blobs, **79 recorded binaries**, **3 original current rlibs** (including baseline UI for the baseline failure run), and **5 external dependency rlibs**. Core/paint/adapter and conformance binaries were rerun, not rebuilt in this session. This is explicit reuse of previously independently built unchanged non-UI code, not a claim that I rebuilt core/paint today. The full original 404-entry manifest passes from its own directory; the worker's 456-entry manifest also passes. All three requested handoff hashes match exactly (`requested-identities.sha256`).

Honest retained harness failures:

- My first added inner-text harness referenced private/nonexistent APIs; compile failure, original source snapshot and log are preserved. The corrected public-API harness actually compiles and produces the failures above.
- A second rlib compiled with basename `librndr_ui-confirmed.rlib` differs in archive member names, not a source change. Archive-member inventories document the distinction. Rebuilding as `confirmation/librndr_ui.rlib` gives exact byte identity and successful `cmp`.
- First original-manifest check was run from repository root rather than the original manifest directory; path-open failures are retained. Correct-working-directory validation then passes all 404 entries. No history/logs were removed or silently replaced to hide these failures.

## Canonical contract and evidence audit

Read the complete canonical Specification, Review Guide, A2 components contract, A1 API decisions, A0/A1 checkpoint and implementation tracking; prior independent REVIEW; correction REPORT and COMMANDS; both runner scripts and relevant retained failure/pass logs before execution. Full correction diff and relevant core allocation/text-layout paths were inspected independently of the implementation report.

**Conservative veto is allowed.** The original sum budget (`natural + 52` per control plus gaps/minimums) may stack earlier than strictly necessary, provided the contract does not promise minimal stacking. Canonical responsive/adaptation guidance and A2's explicitly conservative policy do not promise maximal horizontal packing. Therefore retaining that veto is consistent with the canonical contract. It cannot replace the necessary per-share content-fit check or justify a false Row result. My small-label probe confirms intentional conservative stacking remains operable with correct focus/actions.

**Original tests/history preserved:** all original 32 UI tests and their containing tracked files are byte-identical to the pre-correction candidate. All **504 historical evidence blobs** are byte-identical. The seal is strictly **457 additions** under the correction evidence directory. I did not alter source, goldens or historical evidence to pass a comparison.

### Eight headless galleries and three changed states

I independently rebuilt/executed the final gallery twice: all eight states repeat byte-for-byte. Relative to historical A2 goldens, exactly six files change: display-list and semantics/geometry outputs for **light-wide**, **dark-wide-preview**, **high-dark-compact**. Five other states, inventory and all **six regenerated passive goldens** are unchanged.

The three changes are justified by the corrected per-share rule: the operating toolbar previously distributed five controls into small equal shares; now it stacks them into full-width controls. For light-wide/dark-wide-preview, e.g., first-control root width changes 189 -> 976 and its text rectangle 171 -> 958. Vertical siblings and following content move down; full-document headless height avoids viewport cropping. In all three states, access-node IDs, names, roles, values, focus/selected/enabled flags, ordering and children are identical after excluding only bounds (`three-gallery-audit.json`). Names/actions are not silently dropped. All remain usable targets in the supplied 64-state geometry assertions. That justifies these saved gallery differences; it does **not** establish correct fit at other widths, as my 1,696-point case demonstrates. Evidence is textual headless paint/layout output, not native screenshots.

## Limitations and resources

No native launch, native windows/screenshots/capture, native blur/materials, OS assistive technology, OS preference following, native fonts/fallback/shaping, IME, physical DPI or other platform execution was verified. Headless logical tests do not certify those capabilities. No network, secrets, real documents, production changes, cleanup or integration.

Preflight approximately 3.1 GiB free; final approximately 3.0 GiB free, existing Cargo target still approximately 1.3 GiB. New review output is bounded (see `resource-final.txt`), not a second Cargo target. All Cargo runs were offline/locked with the existing target; jobs 2/debug 0/incremental off. No broad cleanup/removal performed.

**Disposition:** original gross 826/478 bug is corrected, but A2 should remain pending revision because the stronger promised actual-content-fit outcome is not yet met. Owner should assess this independent reproduction and request a correction/re-review. This report itself grants no acceptance or integration authority.
