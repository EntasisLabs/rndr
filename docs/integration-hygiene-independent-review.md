# First integration/hygiene increment — independent review and corrections

## Scope and disposition

Independently reviewed candidate **`5e5fe94b324953109341b59ec032ef1e81990e66`** against **`fe9e6798dddd9fdb07c8bbafa4af58d9e726f30c`** in the admitted isolated checkout. Runtime correction: **`6cac63d1dca2ca34b75e3d485b4ce28bad9da089`**. Packaging/CI correction and this report are in its following commit (use the full returned HEAD as the corrected candidate).

**Original candidate needs changes; reproduced defects have been corrected and the affected gates below pass. This is not implementation acceptance, release readiness, or native integration.** Owner retains sealed review/integration. Main remains at the baseline. No publishing, registry mutation, network access, push, cache deletion, unified list/palette, or later architecture work was performed.

## Findings

1. **P1 — Input-front wheel movement leaves hit-test geometry stale.** `apply_scroll` changed offsets, but Rect/Clip still described the preceding layout when canonical focus/capture ran. Added `wheel_then_pointer_press_and_release_use_scrolled_geometry` reproduced host 0 focusing row 0 instead of post-wheel row 1. This is a real input-targeting defect, not only an offset-count concern. `apply_scroll` now clamps/refreshes layout after actual wheel/drag movement, before contact capture or later input. Duplicate manual/Shell registrations still consume no second wheel or position delta.
2. **P2 — wheel + keyboard reveal reads pre-wheel rectangles.** Added `wheel_then_keyboard_reveal_uses_post_wheel_rects` reproduced Tab focusing row 1 while it remained offscreen at `Rect.y=-50`, offset `-100`. The same geometry correction resolves this. The policy is aggregate wheel first, then pointer/chronological keys within a frame; Input does not encode interleaved OS wheel/key timestamps, so no stronger native chronology is claimed.
3. **P1 — newly broken facade package normalization, then removed coverage.** Original `cargo package --offline --locked -p rndr --no-verify` failed because the optional path-only UI dependency lacked a version. Excluding the facade from CI did not resolve this. Version+path metadata now permits manifest normalization while retaining local API compatibility and **`rndr_ui publish=false`**. Ordinary packaging still fails offline because the registry has no matching `rndr_ui`: optional/default-off does not eliminate Cargo lockfile resolution. We did not invent a registry release or enable publication. CI now separately constructs the actual facade archive with **`--no-verify --exclude-lockfile`** and checks normalized optional/versioned dependency metadata, absence of local paths/lockfile, default-off feature, MSRV metadata, archive membership/source integrity and the UI publication guard. Other previously verified package gates remain. This archive-only gate is meaningful regression coverage, **not a verified registry build**.
4. **P2 compatibility refinement — automatic focus reveal opposes intentional scrolling.** Refreshing geometry exposed the existing unconditional idle reveal: the maintained Shell `pointer_drag_selects_the_pressed_row` test failed (offset 0 instead of -10). Fixing this by revealing only on input was too broad: two maintained select-popup tests require deferred reveal after layout. That intermediate approach was rejected. The final shared policy suspends idle reveal only for the focus displaced by deliberate wheel/drag, and resumes on changed focus, explicit same-target `request_focus`, key or press. Normal repeated/deferred popup reveal stays intact. Added `focused_row_does_not_snap_back_after_wheel_or_drag` covers release/idle persistence and explicit same-target reveal. The full core/UI suites pass, including both deferred-popup probes.

All seven scroll-installation tests pass. The four added behavior tests each exercise five modes: UiPlugin alone, manual Input-front registration before/after installation, and Shell installed before/after UiPlugin. Tests assert target identity, accepted release intent, focused-row visibility, drag cancellation and persistence—not just delta counts.

## Other review conclusions

- The namespace is opt-in/default-off `rndr::ui`, not a glob. Legacy root Button, Item, Theme and List identities are retained; feature-matrix TypeId checks and import doctests pass with and without desktop defaults. UI remains GPU-free as a direct library, though the facade remains the desktop stack.
- Collection Enter-Activated-without-selection versus click/Space/AT-Activate selection semantics are unchanged. Stable identity, embedded passive row content, selection/focus separation, inspector transactions and selection-popup behavior remain covered by the complete UI suite, including the maintained chronology and independent-review tests. This does not add embedded interactive row controls or virtualization.
- Inherited lint edits are narrow equivalents: Input initialization, vacant map insertion, short-circuit reconciliation, a named validator function-pointer alias, fixture expressions, separate observe action `if`s, an access-ID string allocation outside a predicate, and BGRA fixed-size chunks. `as_chunks_mut` stabilized in Rust **1.88**, below the workspace's declared **1.95** MSRV. This is API compatibility inspection, not an MSRV test: only macOS stable 1.98 was used and 1.95 is not installed. CI uses moving stable rather than proving MSRV.
- Docs distinguish live whole-window observation plus matching flattened accessibility metadata from headless/component capture and native certification. Historical reports are retained; the executor handoff is explicitly marked superseded for packaging, not silently rewritten into acceptance.

## Independent verification

macOS arm64, rustc/cargo **1.98.0**, Python **3.14.6**. Commands ran serially with the existing sibling `rndr-refined-cross-platform-ui-framework/target` cache and `scripts/data-gallery-env.sh` (jobs=2, incremental=0, dev/test debug=0). Cargo flags **`--offline --locked`** apply to every Cargo command below. Free disk was only **1.7 GiB** at admission and about **1.3 GiB** after verification. No broad cleanup or concurrent heavy builds.

| Exact command apart from the common Cargo flags | Result |
| --- | --- |
| `cargo test -p rndr_ui --test scroll_installation` with the first three added probes, before correction | 4 pass / **2 fail**, wrong focused row and offscreen Tab focus; expected reproductions |
| `cargo test -p rndr_core --test layout --test focus --test field --test ime --test a1_interaction --test a2_nested_focus -q` | **38 pass**; overlaps full core below |
| `cargo test -p rndr_core --tests -q` | **412 pass**, 50 test targets, zero failures/ignored |
| `cargo test -p rndr_ui --no-default-features --tests -q` | **163 pass**, 28 targets including empty lib target; includes seven scroll tests; zero failures/ignored |
| `cargo test -p rndr_ui --doc -q` | **2 pass** |
| `cargo test -p rndr --no-default-features --lib -q`; `cargo test -p rndr --lib -q` | **1 pass each** |
| `cargo test -p rndr --no-default-features --features ui --lib -q`; same flags with `--doc -q` instead | **2 unit + 2 doc pass** |
| `cargo test -p rndr --features ui --lib -q`; same flags with `--doc -q` instead | **2 unit + 2 doc pass** |
| `cargo test -p rndr_render --features observe --lib -q` | **76 pass** |
| `python3 -m unittest discover -s tools -p 'test_observe*.py' -v` | **20 pass** |
| `cargo clippy -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings` | Pass |
| `cargo clippy -p rndr --no-default-features --features ui --all-targets -- -D warnings` | Pass |
| `cargo clippy -p rndr_ui -p rndr_render -p rndr --features rndr/ui,rndr/observe,rndr_render/observe --all-targets -- -D warnings` | Pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_ui -p rndr_render -p rndr --features rndr/ui,rndr/observe,rndr_render/observe --no-deps` | Pass |
| `cargo build -p rndr --features observe,ui --example ui_gallery --example data_gallery --example data_editors_gallery --example selection_gallery` | All four linked successfully |
| `python3 scripts/check-facade-archive.py --allow-dirty` | Pass, actual 10-file archive plus normalized/source checks; clean invocation is the CI gate |
| `cargo fmt --all -- --check`; `git diff --check` | Pass |

One review command initially combined `--lib --doc`, which Cargo rejects; the separate commands above passed. These runs and intermediate failed correction experiments are not silently counted as passing tests. Counts overlap; do not sum them as unique tests.

### Compact evidence references

Private external logs: `/var/folders/cw/ql6_4ftd3nb2mwwjptzz8j3c0000gn/T/rndr-hygiene-review.8cpesX/`. These are local temporary evidence, not durable release artifacts; no raw logs/captures were added to Git. SHA-256:

| Log | SHA-256 |
| --- | --- |
| `rndr-review-scroll-before.log` | `02b44d5fc196c5980fd7f8ef026307543a9d8fcdbf97944a9cd3efae50fd1d64` |
| `rndr-review-package-before.log` | `566dd481da3e9d2d03b00a4054f1ad9ae9befdfdc17d22c0d9397035232f757a` |
| `rndr-review-package-version.log` | `ed324774c29ce7ae437f54034d205db62b9ec09949c048e154b03ec9285162fa` |
| `rndr-review-core-full.log` | `f6e0e4dd3e826566dbbd91b71b6a5ac4066fdfbe3f1210673c57004814f3c575` |
| `rndr-review-ui.log` | `beabc70634ea8a088e8e577788a4600ffa8bab5c6a25814eda7102a03b3d60da` |
| `rndr-review-facade.log` | `6f300c87f8b15259b82139947626d3d73c12c16242840797ce4582e244f483c9` |
| `rndr-review-render.log` | `62ef9ad4e9bd3e3f0b5b9e31849ccc1689123a7d63efae68ab41282fe1a5c31d` |
| `rndr-review-python.log` | `ada3ce876e7f56a24f3fc1a6f9f84e7879f05383797738c09019c796ee909e04` |

## Unresolved boundaries / owner handoff

- **No registry-ready facade build is demonstrated or currently possible with this unpublished dependency.** Archive-only packaging omits Cargo.lock and skips verification; local feature tests are not a substitute for registry verification. Keep publication disabled. No publishing/registry activity is authorized here.
- Linux/Windows compilation, GitHub Actions execution, Rust 1.95 builds, broad workspace all-features gates and full workspace verified tarball builds were not independently run. The checked-in matrix schedules broader work, but it is not evidence of its result.
- No physical pointer/keyboard, OS-IME, native AT, fresh pixel sign-off or compositor/platform certification. Four linked galleries and headless snapshots do not establish those claims. Added immediate layout propagation was correctness-tested, not performance-benchmarked.
- Owner must perform sealed review/native integration before any separately authorized unified-list follow-on. No approval or integration was performed by this reviewer; archived evidence and main remain unchanged.
