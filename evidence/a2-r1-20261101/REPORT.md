# Bounded R1 correction — ready for FRESH INDEPENDENT re-review, not A2 acceptance

## Outcome and authority

Corrected the one accepted P2 R1 finding in the SAME authorized rndr UI undertaking. Owner retains orchestration, independent review, A2 acceptance and integration. No approval, merge, push, production change, network/secrets, native launch or OS certification was performed.

- Start verified clean: `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d`.
- Initial source commit: `e3847d23fe1902af4c0d93ad6122d187fa9e821d` (full identity in `initial-exact-run/implementation.txt`), **not final**: focused regressions passed but full UI run found conservative narrow-policy change.
- Final implementation/test/docs SHA: **`aba338dfa77a50cd92b6b182fe78074584a20841`**. All successful final-source verification was against this SHA.
- Subsequent seal adds only this new evidence directory. Exact seal SHA is returned in the handoff, not self-referentially embedded here. No implementation changes are hidden in the evidence delta.

Complete canonical Specification, Review Guide, A2 contract, API decisions, checkpoint and implementation tracking were read; oversized output was followed with bounded missing-range reads. Canonical SHA256s remain c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b (Specification) and 8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0 (Guide). Complete independent report, commands, adversarial probe and toolbar failure log read before changing source.

## Correction and preserved contracts

`controls::resolve_toolbars` still resolves after current labels/styles and before core layout. It now computes each direct visible child's equal Fill share, subtracting toolbar padding and gaps, and requires the measured unwrapped label plus the actual owned root/gap/border/face/label horizontal insets to fit that share, including target minima. Unknown/malformed direct visible children still force a stack; hidden children consume neither gap nor share. Empty/single cases stay supported. Nonfinite toolbar widths remain rejected; finite narrow layouts remain finite and stack without hiding actions.

The original conservative sum-width budget (natural label +52 per control) is retained as an additional veto. It preserves existing narrow-row policy; it is NOT used as evidence that equal shares fit. Thus every chosen row meets BOTH the old budget and the actual per-share constraint. Exact actual per-share boundaries pass the new check; the conservative budget may still stack a fitting compact group. No new measured-width allocator or core layout change. No spec/test loosening: all32 existing UI tests are byte-unchanged.

Only production source changed is `crates/rndr_ui/src/controls.rs`. Added one permanent fixture `a2_toolbar_allocation.rs` (five Rust tests), and a bounded contract note in A2-components.md. No core/paint/render source, Cargo manifest/lock, plugin ordering, dispatch/focus/semantics/activation, target styling, label conversion, hidden/unknown policy or unrelated app code changed.

New tests cover uneven long/short labels; mixed icon and visible source actions using actual resolved type; same-update width/font-scale/label mutations both row→stack and stack→row; comfortable/compact density and100%/200% UI type; below/exact/above fit boundaries in both directions; identity, focus, labels, names and typed action payloads; finite32pt narrow geometry; empty/single, hidden/unknown and toolbar padding. Original independent R1 assertion is promoted, not substituted with reproduction-specific label logic.

## Before / after evidence

Identical final five-test fixture linked against independently rebuilt starting UI source:
**0 pass /5 fail, exit101** (`baseline-regressions.log`). Final corrected ALL-source UI:
**5 pass /0 fail**, repeated in the complete UI suite and `checks/logs/final-regressions.log`.

Independent unchanged original adversarial suite: originally4 pass /1 fail; now **5 pass**, including:

| R1 root1000pt | Baseline | Corrected |
|---|---|---|
| axis | Row (incorrect) | Column |
| natural long-label width |826|826|
| actual label width |478|982|
| label Rect height |56|36|

The original review report SHA256 is **72adad8d87b439554d121029aa66a0383fbab18f12a742f43d90de577985552c**. Original manifest SHA256 is **3a188e10c26f1d2052e1a3b88f22f6fb1e394dfcb3140ce180a5599a76b76556**, independently revalidated404/404 original entries. Original `/tmp/rndr-a2-independent-SQwoYo` remains untouched. Small report/commands/probes/pertinent logs and textual identity/gallery receipts are durably copied byte-identically under independent-review/ (305 files); its original manifests refer to the ORIGINAL full output, not a claim all binaries were copied. This new directory's standard manifest covers exactly the durable subset.

## Exact-source verification and counts — no double counting

Both prior verifier scripts inspected completely before builds. New runners refuse old output, use only existing Cargo target and explicit independent rlibs, and do not overwrite history. ALL final UI source was rebuilt directly with rustc at aba338d, including its controls/style/tokens/surfaces modules, every UI test and gallery fixture. Cargo Fresh is not relied upon for source identity. Complete source-before/after, explicit dependencies, binary hashes, commands and statuses are preserved.

Independently rebuilt unchanged artifacts reused only after checking ALL their recorded non-UI inputs and rlib/binary hashes. Their source identities also checked against baseline Git objects. Relevant source-linked core/paint/actual-adapter binaries were **re-executed**, not merely credited with the prior review's results.

| Category | Passed now | Counting boundary |
|---|---:|---|
| Cargo core |420|411 non-doc +9 doctests|
| Cargo paint |63|62 non-doc +1 doctest|
| Cargo UI |37|32 existing +5 new|
| Actual AccessKit adapter |2|included in24 renderer tests|
| Comparable total |**522**|420+63+37+2|
| Other renderer tests |22|outside comparable total|
| All Cargo tests |**544**|520 libraries +24 renderer|
| Direct source-linked rerun |**512**|411 core+62 paint+37 corrected UI+2 actual adapter; repeats Cargo coverage|
| Prior conformance probes |13|separate; not added to comparable522|
| Original independent adversarial |5|includes the promoted R1 case; do NOT add as five unique new product regressions|
| Independent geometry matrix |1 test,64 configurations|not64 extra test cases|

All final suites above have zero failed/ignored. New five-test focused rerun repeats UI coverage, not five more. Cargo check umbrella/all-targets/no-default and real native gallery source/AccessKit check pass; GPU-free normal UI dependency tree remains core-only. Targeted rustfmt passes. `counts.json` was parsed and asserted against logs, not worker claims. Exact command list and runner/transcripts in COMMANDS.md, exact-ui/, checks/.

Artifact SHA256s:

- Verified independent core rlib: `6155aae82332f2c878291d72e492ea426d904f734d4c8f59ab01a87e206ae4fc`.
- Verified independent paint rlib: `c2aea38975caa70ac72e1e38aa6695b99c110d0cf012191d14f1bcd2c68a9ab3`.
- ALL corrected UI rlib: `67c2e5b4fe93182d92ff494753955541fc7ed1ae16235899e0db4b9b42e86494`.

Full baseline/final source and per-binary identities are separate standard/legacy receipts, not abbreviated SHA assertions. Baseline source receipt was verified against exact starting Git blobs, and original baseline rlib selected explicitly. No second large Cargo target, cache invalidation or cleanup needed.

## Gallery truth and historical preservation

Fresh final-source generator processes run twice: all eight complete states repeat byte-identically. New outputs are `exact-ui/gallery-one/`; old sealed galleries were NOT changed. Only **light-wide**, **dark-wide-preview**, **high-dark-compact** differ from original final galleries (both display-list and semantics/geometry files): corrected toolbar selection changes geometry; glyph/quad counts and inventory remain the same. The other five states and inventory match prior files byte-for-byte. Full diff retained in `exact-ui/gallery-established.diff`, exit1 explicitly denotes expected differences, NOT failure. Repeated new-gallery diff is empty/exit0.

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

All **six passive goldens** freshly regenerated against corrected UI/verified independent core+paint and remain byte-identical. All **504 pre-existing evidence Git blobs** at31006f0d were verified unchanged, including prior sealed historical/failed/intermediate receipts. `historical-preserved.sha256` records them; no historical overwrite or broad cleanup.

## Resources and retained failures

Preflight1.2GiB free; original independent output484MiB and existing target1.3GiB. Chosen bounded direct-UI path estimates <200MiB new temporary artifacts, not another484MiB full-source build; compact durable receipts ~8MiB, no binaries. During work free disk independently rose to3.2GiB without any deletion/cleanup by this executor; later measured3.1GiB. Existing target remains1.3GiB. Temporary R1 build directories total about129MiB. No second Cargo target, removal, network or secrets. Jobs2, debug0, incremental0; Cargo always offline --locked. Initial exact-run failure and artifacts were retained, not recycled/overwritten.

Two honest nonfinal failures are preserved:
1. Initial new fixture compile used invalid `Parent.0`; corrected to actual `Parent.entity` before baseline/final test executions. Compile log retained.
2. Initial e3847d2 exact-source UI run failed unchanged old3xGo160pt expected-column policy after dropping the original conservative budget. Corrected production to retain that budget plus the new safety constraint; unchanged test now passes. All partial transcripts/logs/source identities remain under initial-exact-run/. It is not final evidence or an outstanding final failure.

## Limitations / next owner action

Fresh independent re-review of the exact sealed correction is required before A2 acceptance. This is an implementation/evidence handoff, not approval or integration. No native launch, screenshots, real capture/blur/pointer delivery, OS AT client/preferences, fonts/complex shaping/IME, physical DPI, Windows/X11/Wayland execution or platform certification. All remain **UNVERIFIED**. Headless geometry/glyph/accessibility adapter mapping does not certify pixels, scripts, native usability or OS accessibility;320pt remains only a400%-equivalent logical reflow model. Synthetic host authorization remains a test double, not real app permissions. No Blooey or core scope expansion.
