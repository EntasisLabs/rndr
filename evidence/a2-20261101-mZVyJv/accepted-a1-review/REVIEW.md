# Independent exact-candidate A0/A1 re-review — ACCEPT (headless scope)

## Verdict and authority

**ACCEPT for the reviewed A0/A1 implementation candidate. No blocking P0/P1/P2 finding identified. One P3 permanent-test coverage nit is recorded below.** The prior P2 nonmatching-registration ownership defect is resolved by the shared predicate, not an Alt+Space special case. This is an independent review recommendation, **not owner/sender acceptance, A2 authorization, native certification, approval, integration or push**. Owner retains those decisions.

Reviewed exact commit **04642fe03f7603f228eaf9b83611641a95676e8d**, tree **924cc8a9f232845c59735189d59807f78ee2b77a**, against original authorized baseline `291ab05493a59732f891dd762bf90c64e79a48d0`, including current delta from `01da8acdc57240cd536234d062933e232b033eec`. Worktree:

`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`

Git status --porcelain was empty before and after review. All new review outputs are in this uniquely created directory or the runner's uniquely created small direct-rustc source/build directory. No tracked/untracked worktree source, design record, permanent test or prior evidence was edited. No fingerprint invalidation, cleanup, second Cargo target, network, secrets, raw HTTP, custody workaround, commit, integration or push was used.

## Findings

### P3 (nonblocking): permanent fixture's logical-only branch is not logical-only

**`crates/rndr_core/tests/a1_hotkey_ownership.rs:53–68`**, especially 60–67; constructor contract **`crates/rndr_core/src/keys.rs:378–385`**.

The `physical=false` branch leaves `KeyEvent::named`'s already-populated Space/Enter physical code intact. The `physical=true` branch merely writes the same code again. Thus the comment claiming both logical-only and physical-code coverage is not established by this loop, even though the production correction is correct in the independently tested missing cases.

Follow-up recommendation: explicitly set `down.code=None` in the false branch and assert the two shapes differ; preserve all existing tests. **No implementation/test edits were made in this review.** This is not blocking because `independent.rs` explicitly asserts `code==None`, tests 160 nonmatching-registration combinations (4 core Platform values × 2 keys × 2 frame partitions × 5 registrations), and adds genuine logical-only guarded/stale/synthetic, physical-binding, modifier, focus, legacy/editor and FIFO probes. All eight independent tests pass against both the explicitly named cached candidate library and a newly direct-built exact-current-source library. Core Platform emulation is not native platform execution.

### Prior P2: RESOLVED

**`crates/rndr_core/src/hotkey/registry.rs:539–553, 566–584`**.

The old `live` base-code predicate is no longer the ownership test. Actual matching registered stroke or an active guard classifies ownership **before** a real release removes that guard. Mere registration cannot convert ordinary Space up to a synthetic cancellation marker. Same-frame and split-frame ordinary Space now activate exactly once; Enter and no-registration controls remain correct. Genuine chord matches and guard-owned releases still cancel, including after modifier changes. The original independent four-test repro passes, and the unchanged new eleven-test fixture fails 5 / passes 6 on rebuilt exact01da but passes all11 on the exact candidate. This is direct behavioral counterfactual evidence, not an inference from the production diff.

## Full A0/A1 scope audit (not delta-only)

Read the complete approved Specification (1–339), Review Guide (1–70), A1-api-decisions, A0-A1-checkpoint and implementation-tracking; read the copied previous independent report, current evidence README/runner and manifests. Reviewed full baseline production scope, affected scheduling/callers/contracts and relevant tests; `scope.txt`, `reviewed-commits.txt`, `core-diff.txt` and source manifests retain the inventory/identity.

| Area | Independent assessment and evidence |
| --- | --- |
| Additive architecture | `rndr_ui` is opt-in and has normal dependency only on existing GPU-free core; paint is dev/gallery only. No new renderer, event loop, window lifecycle, Shell/app policy, parser or Blooey adoption. Normal dependency tree assertion and umbrella all-target/no-default check pass. |
| A0 tokens and state vocabulary | Light/Dark, explicit high contrast, density independent of UI/reading scale, semantic color pairs, focus width/gap, spacing, typography, corners, target minimum and reading measure are present. Contrast/scale/state tests pass. A0 tokens/passive content are not an operating A2 button/focus-rendering/selection widget or automatic OS preference bridge. |
| Style ownership and passive composition | Reviewed reversible per-property original/applied ownership, recipe/app override separation, explicit primitive and transparent-alpha precedence, detach/theme update, owned vs borrowed/adopted content, keyed mount/update/destroy, stale generations and malformed title rejection before recipe mutation. Current foundation/lifecycle tests pass without reconstruction. No unrelated caller state is intentionally destroyed. |
| Actual layout/paint | Reviewed separately scoped wrapped-Hug refinement in core, width/min-size/Fill reuse and clipping order; no UI-side height calculator. Full 32-state passive matrix, glyph/UV/clip geometry tests and fractional/2× logical geometry pass. All six regenerated actual display-list galleries are byte-identical to preserved goldens. These are not native screenshots or proof of native shaping/optical quality. |
| Installation/scheduling/focus | GenericInteraction reuses idempotent Chrome and authoritative Focus/Input/World; repeated UiPlugin/GenericInteraction/Chrome install does not reset queues or duplicate systems. Pointer policy runs before ordered rich key processing; Tab traversal updates ownership in-frame; legacy shim reconstructed from remaining press, not processed in parallel with rich keys. No new cross-device timestamp-merging promise. |
| Modern interaction and cancellation | ReleaseActivation is per-control opt-in. Matching primary pointer press/release, in-bounds commit, outside/capture loss, Escape/blur, focus roundtrip, hidden/readonly/disabled/destruction/stale generation and scroll capture paths reviewed and covered by current core suites. Enter commits only fresh nonsynthetic down; Space resolves its arm on up; repeat/synthetic/modified or unmatched up cannot leave a stale arm to be committed by a later ordinary up. Editor CapturesKeys remains authoritative. |
| Hotkey ownership | Shared matching-stroke/active-guard predicate handles Space and Enter. Existing receive-order guard helpers remain intact: old/synthetic release cannot clear an active guard; hotkey-owned private edges cancel and do not reach legacy consumers. Both physical-code and genuine code=None logical probes pass; nonmatching registrations, matching release, stale/synthetic guard sequences, fresh post-guard gesture, modifier/focus transitions and registration-drop behavior tested. `KeyEvent.order` is a receive-order fence for guards, not a general queue sorting/dedup API; hosts preserve per-frame receive order. |
| Cardinality and API | `ActionIntents` is FIFO64 **requests**, not targets. Same-target Enter/AT/mixed-source intentional activations retain multiplicity; no inferred dedup/coalescing, eviction, overwrite or modern→Fired fanout. Accepted/Overflow/Rejected and overflow counter are explicit. Rejected requests do not consume capacity. Drain revalidates generation, visibility, enabled/readonly and action name. All14 prior correction tests remain byte-identical and pass. No production AlreadyPending/coalesced path remains in reviewed implementation. |
| Producer/host responsibilities | One canonical producer per physical activation is necessary; duplicate AT/direct calls plus synthetic keys are separate requests, not automatically correlated. Hosts must clear frame input normally, drain after relevant Update producers, handle bounded overflow and revalidate command authority/arguments. This is not an unbounded stream or deduplication service. Legacy Fired remains single-slot. |
| Accessibility/editor safety | Persistent Semantics names/descriptions/readonly and folded visible labels are not inferred from placeholder/current value. Core and actual AccessKit adapter share action matrix and relations; SetValue is explicitly unsupported/not advertised, avoiding raw Label/Buffer/history/IME bypass. Readonly user text/IME/clipboard guards preserve navigation/selection/Copy, while authorized programmatic/source-edit APIs retain their authority. Full existing editor/IME/history/source-edit suites pass. |
| Tabs/relations | Reviewed validated tab/group/panel links, duplicate-link rejection, focused-vs-selected separation, manual move API, canonical selection/panel synchronization, inactive exclusion and live relation revalidation after removal/reparent/destruction. Actual AccessKit controls/labelled_by/TabPanel mapping test passes. A2 orientation keys, roving entry, operating selector/overflow UI are not claimed. |
| Compatibility | Old public structs remain unchanged; additive wrappers/types carry new semantics. Unmarked buttons keep old press/Enter/Fired behavior. Original mixed probe confirms same-frame Tab→legacy Enter and separate Enter; field/editor Space retained. Explicit modern queue never fans out to legacy. Existing source-edit/history/IME tests included in 413 core passes. Deliberate unsupported AT SetValue is the specified safety correction, not a claim of complete native AT text entry. |

## Independently rerun commands and counts

All Cargo commands used `--offline --locked`, existing `target`, `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. Toolchain: rustc1.98.0 (88d9e12ae, 2026-08-18), aarch64-apple-darwin. The new run was invoked from worktree root:

```sh
sh evidence/a1-ownership-20261101-2t23Vj/run.sh /tmp/rndr-independent-04642.hmZHsh/rerun
```

`rerun/status.txt` has exact command/exit receipts; the runner refuses an existing output directory. Its outer invocation exited0.

| Command/check | Result |
| --- | --- |
| cargo test --offline --locked -p rndr_core -v | **413 passed / 0 failed / 0 ignored**, including unchanged prior14 and new11 |
| cargo test --offline --locked -p rndr_paint | **62 passed / 0 failed / 0 ignored** |
| cargo test --offline --locked -p rndr_ui | **14 passed / 0 failed / 0 ignored** |
| cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests | **1 passed**, 0failed/ignored, 22filtered |
| Aggregate requested suites | **490 passed**, not a native certification |
| cargo check --offline --locked -p rndr --all-targets --no-default-features | exit0 |
| cargo tree --offline --locked -p rndr_ui --edges normal + forbidden-dependency assertion | exit0, GPU/runtime-free normal UI graph |
| passive_gallery run + diff -rq evidence/ui-gallery new/gallery | exit0, **6 byte-identical** display lists |
| Original independent adversarial.rs, explicit cached candidate rlib | **4 passed**, exit0 |
| Original probe.rs and mixed.rs, explicit cached candidate rlib | exit0: Tab+Space one intent, later unmatched up none; both legacy Enter probes Fired("legacy") |
| New11 identical fixture vs direct-rustc exact01da | expected **6 passed / 5 failed**, exit101 |
| Same new11 fixture vs explicit cached candidate rlib | **11 passed**, exit0 |
| recovery/gallery/prior-correction manifests + archived evidence diff + targeted rustfmt | all exit0 |

## Exact build identity — no Fresh-only trust

1. Cached candidate rlib is explicitly named `target/debug/deps/librndr_core-99fc9f1d1448f44b.rlib`, SHA256 **3073d3d06f9684c75b2504e18d9f36ea0147db3554106a929c6eb083aa1cbb90**. Hash matched before/after rerun, and all current core source hashes match the sealed worker source manifest. `worker-actual-library-invocation.txt` extracts the real candidate Cargo rustc invocation from preserved initial-focused.log; that is historical build provenance, not a claim this Cargo rerun recompiled a Fresh library.
2. To independently bypass the known Cargo fingerprint risk, I directly compiled **current** `crates/rndr_core/src/lib.rs` with actual rustc, explicit dependency rlibs, default cfg, debug0 and no incremental Cargo target. `run-independent.sh` and **`direct-verified/transcript.log`** record the exact executed invocation, toolchain, source checks and explicit input library hashes. Output `direct-verified/librndr_core.rlib` SHA256 **e352fb5d9da87d4d511d4176c927fcc2aa273b562c6dc1c3a06d317bd343acd2**. Direct output need not equal Cargo's path/configuration-dependent library hash.
3. Against that newly built explicit direct library: **new11 pass + unchanged prior14 pass + original independent4 pass + independent8 pass**, all build/run exits0. Exact commands, fixture copies, statuses, hashes and logs are under `direct-verified/`. These 37 supplemental executions overlap suite fixtures; they are not 37 additional distinct tests added to the 490 suite total.
4. The independent8 also pass against the explicitly named cached rlib (`cached-independent.log`), agreeing with the direct-source build.
5. Counterfactual exact01da was exported and directly rebuilt with rustc, not selected from Cargo workspace cache. I independently compared **all58 exported core source files byte-for-byte to exact01da Git blobs**, retained in `counterfactual-git-source-verification.txt`. Actual old-library invocation/status, SHA and fixture executions are in `rerun/`; exported source/library remains `/tmp/rndr-a1-ownership-verify-gR034q/`. Old direct library SHA256 **0b8486e22fe8028363cce595c38f89b21882c4c92e703f252014fa44eeffe9ed**. Both variants use the same fixture SHA256 **4699893bc4f7509860d1c9d88bdcb641414be9d5bc8c64c4b2842d2ea2fc28cd**. The 5fail/6pass historical outcome is expected, not a candidate failure.
6. Prior14 SHA256 **7adf72bcf44793dc7da042d6589aadd87360ba6b056412fcc84498497173464c** is byte-identical to01da and passes in current suites and direct rebuild. Its earlier f7aa counterfactual was inspected in preserved prior review, **not rerun here**; this review's fresh counterfactual targets exact01da as requested.

## Evidence/proposal preservation and disclosed non-success attempts

Worker evidence-manifest SHA256 **8183cabe575e2b8aa70ae080709379bc38fb9db28c7e5f13bd7eed3f88b99009** and source/docs manifest SHA256 **75a4f5dfbfb5fbbd0aa44b20667bc0a27c84d11d52c37bf2e9cdd2e431944ea2** match supplied receipts. Both full manifest checks pass here (`ownership-evidence-verification.log`, `ownership-source-verification.log`). Recovery/gallery/prior-correction hashes and Git archived-evidence diff also pass. Original reviewer source/report and historical invalid-cache/expected-failure/partial logs were read or referenced in place, never rewritten or normalized. Original `/tmp/rndr-rereview-01da.3eseqx/` was not mutated.

Approved Specification SHA256 **c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b**, Guide **8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0**. Both match tracking's approved digests and are unchanged since proposal import commit4976d580. An initial baseline-to-HEAD proposal diff check exited1 because those files are **newly copied into the repository after baseline291ab05**, not because proposals were edited. That diff is retained as `approved-docs-unchanged.log`; the correct import-to-HEAD comparison exits0 (`approved-docs-from-import-unchanged.log`). No failure was hidden.

My initial external independent test harness used a nonexistent `Keystroke::new` and private NamedKey::code API; its compile exit1 and original source are preserved as `independent-initial-invalid.rs` / `direct/independent-initial-invalid-build.log`. Correcting only reviewer harness construction to public `Keystroke::code` produced passing tests. No candidate source or assertion policy was changed. Earlier worker expected-count assertion failure and cached-invalid logs remain historical, not current candidate results. Raw logs are not whitespace-normalized.

`reviewed-source-docs-sha256.txt` hashes all tracked Cargo/crates/design inputs; `key-input-hashes.txt`, `final-git-identity.txt`, clean-before/after, runner identities/statuses and final review evidence manifest identify this review. An evidence manifest excludes only itself to avoid recursion.

## Native limitations and residual host responsibilities

**UNVERIFIED:** real native windows and screenshots, actual pointer capture/blur integration, OS AT (VoiceOver/UIA/NVDA/Orca), OS appearance/contrast preferences, native font resolution/shaping/RTL/CJK/emoji quality, physical/fractional DPI/display transitions, Windows/Linux native execution and feature sets outside the recorded commands. Headless AccessKit and actual paint/glyph display lists are valuable but are **not** native optical/assistive certification. Core assumes the real host feeds accurate key identity/repeat/synthetic/receive order and calls cancellation on native capture loss/blur. Core has no per-physical-gesture cross-source correlation token; canonical producer policy remains necessary.

Native materials, operating A2 components/gallery, orientation/roving/overflow policies and Blooey adoption remain outside this A0/A1 review. Do not present the passive A0 gallery as a complete usable library or this verdict as approval to start A2.

Start filesystem free3.1GiB; observed final free2.3GiB, existing Cargo target969MiB, this review directory about75MiB. Filesystem delta is not attributed solely to this review. No cleanup was attempted to recover space. Small direct-rustc outputs/source exports are retained as evidence, not a second large Cargo target.

## Disposition

**ACCEPT (independent A0/A1 headless/code review), one nonblocking P3 fixture-coverage nit, prior P2 resolved.** No implementation correction is requested as an A1 blocker by this review. Owner must assess this exact candidate, native limitations and evidence before their own sender/A1 acceptance and any A2 continuation. Clean tree and reviewed SHA remain unchanged. Nothing approved, integrated or pushed.
