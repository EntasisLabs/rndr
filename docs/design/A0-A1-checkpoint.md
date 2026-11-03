# A0+A1 checkpoint — historical independent A1 review boundary

> Historical checkpoint, not current acceptance status. See [current UI status](../ui-status.md) for the integrated `fe9e679` baseline and remaining native limits. Exact historical scope/results below are preserved.

This delivers **A0+A1 only** within the user-approved whole A0→A1→A2 undertaking. Approval is human conversation entry **124**, 2026-11-01. Owner retains orchestration, independent A1 API/cardinality review, A2 continuation, native verification and integration. No approval/integration/push was performed here.

The adjacent canonical Specification and Review Guide remain byte-for-byte unchanged proposals (including historical held wording). Their full reads/digests and baseline are recorded separately in `implementation-tracking.md`; approval is not retroactively written into either proposal. Initial isolated worktree was clean at `291ab05493a59732f891dd762bf90c64e79a48d0`. No pending work or caller edits were adopted.

## A0 checklist

- [x] Additive opt-in `rndr_ui`, with normal dependency only on GPU-free `rndr_core`; `rndr_paint` is dev/gallery evidence only. No new renderer, app/window lifecycle, manager/dispatcher or runtime.
- [x] Semantic Light/Dark and explicit high-contrast palettes; surface/text/accent/hover/pressed/selected/focus/stroke/feedback pairs; tested WCAG pair ratios (text ≥4.5:1, essential feedback/focus boundaries ≥3:1). Decorative subtle separators are not claimed to be essential control boundaries.
- [x] Density independent of UI and reading text scales; type roles, focus width+gap, corners, spacing, target-min tokens and adjustable passive reading measure.
- [x] Reversible per-property style ownership; explicit primitive and alpha-zero precedence; app override versus recipe separation; theme updates without reconstruction; legacy default Theme/appearance unchanged for unstyled nodes.
- [x] Passive Surface/Card/Callout, meaningful textual callout prefixes, typography, decorative separator and reading layout recipes. Stable keyed mount/update, content slots, adoption/borrowing, destruction, stale-handle rejection and targeted state cleanup.
- [x] Passive headless full-document gallery (no A2 operating controls). Actual paint fills/glyph UVs/clips/rectangles saved for six representative states, with golden comparison.
- [x] 32 state combinations (Light/Dark × Standard/High contrast × Comfortable/Compact × 100%/200% text × 280/800pt width), visible Hug containment and complete ASCII glyph coverage; actual glyph scale and fractional/2× logical geometry, nested scroll scissors, lifecycle and content retention tests.

## A1 checklist

- [x] Necessary core findings reproduced before fixing. Baseline folded-label/unsafe SetValue/tab-action failures retained; later lifecycle, release-edge and real wrapped-layout failures retained separately.
- [x] Generic focus without Shell/file/quit behavior; core Focus/Input/World remain authoritative; repeat/synthetic/modifier handling and editor CapturesKeys ownership preserved.
- [x] Per-control `ReleaseActivation` opt-in; unchanged unmarked legacy press/Enter/Fired path in mixed windows; modern pointer/keyboard/AT use canonical `try_activate` and do not fan out to Fired.
- [x] Explicit cancellation and ownership: matching primary pointer, release-outside, Escape, blur, focus loss, disable/readonly/hidden, scroll capture, explicit capture loss, destroy/stale generation. Ordered control edges survive pre-frame focus filtering until dispatch; registered-hotkey-owned edges only cancel, and private release/synthetic edges are stripped before legacy delivery.
- [x] **Owner-directed correction after independent REVISE:** bounded FIFO64 **requests**, preserving distinct same-target keyboard/pointer/AT activations before drain. Observable Accepted/Overflow/Rejected, explicit overflow counter, no eviction/coalescing/modern-to-Fired fanout. New unaccepted A1 AlreadyPending/coalesced spellings removed; legacy public structs unchanged. See current `A1-api-decisions.md`; independent exact-candidate re-review remains required before A2.
- [x] Explicit names/descriptions/readonly and shared allowed-action matrix; folded visible button labels; placeholder/value not used as persistent names. Core and actual AccessKit role/action/relation tests.
- [x] AT SetValue explicitly returns UnsupportedSetValue and is not advertised. No raw Buffer/Label mutation; editor/history/IME/source-edit regression suite retained. Opted-in readonly user input is guarded; programmatic validated source-edit authority is unchanged.
- [x] Real semantic tablist/tab/panel prerequisite API with checked links, focus distinct from selection, manual focus-move API, activation-driven panel selection, inactive exclusion, native controls/labelled_by and TabPanel role, stale/orphan handling. This is not the A2 mode-selector widget or its roving/orientation/overflow UI.
- [x] Idempotent UiPlugin/GenericInteraction/Chrome installation, no duplicate bindings/systems or queue reset. No parallel rndr_app lifecycle.
- [x] A proven nested wrapped-Hug clipping fix kept in a separate reusable-core commit. Existing Fill/min-size rules are reused; no library-side text-height calculator.
- [ ] Independent A1 review/acceptance by owner/reviewer — **required before A2**.

## Initial implementation commands / historical results

All commands ran offline inside the isolated worktree with the same bounded local cache:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --offline -p rndr_core
cargo test --offline -p rndr_paint
cargo run --offline -p rndr_ui --example passive_gallery -- evidence/ui-gallery
cargo test --offline -p rndr_ui
cargo test --offline -p rndr_render --no-default-features --features accesskit --lib a11y::tests
cargo tree --offline -p rndr_ui --edges normal
cargo check --offline -p rndr --all-targets --no-default-features
```

| Receipt | Final result |
|---|---|
| `evidence/ui-core-suite.log` | 387 tests passed in aggregate, including doc tests; no failures |
| `evidence/ui-paint-suite.log` | 62 tests passed in aggregate; no failures |
| `evidence/ui-foundations.log` | 13 tests passed; no failures (matrix iterations occur inside tests) |
| `evidence/ui-accesskit.log` | 1 real AccessKit adapter test passed; no failures |
| `evidence/ui-umbrella-check.log` | `rndr` all-targets, no-default-features compatibility check passed |
| `evidence/ui-normal-dependencies.log` | normal dependency graph is rndr_ui → rndr_core; no GPU/runtime dependency added |
| `evidence/ui-gallery-run.log`, `evidence/ui-gallery/*.txt` | 6 full-document display lists, each 1,147 quads including 1,135 glyph quads and 26 Label entities |

Failure logs named `*-reproduced.log` / `ui-a1-baseline.log` intentionally record expected historical failures, not outstanding failed final builds. Fixed and final logs distinguish the outcomes. Golden files are display-list evidence, **not native screenshots or optical/native certification**. The locale fixture retains exact strings; bitmap fallback does not certify their shaping.

Historical first-run disk preflight had 5.2 GiB available. That run ended with a 912 MiB bounded cache and 4.4 GiB free; these are not the recovery disk readings. No cleanup outside the worktree, network fetch, publish/push/deploy, spending, secrets or integration was used. Optional feature sets/native target cross-builds beyond the commands above were not certified.

## Recovery verification — historical pre-correction review candidate

Recovery began at preserved HEAD `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1`. The modified tracking record and four untracked checkpoint/evidence files were inventoried and retained, then reconciled here. Supported native admission succeeded with exclusive execution; no extra old-worker raw observation prerequisite was imposed, and no credentials/raw HTTP/re-interruption/fencing bypass was used.

Additional separately reviewable source fixes:
- `fc6f622eec4411b103a33a45168103c02b2dde7e` — semantic tab relations revalidated after host reparent/component removal; stale advertising/activation/panel exposure reproduced before fixing.
- `d1610de48e4f249ad83788161d25368bdd935534` — malformed owned title Label rejected before any Card/Callout recipe mutation; failure reproduced before fixing.

That recovery's final documentation/evidence commit (`f7aa9703b7258e5ed546fc8436b3676f2705d9b0`) contained no further source changes; its tested source tree was `d1610de48e4f249ad83788161d25368bdd935534`. It subsequently received independent REVISE. All prior five source commits remain ancestors, listed in `evidence/ui-code-commits.txt`. New bounded corrections/evidence are separate; this historical run does not certify them.

Run from the worktree root:

```sh
# Historical reproduction only: run.sh overwrites archived logs.
# Do not run as-is when preserving evidence; use the new correction run script instead.
shasum -a 256 -c evidence/recovery/logs-sha256.txt
shasum -a 256 -c evidence/ui-gallery-sha256.txt
```

That historical script used offline **locked** resolution, jobs=2, debug=0, incremental=false and the existing local target cache. Saved hashes identify its archived run, not the correction candidate. The new correction script requires a fresh output directory; preserved logs must not be overwritten.

| Fresh recovery receipt | Result at corrected source candidate |
|---|---|
| `evidence/recovery/core.log` | **388 passed**, zero failed, including existing editor/IME/history/source-edit regressions and new tab lifecycle test |
| `evidence/recovery/paint.log` | **62 passed**, zero failed |
| `evidence/recovery/ui.log` | **14 passed**, zero failed; theme/contrast/ownership/stale handles, full 32-state geometry/glyph matrix and new malformed-title test |
| `evidence/recovery/accesskit.log` | **1 passed**, zero failed; actual adapter test, not native AT execution |
| `evidence/recovery/umbrella.log` | all-targets no-default-features compatibility check passed |
| `evidence/recovery/tree.log` | normal UI graph remains GPU-free, UI → core |
| `evidence/recovery/gallery.log`, `gallery-comparison.log` | all six regenerated display lists byte-identical to preserved `evidence/ui-gallery`; empty comparison log denotes no differences |
| `evidence/recovery/format.log` | rustfmt check on the five recovery source/test files passed; empty log denotes no diagnostics |
| `evidence/recovery/*-reproduced.log`, `*-fixed.log` | two new expected historical failures and their passing focused reruns, clearly separated from final suite logs |

Recovery disk preflight and final reading: **3.3 GiB free**; reused target grew from 912 to **920 MiB**. No broad cleanup or cache deletion. Regenerated gallery copies are under ignored `target/ui-recovery-gallery`, not an extra committed duplicate. Approved design digests and all six preserved golden hashes still match. No native execution/screenshots were performed.

## Bounded A1 correction verification — prior 01da candidate (independently REVISED)

Starting SHA `f7aa9703b7258e5ed546fc8436b3676f2705d9b0` received independent REVISE; owner accepted the REVIEW deliverable only. This correction preserves A0 and recovery fixes, and does not start A2 or perform acceptance/integration/push.

- [x] Same-frame no-focus Tab→Space down/up focuses and activates once; subsequent unmatched Space up cannot commit a stale arm. Legacy→modern and pointer-induced focus transitions, modifiers, hotkey cancellation, focus/capture loss, blur/cancel and editor ownership covered.
- [x] Same-frame Tab→legacy Enter preserves Fired; unconsumed rich event order and first-press shim/text are reconstructed without processing both representations. Field/editor navigation/editing guardrails retained.
- [x] Distinct Enter presses, AT requests and mixed-source requests preserve multiplicity before drain. Same-target capacity64/explicit overflow, FIFO survivor order and disabled/readonly/stale/hidden/action-name drain validation covered. No inferred deduplication or modern→legacy fanout.
- [x] Initial five regressions failed original production source before changes. Final identical 14-test fixture: **11 fail / 3 guardrails pass** on rebuilt exact original source, **14 pass** corrected. Full current run: **402 core, 62 paint, 14 UI, 1 AccessKit adapter passed**, zero failed/ignored.
- [x] Offline locked umbrella check, GPU-free normal dependency assertion, six byte-identical passive galleries, preserved original gallery/recovery hashes and targeted rustfmt passed. Disk remains **3.3 GiB** free, existing target **964 MiB**. Only a colliding core-library fingerprint was quarantined/preserved during counterfactual artifact validation; no broad cleanup or second build target.
- [ ] Owner-led independent re-review/acceptance of the exact correction candidate; A2 remains blocked on that gate.

Exact commands, statuses, before/after source identities, cache-collision caveats, new evidence hashes and both full inventories are in `evidence/a1-corrections-20261101/README.md`. Authoritative full logs are `fresh-verified-run/` there, not the earlier invalid cache-selection attempts. `A1-api-decisions.md` defines the revised contract and limits. Old archives are unchanged. Native windows/screenshots/OS AT/preferences/fonts/shaping/physical DPI remain **UNVERIFIED**.

## Second bounded A1 correction — nonmatching registration ownership

Independent re-review of exact `01da8acdc57240cd536234d062933e232b033eec` was accepted **as a review deliverable**, with implementation **REVISE** for P2. Mere live Alt+Space registration converted an ordinary Space release into a synthetic cancellation marker in both same/split-frame cases. This section supersedes the prior candidate's readiness claim, not its preserved evidence.

The correction changes only the shared hotkey control-edge ownership predicate: a matching chord or active physical-key guard owns an edge; base-key registration alone does not. Classification occurs **before** release clears the guard, retaining genuine hotkey cancellation even after modifier changes. Existing guard receive-order/repeat/synthetic rules and GenericInteraction cancellation/focus/editor behavior are unchanged. Space and Enter share this policy; no Alt+Space special case or FIFO/API change.

- [x] New permanent `a1_hotkey_ownership.rs`: **11 tests**, covering logical/physical Space+Enter, Alt/Ctrl registrations, same/split frames, no-registration controls, matching chords/releases, active guards with stale/synthetic/repeat releases, fresh post-guard gesture, traversal/focus cancellation and legacy Enter/editor Space.
- [x] **Identical new fixture** against direct-rustc rebuilt starting source: **5 fail / 6 pass**, exit101; against explicit corrected rlib: **11 pass**, exit0. Both ordinary Space positive tests fail starting source; three additional tests fail on their fresh unmodified Space gesture after cancellation/traversal. Enter/no-registration/compatibility controls pass starting source. No fixture alteration between builds.
- [x] Actual candidate rustc library invocation in `initial-focused.log`; exact explicit rlib SHA and all source hashes match the full run. Starting source is `git archive 01da8acd…`, compiled directly with existing dependency rlibs, bypassing Cargo snapshot fingerprints. No invalidation or broad cleanup needed this time; no second Cargo target.
- [x] Fresh relevant suites: **413 core + 62 paint + 14 UI + 1 AccessKit adapter passed**, zero failed/ignored; includes all **14 unchanged** prior correction tests and existing hotkey/editor/IME/source-history tests. Original independent adversarial repro now **4/4 pass**; original probe/mixed outputs retained as expected.
- [x] Offline locked umbrella all-targets/no-default check, GPU-free normal graph, six byte-identical galleries in a new output path, preserved recovery/gallery/prior-correction hashes, archived-evidence diff and targeted rustfmt all pass.
- [ ] Fresh independent exact-candidate re-review and owner A1 acceptance **still required**; A1 is not accepted, A2 remains blocked. No approval/integration/push performed.

Durable evidence: `evidence/a1-ownership-20261101-2t23Vj/README.md`, `run.sh`, `identity-verified-run/status.txt` (latest complete run; earlier `final-run/` also passed), copied reviewer report/repro/failing log with provenance. The entire `/tmp/rndr-rereview-01da.3eseqx/` remains untouched. All prior evidence, including archived invalid cached logs, is unchanged. An initial partial run stopped on a mistaken **harness expected-count assertion**, not a candidate test failure; its raw outputs are preserved and explained. Final disk **3.1 GiB free**, existing target **969 MiB** (started 3.2 GiB free). Native windows/screenshots/capture-blur integration/OS AT/preferences/native fonts-shaping/physical DPI remain **UNVERIFIED**.

### Owner exact-candidate acceptance checklist

- [ ] Review exact final candidate against baseline `291ab05493a59732f891dd762bf90c64e79a48d0`, using complete `evidence/a1-ownership-20261101-2t23Vj/baseline-changed-files.txt` and the focused correction inventory alongside it; distinguish reusable core prerequisites, passive A0 library, recovery fixes and these corrections.
- [ ] Independently re-review additive API spellings and corrected **FIFO64 requests / distinct intentional activations / no coalescing / explicit overflow** contract in `A1-api-decisions.md`. The earlier per-target contract was rejected, not accepted by tests.
- [ ] Independently inspect/reproduce offline behavioral, compatibility, real paint/glyph/layout and AccessKit evidence; verify archived log/golden/design hashes.
- [ ] Assess the UNVERIFIED native matrix below; do not claim optical/native/OS AT certification or complete usable library.
- [ ] Owner alone authorizes A2 continuation after A1 review and retains later fixes, review and native integration. No self-approval/integration occurred in this recovery.

## Remaining A2 / native matrix

| Area | Status after this checkpoint |
|---|---|
| A1 API spelling/cardinality independent review | **Pending owner/reviewer**; decision record is implementation candidate, not self-approval |
| Buttons/icon buttons/nav rows | A2 **not implemented**; no operating library widgets before review |
| Manual Source/Preview-style selector / keyboard orientation / roving entry / overflow UI | Semantic API prerequisite delivered; A2 composition/operating policy **not implemented** |
| Toolbar/app reading shell/visible source action | Passive reading recipe delivered; A2 operating compositions and Blooey adoption **not implemented** |
| Native window pointer capture/blur/mixed input | Core/headless events tested; actual host/native integration **unverified**; capture loss must call cancel |
| macOS VoiceOver, Windows UIA/NVDA, Linux/Wayland/Orca | **Unavailable/unverified in this headless run**, not certified by core/AccessKit tests |
| Windows/Linux native target execution/cross-build | **Not performed**; no claim of platform verification |
| OS appearance/contrast/preference following | Explicit settings work; automatic OS-follow bridge **unavailable/not implemented**, not reported as functioning |
| Native font resolution, optical HIG/Fluent alignment, CJK/RTL/emoji correctness | Family requests/content retention tested; actual native shaping/font fallback/optical behavior **unverified** |
| Native GPU screenshots, physical fractional-DPI/display transitions | Headless geometry verified; actual platform/GPU/display matrix **unverified** |
| Native materials, menus/dialogs, parser changes, networking/actions, Blooey adoption | Out of scope; no changes |

Exact current review source/evidence paths are listed in `evidence/a1-ownership-20261101-2t23Vj/baseline-changed-files.txt` (focused delta from 01da in `correction-changed-files.txt` there); historical `evidence/ui-changed-files.txt` is preserved. Commits are reported in the handoff and can be reproduced with `git log --reverse 291ab05493a59732f891dd762bf90c64e79a48d0..HEAD`. Review core prerequisites separately from the A0 library commit. Owner alone decides review acceptance and continuation.

## Current reconciliation — sender accepted scoped R1 review, not all A2 (2026-11-01)

The sender accepted the complete independent scoped HEADLESS R1 review of exact
`f427b7474dad99a983115edfad14c5084c9cb253` (implementation
`91c008bdc1054f42f67dcd5d45729290ddb4ffa4`). Prior P2 actual-content/snapping finding is resolved.
This supersedes historical “fresh R1 review required” wording only: it is NOT owner full-A2
acceptance, native certification, integration or project completion. Earlier A1-blocked/A2-not-started
paragraphs remain historical; accepted04642 already lifted the A1→A2 gate.

Complete independent report/commands/scripts/logs/negative controls/source identities and original
267-entry standard manifest are now durably preserved, without changing their provenance, under
[`evidence/a2-gate-reconciliation-20261101`](../../evidence/a2-gate-reconciliation-20261101/REPORT.md).
238 payload files are retained byte-for-byte;29 large compiled artifacts stay at their external
locations with bounded hashes/identities. The original /tmp directory and prior evidence are untouched.

The [current exact remaining-gate matrix](A2-remaining-gates.md) reconciles all A2 components,
Specification §8 numbered evidence gates, platform/manual cells, saved goals and previous reviews,
with executable next steps. No additional implementation defect is established in the latest scoped
review; this is not an exhaustive full-A2 review. Preserve the extra whole-world layout/text cost caveat
(100 unrelated-label calls→200/300; native responsiveness unverified) and the two pre-existing strict
core-test lint warnings at a1_wrapped_hug.rs79/125. No lint/performance/source fixes are made here.

Next concrete native gate: owner-authorized macOS synthetic operating-gallery keyboard/pointer/resize
and responsiveness, followed by real VoiceOver/IME/fonts/DPI checks; separate Windows/X11/Wayland
capabilities remain unavailable to this no-launch task. All native cells remain UNVERIFIED.
Owner/sender retains full A2 disposition and every project lifecycle decision. This reconciliation is
evidence/doc-only: no new build artifacts, launches, grants, source changes, approval, integration or push.
