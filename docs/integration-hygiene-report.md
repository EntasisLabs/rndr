# First consolidation increment — review handoff

Baseline: `fe9e6798dddd9fdb07c8bbafa4af58d9e726f30c`. Candidate is the commit containing this report on the admitted isolated worktree branch. Executor has not approved, integrated, pushed or published it. No unified list/palette or later architectural increment is included.

## Audited findings and diff

The older `c9536ac` report was checked against current sources. **rndr_ui already had a README**; its content/metadata/status, not existence, needed correction. Data editors and selection controls are implemented and owner-integrated in the baseline. Remaining gaps verified: UiPlugin lacked scrolling installation, facade had only a UI gallery dev dependency, CI omitted the new galleries/Python tests/UI minimal clippy, and current status prose still used old candidate/roadmap wording.

- UiPlugin installs existing core `apply_scroll` at Input front, retaining core focus/activation. Core already consumes wheel deltas and advances the last-applied drag position. Three new tests exercise minimal hosts, repeat plugin install, legacy Input registrations before/after plugin and Shell before/after plugin; no multiplied wheel/drag effect. Three actual galleries now omit manual scrolling.
- Optional default-off facade feature `ui` exposes **`rndr::ui`**, never a semantic glob at the root. Unit TypeId checks and doctests cover legacy Button/Item/Theme/List plus explicit semantic imports. Legacy core API remains unchanged.
- UI metadata describes implemented components, wires the existing README, adds keywords/category and **retains publish=false**. The optional local UI dependency is deliberately not registry-release ready. CI explicitly excludes internal rndr_ui and the facade from tarball packaging rather than claim otherwise; feature builds/docs remain covered. Release scripts are not enabled/expanded.
- UI missing-doc policy is now `deny(missing_docs)`, matching core/facade. It checked warning-free before tightening; no blanket suppressions added.
- Root/crate docs, collection/data/selection docs and the historical A0/A1/A2 record headers point to `docs/ui-status.md`. Old evidence remains intact. Screen-reader Activate-selects versus Enter-Activated-without-selection is documented, not changed. Observe is described as live whole-window surface pixels plus matched flattened accessibility metadata, Unix-only, with explicit privacy/certification/targeting limitations.
- CI builds ui_gallery, data_gallery, data_editors_gallery and selection_gallery with observe+ui; runs Python observe tests on supported Unix runners; includes UI in no-default clippy and covers facade feature configurations.
- Strict clippy exposed small inherited style warnings. Fixes are equivalent: Input initializer, vacant map entry and collapsed reconciliation condition, named existing validator function type, three fixture expressions, independent observe action-if newlines, fixed-size BGRA chunks (within MSRV), and allocating a **string** access-ID once outside a test predicate. No focus/transaction/capture architecture was rewritten.

## Verification actually run

Host: macOS/Darwin arm64, rustc/cargo 1.98.0, Python 3.14.6. Cargo commands below used `--offline --locked`, an explicitly inspected existing shared target cache (`../rndr-refined-cross-platform-ui-framework/target`) and `scripts/data-gallery-env.sh`: jobs=2, incremental=0, dev/test debug=0. No concurrent large builds or cache deletion. Free space was 3.6 GiB initially and about 3.0 GiB at final audit; the initial shared cache was 5.3 GiB.

| Command (Cargo flags `--offline --locked` omitted here only for readability) | Actual outcome |
| --- | --- |
| `cargo check -p rndr_ui --lib` | Pass, no missing-doc warnings before policy tightening |
| `cargo test -p rndr_ui --test scroll_installation --test collections --test collection_chronology` | 32 pass: 3 + 17 + 12; overlaps full UI run below |
| `cargo test -p rndr_ui --no-default-features --tests -q` | **159 pass**, zero failed/ignored; all 27 integration binaries plus empty lib target, including gallery regressions |
| `cargo test -p rndr_core --test field --test ime --test layout -q` | **24 pass**: field 5, IME snapshot 3, layout 16; not real OS IME certification |
| `cargo test -p rndr --no-default-features --lib -q` and `cargo test -p rndr --lib -q` | 1 pass each; legacy namespace |
| `cargo test -p rndr --no-default-features --features ui --lib -q` / `--doc -q` (separate commands) | 2 unit + 2 doc pass; repeated earlier minimal-feature runs overlap |
| `cargo test -p rndr --features ui --lib -q` / `--doc -q` (separate commands) | 2 unit + 2 doc pass; desktop defaults |
| `cargo test -p rndr_render --features observe --lib -q` | **76 pass**, zero failed/ignored |
| `python3 -m unittest discover -s tools -p 'test_observe*.py' -v` | **20 pass** on macOS, including permission/FIFO/race/paired-output tests |
| `cargo clippy -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings` | Pass after narrow inherited lint fixes |
| `cargo clippy -p rndr --no-default-features --features ui --all-targets -- -D warnings` | Pass |
| `cargo clippy -p rndr_ui -p rndr_render -p rndr --features rndr/ui,rndr/observe,rndr_render/observe --all-targets -- -D warnings` | Pass after equivalent observe lints; runs used `--keep-going` |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_ui -p rndr_render -p rndr --features rndr/ui,rndr/observe,rndr_render/observe --no-deps` | Pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p rndr --no-default-features --no-deps` | Pass; no disabled-ui broken link |
| `cargo build -p rndr --features observe,ui --example ui_gallery --example data_gallery --example data_editors_gallery --example selection_gallery` | Pass; four linked binaries, repeated successfully after final renderer lint changes |
| `cargo metadata --no-deps --format-version 1` | Pass; UI publish=[] (disabled), README/description/keywords/categories confirmed |
| `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml")'` | Pass syntax parse, not a GitHub matrix execution |
| `cargo fmt --all -- --check`, `git diff --check` | Pass after formatting new tests/import assertions |

Earlier clippy iterations returned 101 for the listed inherited lints before correction. One early `cargo test --lib --doc` invocation returned 101 because those selectors cannot be combined; no tests ran in that invocation, and separate commands passed. `cargo package -p rndr --list --allow-dirty` returned 0 (file listing only); **`cargo package -p rndr --allow-dirty --no-verify` returned 101** at manifest normalization: `rndr_ui` lacks a registry version requirement. This is the deliberately unresolved dependency/publication-policy block, not a passing package gate. No publication was attempted.

## Gaps and focused owner review checklist

- [ ] Review scroll installation ordering and five host registration modes; ensure Input-front ownership and delta consumption are the intended compatibility contract, not a claim that every legacy function registration is removed.
- [ ] Review optional/default-off `rndr::ui` and collision-safe imports, API-equivalent lint changes and preserved selection/activation/transaction behavior.
- [ ] Review explicit CI internal-UI/facade package exclusions while UI stays unpublished. A registry dependency/release decision is future owner work, **not** a reason to enable publishing here.
- [ ] Review current-vs-historical status wording and observe limits. No fresh native pixel inspection, physical scrolling/AT or real desktop IME testing was done in this hygiene pass. Gallery binaries and headless regressions are not native certification.
- [ ] Linux/Windows compilation and CI execution were **not run locally**. Python observe tests are scheduled on macOS/Linux only; non-Unix live transport remains unsupported. Ruby parsing is not Actions execution.
- [ ] Broad workspace all-features tests/clippy/docs, full workspace tarball/package verification and MSRV toolchain runs were **not run**; bounded affected gates above were chosen under low disk space. Do not interpret them as all CI green.
- [ ] `git diff --name-only -- evidence` is empty. No bulky tracked captures/logs were added, archived evidence deleted, history rewritten, other checkout source changed or broad caches cleaned. Sustainable private external/CI artifact handling is documented in ui-status.md.
- [ ] Owner independently review/fix and integrate this candidate before any separately assigned unified virtualized list + palette increment. No overlay, markdown slots, capture architecture, IME or startup rewrite starts here.
