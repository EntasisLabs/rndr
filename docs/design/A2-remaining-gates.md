# Current A2 gate matrix — scoped R1 accepted, A2 decision still open

Date: 2026-11-01. This is a documentation/evidence reconciliation, **not a new independent review**.
Authority: sender's current instruction accepts the scoped headless R1 review at exact
`f427b7474dad99a983115edfad14c5084c9cb253`; it expressly does NOT accept all A2 or authorize integration.
No owner/project acceptance is invented here. Original whole A0→A1→A2 goal remains in force;
no native execution, additional grants, implementation/refactoring, app adoption or lifecycle action is admitted by this task.

## Evidence and status vocabulary

- **H — completed headless:** automated/source evidence exists, not a native pass or new owner acceptance.
- **R — resolved finding:** specific earlier defect corrected and independently rechecked.
- **U — unverified native/manual:** no executed native receipt. This task prohibits native launches.
- **C — unavailable capability/authorization:** no authorized native session/OS/AT/target supplied in this bounded task; not a claim that the host inherently cannot do it.
- **D — deferred/unsupported by explicit contract:** not silently counted as implemented or a current blocker to the bounded scaffold.
- **O — owner decision open:** only owner/sender can assess the full A2 deliverable and project lifecycle.

Primary preserved independent evidence: [complete REVIEW](../../evidence/a2-gate-reconciliation-20261101/independent/REVIEW.md),
[complete COMMANDS](../../evidence/a2-gate-reconciliation-20261101/independent/COMMANDS.md).
[Preservation report](../../evidence/a2-gate-reconciliation-20261101/REPORT.md) and inventory distinguish retained sources/logs from external binaries.
The independent reviewer is `ses_peer_d1737b092b7f20e04afee355b12757431811473f7d9e447a840b8a1589205231`,
assignment `2a057eb53b9ddae6099e66536ef6d7dc06775d630e940c0d35f5b3a9f4e667cf`,
attempt `att-18e3ea0698b05e2041db6219c3e26564`. This reconciler preserves their verdict; it does not become that reviewer.

## Historical versus current decisions

| Exact checkpoint / review | Historical outcome | Current meaning / next step |
|---|---|---|
| Initial research Specification / Review Guide | Held proposal; no coder launched at research time | Immutable proposal history, not current execution status. Approval entry124 and tracking record authorize original A0→A1→A2 only. |
| A0/A1 `f7aa9703…` then `01da8acd…` | Independent REVISE: ordering/legacy/cardinality then hotkey ownership | Historical fixes covered by accepted A1; do not reopen as current A2 defects. See A1 API/checkpoint and prior reports. |
| Accepted `04642fe03f7603f228eaf9b83611641a95676e8d` | Owner accepted HEADLESS A0/A1/API/FIFO64; lifted A2 gate | A1 no longer blocks A2. Native evidence/integration were NOT accepted. |
| A2 `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d` | REVISE: total natural widths did not prove equal-share fit | Historical R1. [First review](../../evidence/a2-r1-20261101/independent-review/REVIEW.md). |
| First correction `92e98004d886a9ccfc3d2d05eaf090b591e10cc9` | REVISE: text-origin inset and snapping omitted | Historical R1 continuation. [Second review](../../evidence/a2-r1-content-fit-20261101/independent-review/REVIEW.md). Five old boundary tests had an invalid oracle, not five new product defects. |
| Implementation `91c008bdc1054f42f67dcd5d45729290ddb4ffa4`, evidence child `f427b7474dad99a983115edfad14c5084c9cb253` | Independent ACCEPT for scoped HEADLESS R1 correction; sender now accepts that review | R1 P2 resolved. No remaining P1/P2 reproduced within that scope. Not an exhaustive all-A2/native acceptance. Do not schedule another identical review. |
| This evidence/doc-only child | Evidence preservation and gate reconciliation | Sender assesses this deliverable; owner still decides full A2 and later lifecycle. |

Old “A2 not started”, “A1 blocked”, “fresh R1 review required” paragraphs in tracking/checkpoint/A2-components are historical at their checkpoints. The current sections and this matrix supersede their *status*, not their source receipts. No past report/golden was overwritten.

## Specification §4 A2 — every contracted component

Evidence anchors are real current tests plus preserved independent reruns, not tests newly executed by this reconciler.

| Requirement | Status and evidence | Remaining gate / executable next step |
|---|---|---|
| §4 A0 passive foundations and A1 reviewed prerequisites | H; accepted04642, API decision record, FIFO64 distinct requests and overflow; original prior14 preserved | No repeat A1 review needed. Carry accepted cardinality into full A2 owner assessment. Native limits below stay open. |
| A2.1 Button primary/secondary/quiet/destructive + named IconButton | H; `a2_operating`, `a2_states`; canonical release-cancellable input, name validation, actual paint; independent contract audit | U/C: native pointer/keyboard/AT gate below; no additional headless defect established. |
| A2.2 NavigationRow selection distinct from focus; dirty/secondary text | H; operating/state tests, selected marker + focus ring + AccessKit selected metadata | U/C: inspect focused+selected in real window and reader. |
| A2.3 Manual ModeSelector; real tab/panel relations and hidden exclusion | H; `a2_operating`, core manual-tab tests, actual adapter2, 13 conformance probes | U/C: execute arrows/Home/End/Tab/Enter/Space in native window, inspect VoiceOver/Narrator tree; retain focus vs selection distinction. |
| A2.4 Toolbar and reading-surface recipes; explicit overflow | H/R; current actual content+snapped-line predicate, `a2_toolbar_content_fit`4, corrected allocation5, type-update test, independent unchanged probes4 and adversarial5 | R1 closed. U/C: native resize, narrow/long/enlarged content and performance gates below. No performance refactoring authorized. |
| A2.5 Card/Callout slots passive by default | H; foundations/lifecycle/adoption/nested paint; passive6 goldens unchanged | U/C: real reader prose and nested focus/scroll; no whole-card interaction implied. |
| A2.6 Compact visible labelled source action and keyboard route | H; `source_is_visible_named_keyboard_reachable_and_host_validates_locator`, host stale/replacement tests; typed synthetic locator | U/C: Tab and activate Show source in synthetic gallery; check persistent name. Real source parsing/permissions/app reveal are D, not supplied by library. |
| A2.7 Operating gallery of exact subset | H; fixed synthetic host, actual Buffer editor, eight display-list states, two independently fresh identical runs; original32 UI tests preserved | U/C: launch explicitly authorized native gallery, record screenshots/input/AT receipt. Gallery is not Blooey and preview is not Live editing. |

## Specification §8 required evidence — exact numbered reconciliation

| Requirement | Completed evidence / qualification | Remaining gate / next action |
|---|---|---|
| 1. Core/default examples without opt-in; Color/transparent/text compatibility | H: core411/paint62 rerun with source/artifact identity checks, umbrella source checks, foundations override/legacy tests; GPU-free normal UI dependency tree | No uncovered headless requirement identified. Native/default-example visual compatibility remains U; owner can include a non-opt-in comparison when admitting native session. |
| 2. Tokens/overrides/themes/contrast | H: foundations resolved pairs + `a2_states` actual styles, Light/Dark/HC, explicit transparent/takeover/detach; focus gap/ring and non-color selection cues | U/C: real composited display/forced-color/preferences. Arbitrary caller palette contrast is unsupported, not certified by defaults. |
| 3. Owned lifecycle/stale handles/focused updates/editor identity/history/scroll | H: foundations/malformed/adoption tests; operating real typing/Undo/Redo, new independent editor/reflow test; unrelated scroll parity probe | U/C: native editor/IME and OS blur/capture matrix. No remaining scoped lifecycle defect reproduced. |
| 4. Mixed legacy/UI input/cancel/repeat/AT/cardinality/order | H: accepted A1 FIFO64 distinct requests/no dedup/overflow, conformance13, operating/core suites; no modern Fired fanout | U/C: real delivery/capture/blur + AT session, not synthetic-only events. |
| 5. Names/values/states/allowed actions/tab relations/no Shell/duplicate install | H: real AccessKit adapter2, core allowed actions (readonly denied; SetValue explicitly unsupported), idempotent plugin, source/dependency audit | U/C: actual OS bridge and reader tree/actions; app permission service is D. |
| 6. Actual bounds/wrap/targets/clips/long labels/nested scroll | H/R: actual paint/glyph/clip tests, geometry test64 configurations, original32 and finalUI41; corrected content/line/glyph oracles | U/C: installed fonts/physical DPI and window screenshots; 175 fractional survey is diagnostic configurations, NOT175 assertion tests. |
| 7. Deterministic fixed-font/theme/viewport gallery, scales and widths | H: six passive goldens; eight operating states (17 files with inventory) repeat and equal starting/worker/fresh outputs; ASCII builtin font, 1x/2x/fractional logical geometry | U/C: native fonts/shaping and real pixel scaling. 320 logical=1280/4 is only a400%-equivalent fixture, NOT physical400% window evidence. |
| 8. Native manual matrix, separately labelled per OS | U/C entirely at native level; shared platform-enum/core/headless tests do not satisfy this cell | Exact platform cells and admission requirements below. Missing native evidence prevents production/platform-accessibility claims; §8 outcome labels allow owner to assess a bounded A2 scaffold with explicit native gaps, not automatic acceptance. |
| 9. Revisions/files/commands/results/gallery instructions/resources | H for tested candidate; complete original report+commands, original267 manifest verified, retained238 payloads +29 external binaries, new standard manifest/inventory and final Git parent/child receipt | Complete this doc/evidence verification and commit; sender then assesses. Never run archived overwrite scripts in place. No repeat builds required for this reconciliation. |

## Other mandatory cross-cutting contracts (§§2–7 and Review Guide)

| Contract | Current disposition | Remaining gate / executable next step |
|---|---|---|
| Additive opt-in crate; core owns input/layout/focus/AT/editor; stable UiId/slots | H: source audit, tests; no second renderer/event loop/focus manager, no Shell filesystem install | Owner inspects preserved exact-source audit; no architectural implementation request uncovered. |
| Style precedence/explicit color, transparent values, re-theme/update/destroy | H: lifecycle/foundations/malformed/state tests; borrowed content detached alive, unsupported destructive edits rejected | Native/manual focused update check; arbitrary cyclic hostile worlds not exhaustively supported. |
| Focus2pt ring+2pt contrasting gap, strong boundary, selection shape, target32 comfortable/compact opt-in | H: resolved-state contrast and actual geometry/paint evidence; no generic target-size/platform certification | Native real adjacent-surface/clip/obscuration check at DPI/font scales; owner visual tuning remains open, not a reproduced defect. |
| Independent hover/pressed/selected/focus/disabled/readonly; unsupported state declarations | H: supported state matrix tested; D: busy/pending/toggle/checked/mixed/validation/default-action semantics not exposed by this subset | Do not show an unsupported state or claim a full widget catalogue. Future states require separately bounded scope. |
| Activation cancellation, keyboard ownership, AT matrix, source intent safety | H: canonical release/guard/queue tests; no action dispatch by resolver; host must validate opaque payloads | U: OS delivery. D: command payload validation is value-based, no monotonic ABA nonce promised; existing old probe observation is not a new security guarantee. |
| Reading measure/type/density/opaque-first/materials/motion | H: adjustable passive recipes and explicit theme/density scales; D: native glass/materials and animations deferred | OS preference propagation and platform font resolution U; do not infer them from explicit HC/theme controls. Inert no-animation scaffold does not establish OS reduced-motion integration. |
| Long translations, multiline, CJK, Arabic/RTL, emoji/combining, CRLF/BOM (§7) | H only for fixture/content retention and semantic names (`tests/support/gallery.rs`, `complete_gallery`); ASCII glyph assertions are bounded | U/C for real fallback/shaping/bidi/navigation. Native font gate must exercise these exact strings; presence is NOT localization readiness. No source bytes changed/parsed. |
| Licensed fallback/fonts/icons and platform accelerators | Existing public-domain font8x8/IBM VGA fallback documented in crate README; caller glyph and persistent name, no new asset bundle | U: actual platform font/accelerator rendering and licensing review if assets later introduced. No new assets or network retrieval here. |
| Editor/IME/source integrity, Undo/Redo, read-only Preview; no app migration | H: real core editor synthetic tests and source-backed boundaries; no parser/save/recovery/application changes | U: OS IME. D: Blooey adoption/Live editor/new components/materials/lifecycle are later work, not remaining implementation implicitly authorized here. |

## Actual outstanding findings and risks (not fabricated implementation work)

| Item | Classification/status | Executable next step and boundary |
|---|---|---|
| Previous P2 R1 equal-share / inner text inset / fractional snapping | R: independent scoped ACCEPT at f427; sender accepted that review | No duplicate R1 review. Include preserved negative controls and exact source identity in sender's full A2 assessment. |
| Extra whole-world layout/text work | Known P3 cost caveat;100 unrelated-label calls become200/300 in probes. N tentative nonempty rows permit at mostN+1 measurement walks plus App normal final layout. Native responsiveness UNVERIFIED | In an owner-authorized native session, measure resize/type/theme latency and input responsiveness with long/nested gallery content; record timing method and event counts. Escalate reproduced lag before any performance/source refactor; none authorized here. |
| Strict aggregate all-target clippy | Existing known tooling failures: `needless_borrows_for_generic_args` at `crates/rndr_core/tests/a1_wrapped_hug.rs:79,125` under `-D warnings`; UI-only strict clippy and format passed | Preserve failure (done); owner decides separate bounded test-only cleanup if a universally green aggregate lint gate is required. No lint fix/waiver fabricated. Earlier broad-green lint wording is not current all-target evidence. |
| Additional A2 implementation/review defect | None established by the latest scoped review or this documentary reconciliation; NOT exhaustive full-A2 re-review | Owner reviews complete A2 evidence/matrix; if a concrete new requirement defect is found, issue a bounded correction. Do not invent new product scope to fill native cells. |

## Native/manual matrix — no cell executed in this task

All platform/manual rows below are **U/C**: unverified, and execution is unavailable under this task's explicit no-launch/no-new-grant restriction. The last owner-decision row is **O**, not a native test. Cross-platform capabilities are not assumed. Test only synthetic content; never a real vault.

| Platform / exact gate | Capability/admission required | Executable next step after owner admits it |
|---|---|---|
| macOS window/visual/input | Authorized interactive macOS GUI session and gallery launch; current task expressly lacks launch authority | From exact source, reuse existing target, run the command below. Pointer down→outside release, Escape, focus change, OS blur/capture loss; keyboard-only all controls/manual tabs; resize wide/narrow/long/200%, selected+focus, nested scroll. Save command/HEAD/OS/scale/screenshots and result per action. |
| macOS editor/IME/native font/DPI | Same session plus installed text input method/fonts/displays | Type/compose/cancel multilingual text in Source; switch panels/themes, preserve selection/Undo/Redo; test CJK/Arabic/emoji/combining/CRLF-BOM fixture; verify native fallback and physical scaling, not logical fixture alone. Record inputs/font/engine/scale. |
| macOS VoiceOver/AccessKit | Explicit accessibility-enabled build, VoiceOver/OS inspection capability; no AT grant requested here | Inspect real roles/names/states/tab-panel relations/hidden exclusion and Focus/Activate/readonly rejection; confirm prose readable without added tab stops. Save native tree/action receipt. |
| macOS preferences/responsiveness/package | OS settings and timing instrumentation; package gate separately admitted | Change appearance/contrast/reduced-motion preferences and document supported vs absent propagation; measure cost caveat during resize/typing. Packaging/signing is NOT requested by this task; report unavailable until separately admitted. |
| Windows native window/input/fonts/DPI/preferences | Authorized Windows target/session/toolchain, not supplied here | Owner supplies target; run same synthetic gallery under Windows, record native build+OS, pointer/capture/blur/keyboard, IME/fonts/scaling and forced colors/preferences independently. No target/network/grants created here. |
| Windows Narrator/UI Automation | Authorized Windows AT session and actual bridge | Exercise names/actions/relations/hidden panels with Narrator/UIA and save receipt. macOS adapter2 is not Windows acceptance. |
| Linux X11 | Authorized Linux X11 session/toolchain and actual accessibility bridge/reader capability | Build/run gallery there; report build/window/input/capture/blur/IME/fonts/scaling/preferences and AT individually, including bridge unavailable cells rather than inventing parity. |
| Linux Wayland | Separate authorized Wayland compositor/session and AT bridge/reader capability | Repeat native matrix on Wayland; X11 result is not transferable. Record compositor/OS/backend/bridge and unsupported capabilities. |
| Owner visual tuning/full A2 decision/native integration | Sender/owner assessment and lifecycle authority, retained by owner | Read complete A2+this matrix+independent report, choose scoped A2 disposition with explicit native gaps. Only owner arranges later integration. No approval/merge/push/deploy performed or implied. |

### Concrete first next native command (instructions ONLY; not executed)

The smallest next gate is **macOS synthetic operating-gallery keyboard/pointer/resize plus responsiveness**, followed by VoiceOver and IME/font/DPI checks, not another headless R1 review. Owner must explicitly admit the native session first.

```sh
cd '/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library'
git rev-parse HEAD  # record exact doc child; implementation must still match f427/91c008
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
# Only after native authorization; existing target only, no clean/network/new target:
cargo run --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit -- --dark --hc --compact --narrow --200 --long
# Also run without flags for Light/comfortable/ordinary type/wide/short labels.
```

Do not run this command as part of the evidence/doc-only task. Native failure may expose a new bounded defect; it does not authorize source/lint/performance refactoring automatically. Full polished library, native platform parity, Blooey adoption and project terminal success remain distinct later outcomes.
