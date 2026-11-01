# A2 handoff evidence — HEADLESS implementation, fresh review required

**Authoritative production source: `71328489e435e0dc8ad1e678a10b2a6dfd9b57b4`.** The final evidence-only sealing commit is returned in the handoff; it changes no production/test/example/Cargo source. This continues the SAME undertaking from owner-accepted exact A0/A1 `04642fe03f7603f228eaf9b83611641a95676e8d`. The owner lifted A2's gate, not native certification or project acceptance. No approval, integration, push, external publishing, deployment or Blooey adoption was performed.

## Which receipts are final

Use **`handoff-*.log`**, **`handoff-counts.json`**, **`exact-handoff-ui/`**, **`handoff-source-commits.txt`**, **`handoff-build-inputs-sha256.txt`**, **`handoff-gallery-validation.txt`** and **`MANIFEST.sha256`** for this handoff. Earlier files containing “final” or “completion” in their names are preserved intermediate candidates; their source SHA/counts identify them, not their filename. They are not substituted for the final source.

The canonical new eight-case gallery is `gallery-a2-final/`. Its complete bytes match the final rebuilt UI's two fresh processes. These are **actual headless display lists, semantic trees and geometry inventories, not screenshots or native AT captures**. Original six passive goldens and historical evidence remain unchanged.

## Implemented result

- Primary/secondary/quiet/destructive Button; named IconButton; NavigationRow with independently selected/focused state and visible/accessibility metadata.
- Manual ModeSelector with real Tabs/TabPanel relations, explicit orientation, active-tab roving entry, arrows/Home/End moving focus only, Enter/Space activation and inactive subtree layout/hit/focus/AT exclusion.
- Toolbar row only when actual current resolved type fits; otherwise all actions stack visibly. No fake overflow menu. Reading layout uses existing measure/type recipes.
- Existing passive Card/Callout slots/defaults remain. Their adoption boundary rejects all operating ownership, including internal slots and plain wrappers containing an operating control; explicit plain adoption and borrowed teardown still work.
- Compact VISIBLE labelled Show source action; Tab then Enter/Space or pointer/AT produces only an opaque typed locator request. Host identity/revision/permission checks remain outside UI. The gallery tests independent stale-host revision rejection.
- A functioning synthetic-host gallery reuses core FIFO64, existing Buffer editor, existing layout/paint/focus/loop. Real typing, logical selection, Undo/Redo, mode/theme updates, source host validation, nested scrolling and actual glyph/clip/hit geometry are tested. No parser, real-document save/recovery/business logic or second editor/renderer/dispatcher is introduced.

Per-control purpose/props/slots, all keyboard/pointer/semantics/overflow/lifecycle/state/unsupported contracts and native launch/per-platform matrix: **`../../docs/design/A2-components.md`** from the repository root location (`docs/design/A2-components.md`). Current crate entry: `crates/rndr_ui/README.md`.

Necessary core/paint hooks are separate commits: opt-in manual roving/orientation, selected/disabled supplemental metadata, canonical pressed query, labelled-Tab descendant release, nested focus reveal, and opted-in resolved-color preservation. Legacy timing/Fired behavior, explicit color overrides, editor ownership and deliberate same-target FIFO multiplicity remain intact. Original A1 queue/cardinality production was not rewritten for the P3 fixture nit.

## Verification counts — no double counting

| Requested baseline category | Final checks | Detail |
|---|---:|---|
| rndr_core | **420** | 411 unit/integration + 9 doctests (baseline413; +3 promoted logical-only, +3 A2 core contracts, +1 nested focus) |
| rndr_paint | **63** | 62 unit/integration + 1 doctest (baseline62; +1 actual resolved-color/legacy paint test) |
| rndr_ui | **32** | Original14 unchanged +18 A2 tests: operating11, state2, nested paint1, malformed lifecycle2, adoption1, same-update type reflow1 |
| Actual AccessKit adapter | **2** | Actual adapter module source, not a reimplementation (baseline1) |
| **Comparable total** | **517** | Baseline490 +27 documented additions |
| Other renderer tests | **22** | Full renderer lib suite is24 with AccessKit, of which2 are the adapter tests above |
| **All Cargo checks counted** | **539** | 517 +22, not517 +24 |

All relevant Cargo suites pass. Umbrella **all-target/no-default** check and native gallery source check pass. Normal UI dependency tree excludes wgpu/winit/rndr_render/rndr_app/Blooey. UI is a dev dependency solely for the explicitly opted-in umbrella gallery example; Cargo.lock adds one existing workspace edge, no external version changes.

The operating geometry test covers **32** Light/Dark × standard/high-contrast × comfortable/compact ×100/200%-type ×320/1000-wide configurations with long labels and actual complete ASCII glyph coverage, real quads, target bounds, wrapping, selected/focus overlap and semantics. Actual resolved-control-state tests cover **96** appearance/contrast/variant/state combinations plus real doubled glyph geometry and reversible explicit overrides. The promoted logical-only registration fixture covers **80** genuinely `code=None` combinations; the archived independent8-test probe remains unchanged and is also executed directly. Counts for parameter iterations are not added to the test totals.

## Exact build/source/rlib identity — Cargo Fresh is not the proof

1. `verify-current.sh` built **all three rndr libraries directly from current source** at `4c8726f9789c4487c33abdc413b0ce2b97e7fa68`, ran complete unit/integration suites plus actual adapter, original4 and independent8 ownership probes, and the standalone descendant-Tab reproduction. It reconstructed exact accepted A1 source from Git and verified every archived byte against Git objects. Identical Tab-descendant and nested-focus fixtures fail on exact A1 with the expected assertion causes, and pass with A2. Baseline/source/rlib/binary hashes and raw transcripts are in `exact-current-source/`. Original invalid-cache/expected-failure evidence is preserved; no counterfactual Cargo cache was trusted.
2. Subsequent changes were UI-only (adoption boundary/current-style toolbar measurement), generator inventory sorting and current README documentation. **Every non-UI source/test/Cargo/actual-adapter input remains byte-identical** to that direct full-source receipt. The exact source-built core/paint rlibs, not Cargo rndr artifacts, are reused explicitly by absolute path.
3. **`verify-handoff-ui.sh` freshly rebuilt CURRENT UI source7132848** against those verified core/paint artifacts, then ran **all32 current UI tests**, including the last adoption and same-update reflow regressions. It rebuilt the current gallery generator and ran it twice in fresh processes. Complete display-list/semantics/geometry files match each other AND `gallery-a2-final/`. `exact-handoff-ui/` binds source-before/after hashes, source HEAD, actual rustc invocations, explicit dependency paths, binary hashes and the final library hashes below.
4. Combined exact direct verification is **507** library tests (411core +62paint +32UI +2actual-adapter), plus the **10 Cargo doctests** =517 comparable checks. The extra **13 direct core probes** (independent8 +original4 +standalone tab reproduction1) are conformance/reproduction evidence, not added again to517. Two expected A1 failures are not reported as passing tests.

Final explicitly used rlibs (SHA256):

```text
core   6155aae82332f2c878291d72e492ea426d904f734d4c8f59ab01a87e206ae4fc
paint  c2aea38975caa70ac72e1e38aa6695b99c110d0cf012191d14f1bcd2c68a9ab3
ui     e335d46e6d40e5bcb36894a6672d18a0cb0edc1331f418b6fb38f4b4b7f89a75
```

Identity manifest SHA256: **`069683cf4ce5453f073533df412c0f8a016316362b54c59d8e4e5b20517e12f9`** (`exact-handoff-ui/handoff-rlibs-sha256.txt`). Temporary explicit build/artifact locations are retained in the location files for independent inspection. No second Cargo target, broad clean/removal, network or custody workaround was used.

## Preservation and chronology

`historical-preservation-sha256.txt` verifies **274 original tracked files** byte-for-byte against exact04642, including all historical evidence, old UI tests/six passive goldens, original prior14, A0/A1 checkpoint, A1 decisions and both canonical proposals. Its SHA256 is **`c269267dd831608bbb0abc917e58ffd933a7ca69a898817e190341cbff90ccb6`**. The owner-acceptance tracking section is append-only; old checkpoints remain historical. The old public crate README is archived separately before its current scope/entry update.

- Specification remains `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b`.
- Review Guide remains `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`.
- Original independent REVIEW.md remains **`b738135187bdbe6f215b090589836d5731f65b4b052f5986c45825b168eaadd2`**. Small report, independent.rs and manifests were copied without replacement into `accepted-a1-review/`; original `/tmp/rndr-independent-04642.hmZHsh/` was not modified.
- P3 is a separate bounded test-only commit: physical=false explicitly sets code=None and asserts branch distinction. Original prior14 was not weakened/edited. Promoted independent cases are a new fixture; its80-case label was corrected without changing assertions.
- `nested-focus-before.log` and exact A1 negative controls reproduce the required nested reveal repair; after/current tests pass.
- `malformed-controls-before.log` records a missing-Layout validation failure and removed-label toolbar panic; both are fixed with strict regressions.
- `adoption-boundary-before.log` records passive adoption wrongly accepting operating ownership; the cross-family guard now rejects it while preserving plain adoption/borrowed teardown.
- `toolbar-type-update-before-400.log` is the valid same-update stale-type reproduction; after/current tests pass. The earlier300-wide fixture failed its initial-fit premise and is retained as an invalid fixture, NOT evidence of the target regression.
- Initial `gallery-cargo-vs-direct.diff` contains **metadata inventory ordering only** from hash-backed component iteration. The generator now sorts inventory entities. All initial/final DISPLAY LISTS are byte-identical; full final gallery files match Cargo and fresh direct processes. Earlier unsorted `gallery-a2/` is preserved, but is not the canonical final inventory.
- Earlier515/516-count reports and source manifests remain as intermediate receipts. The final authoritative counts are517/539 in `handoff-counts.json`, with the identity chain above. No historical log/report was overwritten to make a failure disappear.

## Reproduction commands

Run in this undertaking checkout with its existing target:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib
cargo check --offline --locked -p rndr --all-targets --no-default-features
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit
cargo tree --offline --locked -p rndr_ui -e normal
# Complete fresh exact-source reproduction; NEVER reuse an old output directory.
P=$(mktemp -d /tmp/rndr-a2-review.XXXXXX)
sh evidence/a2-20261101-mZVyJv/verify-current.sh "$P/fresh"
# Headless gallery generator also refuses an existing output directory.
cargo run --offline --locked -p rndr_ui --example operating_gallery -- "$P/gallery"
```

For a real window (instructions/source-check only here):

```sh
cargo run --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit -- --dark --hc --compact --narrow --200 --long
```

The existing Buffer source editor is synthetic and preview inert. Repeated Add requests remain distinct; Reset resets only the demo count; navigation selection, manual mode activation and Show source status respond through the host test double. It never touches a real vault. Resize, keyboard/pointer/AT, capture/blur and native accessibility must be tried on actual platforms, not inferred from these logs.

## Native and project limitations

**Native windows/screenshots, actual OS capture/blur, OS AT/preferences, native fonts/shaping/IME and physical DPI remain UNVERIFIED.** 320 logical width is1280/4 reflow, not a physical400%-DPI screenshot;200% type is separately exercised. Explicit library HC is not OS forced-color certification. macOS/Windows/X11/Wayland launch/testing boundaries are tabulated in `docs/design/A2-components.md`.

No menus/dialogs/materials, business/parser logic, rndr_app/lifecycle adoption, new renderer/editor/focus manager/event loop or external publishing were added. Existing default/legacy apps and editor ownership remain intact. A2 is a **reviewable implementation result**, not whole-project satisfaction. The owner retains fresh independent A2 review, corrections and native integration. This evidence does not approve or integrate itself.
