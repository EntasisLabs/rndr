# A0+A1 checkpoint — ready for independent A1 review, not A2 clearance

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
- [x] Explicit cancellation and ownership: matching primary pointer, release-outside, Escape, blur, focus loss, disable/readonly/hidden, scroll capture, explicit capture loss, destroy/stale generation. Rich release edges survive only for opted-in controls outside registered hotkey ownership.
- [x] **Review decision recorded:** bounded FIFO64, one pending intent per target until host drain, observable Accepted/AlreadyPending/Overflow/Rejected and counters, no eviction/overwrite/dual Fired delivery. This is an explicit limited-cardinality contract, **not an unrestricted lossless stream**. Independent reviewer must accept or revise it before A2.
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

## Recovery verification — current review candidate

Recovery began at preserved HEAD `eb2a6f0b7630bf27fd2f32e585aa2125e28c21f1`. The modified tracking record and four untracked checkpoint/evidence files were inventoried and retained, then reconciled here. Supported native admission succeeded with exclusive execution; no extra old-worker raw observation prerequisite was imposed, and no credentials/raw HTTP/re-interruption/fencing bypass was used.

Additional separately reviewable source fixes:
- `fc6f622eec4411b103a33a45168103c02b2dde7e` — semantic tab relations revalidated after host reparent/component removal; stale advertising/activation/panel exposure reproduced before fixing.
- `d1610de48e4f249ad83788161d25368bdd935534` — malformed owned title Label rejected before any Card/Callout recipe mutation; failure reproduced before fixing.

The final documentation/evidence commit contains no further source changes. Exact candidate is that final commit; the tested source tree is `d1610de48e4f249ad83788161d25368bdd935534`. All prior five source commits remain ancestors, listed in `evidence/ui-code-commits.txt`.

Run from the worktree root:

```sh
sh evidence/recovery/run.sh
shasum -a 256 -c evidence/recovery/logs-sha256.txt
shasum -a 256 -c evidence/ui-gallery-sha256.txt
```

The script uses offline **locked** resolution, jobs=2, debug=0, incremental=false and the existing local target cache. It exits on any failed cargo command or gallery mismatch. Saved log hashes identify this run; rerunning changes timing/build-status text and therefore may change log hashes. Verify archived hashes before overwriting logs when exact evidence custody matters.

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

### Owner exact-candidate acceptance checklist

- [ ] Review exact final candidate against baseline `291ab05493a59732f891dd762bf90c64e79a48d0`, using complete `evidence/ui-changed-files.txt`; distinguish reusable core prerequisites, passive A0 library and recovery fixes.
- [ ] Independently accept/revise additive API spellings and **FIFO64 / one outstanding intent per target / explicit coalescing and overflow** decision in `A1-api-decisions.md`. Tests do not grant this acceptance.
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

Exact review source/evidence paths are listed in `evidence/ui-changed-files.txt`; commits are reported in the handoff and can be reproduced with `git log --reverse 291ab05493a59732f891dd762bf90c64e79a48d0..HEAD`. Review core prerequisites separately from the A0 library commit. Owner alone decides review acceptance and continuation.
