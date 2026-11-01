# Independent A2 review — REVISE

## Decision and authority

**REVISE before owner A2 acceptance. One P2/medium correctness-policy finding; no P0/P1 finding established.** The measured toolbar row decision is inconsistent with its actual equal-share layout for unequal labels. Existing green suites do not cover this case. No implementation, approval, commit, merge, push, or integration was performed. Owner retains correction, sender review and final acceptance/integration.

Review date: 2026-11-01. Evidence root: `/tmp/rndr-a2-independent-SQwoYo` (ephemeral filesystem evidence, not a durable repository commit).

## Exact identity and reviewed scope

- Candidate actually observed before and after execution: `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d`.
- Accepted A1 baseline: `04642fe03f7603f228eaf9b83611641a95676e8d`.
- Worker implementation reference: `71328489e435e0dc8ad1e678a10b2a6dfd9b57b4`. Independently checked intervening changed paths: evidence only (`seal-only-diff.txt`), not production/test/docs/manifests.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
- `status-before.txt` and `status-after.txt` are empty; HEAD unchanged. Independent probe/report/binary output exists only under the unique /tmp review directory.
- Complete canonical Specification, Review Guide, A1-api-decisions, A0-A1-checkpoint, implementation-tracking and A2-components read, using bounded follow-up reads after initially overlarge output. Canonical SHA256s match the requested values. Full actual baseline-to-HEAD source/manifest delta inspected, including operating controls, ownership/lifecycle and style hooks, core focus/interaction/access, opt-in paint mapping, real AccessKit adapter and native example, not merely tests or worker reports. `full-source.diff` saves the crates/Cargo.lock delta; `reviewed-file-inventory.txt` identifies all non-evidence changed paths. Full Git delta:269 files,73010 insertions,55 deletions, predominantly evidence. Historical receipts are preserved, not mistaken for final-source proof.

Requested reference hashes independently match exactly (`identity.sha256`):

| File | SHA256 |
|---|---|
| Specification | c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b |
| Review Guide | 8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0 |
| Worker README | f3a6747a2ce04adbf8525c23db1f7d1cc2f063a0704ecf9732a52d2fa285f20c |
| Worker MANIFEST.sha256 | 46edb394f2d5aa9b756fa4a789d22a8503df87f01f4174989decd3ed67bc915c |
| Worker exact-handoff-ui/handoff-rlibs-sha256.txt | 069683cf4ce5453f073533df412c0f8a016316362b54c59d8e4e5b20517e12f9 |

## R1 — P2/medium: toolbar total-width test permits rows whose longest control cannot fit

**Source:** `crates/rndr_ui/src/controls.rs:843–890`, particularly accumulated natural widths at873–878 and row choice884–888, together with Fill-width control/internal layouts in the same file. `resolve_toolbar` sums each label's unwrapped measurement plus52 and gap8, but core's row distributes available width equally to these Fill controls. A sum fit is not proof that every label fits its allocated share. The row/stack policy therefore makes the wrong decision for uneven labels, even though text measurement itself and the same-update style ordering work.

**Independent exact-current-source reproduction:** `adversarial.rs`, test `unequal_toolbar_labels_must_fit_each_row_share`; run commands in COMMANDS.md. Root1000pt, Toolbar1000pt, default theme; two buttons:

1. `This is a very long but valid command label with many words`
2. `Go`

After a single update:

```text
axis=Row natural_label_width=826
actual_label_rect=Rect { x: 9.0, y: 9.0, w: 478.0, h: 56.0 }
row selected even though longest label does not fit its allocated width
```

The compact single-test reproduction exits101 (`toolbar-reproduction.exit`); full new adversarial suite:4 pass,1 fail. Total estimated requirement966 fits1000, but equal distribution gives the long label478, causing wrapping in the purported fitted row. This violates the contracted measured row-to-stack choice; it does not demonstrate hidden/lost actions or unsafe dispatch. Severity is medium, not critical.

**Required correction outcome, not an implementation:** make the fit predicate match actual child allocations/insets, or allocate measured widths consistently; otherwise choose a stack. Add unequal-label regressions, including same-update font-scale/width/label changes and mixed compact/long commands. Preserve all targets, labels, focus, and current same-update style ordering. Re-review exact corrected source rather than accepting current green counts.

## Independent execution and count reconciliation

All builds/checks used offline locked Cargo, the existing target, jobs2, debug0 for dev/test and incremental0. No second Cargo target, cleanup, network, secret access, historical overwrite, or native activation. Initial disk approximately1.7GiB free; final approximately1.2GiB; target remains1.3GiB. Independent outputs approximately484MiB. No resource blocker or disk exhaustion occurred.

Scripts were inspected before invocation. Exact commands: COMMANDS.md. `exact/transcript.log` records explicit rustc source and rlib paths; source-before/after, dependency rlib hashes and binary hashes are retained. `verify-current.sh` rebuilt core, paint, and ALL final UI source, not an intermediate UI rlib. `verify-handoff-ui.sh` rebuilt final UI again in a distinct directory using independently rebuilt unchanged core/paint. Both exited0; actual adapter code is compiled through its real source wrapper, not a substitute AccessKit model. This is independent execution of audited fixtures, not reliance on the worker verdict/logs or Cargo Fresh.

| Category | Independently passed | Composition |
|---|---:|---|
| core |420|411 non-doc +9 doctests|
| paint |63|62 non-doc +1 doctest|
| UI |32|existing unit/integration suites|
| actual AccessKit adapter |2|two `a11y::tests` in real renderer source|
| comparable total |**517**|same categories as requested|
| other render library |22|outside comparable total|
| all Cargo tests |**539**|515 core/paint/UI +24 renderer|

`cargo.log` and `cargo-counts.json`: zero failed/ignored tests. Umbrella `--all-targets --no-default-features`, native `ui_gallery` source check with AccessKit, and normal UI dependency-tree check pass. Two actual adapter tests are `manual_orientation_selected_and_disabled_matrix_reaches_accesskit` and `actions_descriptions_and_real_panel_relations_match_dispatch`.

Direct source-linked rerun: **507 =411 core+62paint+32UI+2adapter**; doctests are separately covered by Cargo. **13 existing conformance probes** pass separately, including original4+independent8+Tab-release1. Two expected A1-baseline reproductions fail as expected and are retained separately, not counted as passing final tests. Direct tests repeat Cargo coverage: DO NOT add507 to539, or add the13 probes to the comparable517. Final UI repeat32 is another run of the same32, not32 additional tests.

Reviewer-authored additions: five adversarial tests (4 pass,1 expected-to-be-fixed failure R1), plus one geometry/semantics test exercising64 configurations. This matrix is one Rust test, not64 extra suite tests. Its initial harness mistakenly expected128 for six binary dimensions; all body assertions completed, then the count assertion failed. Original source/log retained as geometry-initial.*; corrected count only64 passes. That initial harness count failure is not a product defect and is not silently discarded.

## Contract audit findings beyond R1

- Four Button variants are implemented on the canonical core Button/action path, named IconButton rejects empty names, NavigationRow selection is independent from Focus with visible/announced metadata. Focus/selected overlap retains both cues in the headless painted/semantic evidence.
- ModeSelector uses actual Tabs/Tab/TabPanel and relation entities; horizontal/vertical manual policies route arrows/Home/End to focus only, Enter/Space activate, active entry roves. Hidden inactive descendants are excluded from layout/hit/focus/access tree while borrowed editor state remains alive. Disabled/readonly behavior and allowed access actions are covered in core/UI and actual adapter tests. Manual hooks are opt-in; no parallel dispatcher or Fired fanout was introduced.
- Lifecycle/style/update/destroy/stale/malformed and adoption-boundary cases are checked through implementation plus independent reruns. Additional new tests confirm malformed reparented panel destroy rejects without partially deleting the old tab; disabled active roving entry and hidden nested target reject inappropriate focus/activation; same-update disable/reenable cancels an armed Space release.
- FIFO64 distinct same-target multiplicity and explicit overflow tests pass from freshly linked core. Legacy immediate and marked canonical pointer/keyboard/AT routes are kept together. Nested focus reveal and descendant Tab-release repairs reproduce against A1 and pass current source. Pressed styling is an additive read-only projection from canonical arm state, not another activation producer.
- Current payload validation is value-based: the new ABA observation test shows a previously drained command request can validate if command payload changes away then back. This is not claimed as a new contract violation; command requests have no monotonic generation nonce. Source locators carry explicit document/revision/anchor tokens and host must revalidate permission/current identity at consumption. The test name contains “invalidates” but its actual asserted observation is acceptance after restoration, documented here to avoid implying a security guarantee it does not test.
- SourceAction has persistent visible `Show source`, keyboard instructions, Tab and Enter/Space/AT route, and opaque typed locator; no parser/executable locator or hidden hover-only action. Gallery checks current UI request plus fixed synthetic host locator/command allow-list. This demonstrates a synthetic authorization decision, not a real application permission service.
- Operating gallery is rndr-only synthetic host state, with existing Buffer/Editor, core logical selection/history and nested Scroll ownership. Existing real typing, mode/theme updates, selection, undo/redo and scroll-preservation tests pass. No business parser, default app, Blooey, save/recovery adoption, or lifecycle integration is introduced. Native launcher is an opt-in example/source check only.
- Passive Card/Callout defaults stay noninteractive with explicit slots; adoption rejects owned internals across passive/operating families. Reading recipes and theme/scale changes are inspected and headless exercised. Normal UI dependency tree stays GPU-free; GPU/window renderer dependencies are dev-only for examples, not normal UI dependencies.
- P3 physical=false correction is bounded test-only and explicitly sets code=None, with an assertion distinguishing branches; promoted logical-only cases use a new fixture. Original prior14 and all264 preexisting evidence files independently match accepted A1 Git blob identities. All274 historical-preservation SHA entries also verify. No canonical proposals/checkpoint/history were rewritten.

## Independent visual/semantic evidence, not screenshots

Fresh final UI generator was executed twice. All eight final display-list/semantics-geometry states match each other and sealed final galleries byte-for-byte (`final-ui/gallery-repeat.diff` and gallery-established.diff empty). Actual glyph/quad counts:

| State | Quads | Glyphs |
|---|---:|---:|
| light-wide |691|647|
| dark-wide-preview |746|702|
| light-narrow-200-long |747|703|
| dark-compact-200-long |802|758|
| high-light-200 |579|535|
| high-dark-compact |695|651|
| reflow-400-equivalent |780|736|
| wide-long |867|823|

All six passive goldens independently regenerated from current-source-linked core/UI/paint and compare byte-identically (`passive-comparison.log` empty). Additional independent64-state test crosses Light/Dark, standard/high contrast, both densities,1x/2x type,320/640pt widths, short/long labels. It validates target minima and horizontal bounds, label-within-target geometry, full ASCII glyph sequence and at least one real glyph paint quad within each control label, focused+selected semantic coexistence, passive non-interaction, and inactive editor focus/AT exclusion after panel activation. Existing nested paint/focus clipping and 32-combination long-label tests independently rerun as part of UI32. These checks are bounded geometric/semantic observations; they do not certify every pixel or every Unicode glyph.

## Limitations, preserved failures, and next owner step

- **REVISE R1 before A2 acceptance.** Corrective source change/review is owner work, not performed by this reviewer. Current passing golden configurations use predominantly similarly long labels and miss uneven equal-share width allocation.
- No native windows, screenshots, capture/blur surfaces, OS AT client, OS preference propagation, installed fonts/complex shaping, IME, physical DPI, or Windows/X11/Wayland execution were exercised. All remain **UNVERIFIED**. Headless library HC is not native high-contrast or accessibility certification.320pt is only a400%-equivalent reflow model.
- Actual AccessKit means its adapter mapping/unit tests, not a screen-reader/OS bridge session. Native launcher check means compile/check, not launch/native usability proof.
- Arbitrary malformed cyclic external Worlds, hostile host implementations, all scheduling interleavings, real permission service, every font/script and every paint pixel are not exhaustively proven. Tests do not replace the explicit source audit or owner sender review.
- Worker manifest has239 valid hash/path entries but uses single-space syntax rejected by standard `shasum -c`. Raw rejection log preserved; independent parser validates239/239 and historical274/274 with no mismatches. This is an evidence-tool compatibility observation, not evidence corruption or an additional blocker.
- Evidence is in bounded /tmp storage: owner must preserve it via their normal evidence handling if durable retention is needed. No historic worker receipts/intermediate failures were removed. No further builds are needed for this verdict.

**Next:** owner reviews R1 and the supplied failing source probe, arranges the bounded correction plus fresh exact-source regression check, then determines A2 acceptance. No approval/integration is authorized by this report.
