# Markdown slots + theme bridge — implementation evidence

Baseline: `e46a3205bdff534278f5281d63b48d5039f3a9dc`.
Branch: `worktree/rndr-markdown-host-slots-and-theme-bridge`.
This report and both manifests belong to the containing implementation commit;
obtain its exact ID with `git rev-parse HEAD`. Sender review is still required.
No approval, integration, push, publication or deployment was performed.

## Delivered

- Public core `mount_markdown_slot`, `MarkdownSlotId`, `MarkdownSlot` and typed
  errors, re-exported at the core root and therefore the existing facade core
  namespace. Last-synced positional/content-checked bindings own retained real
  ECS subtrees. Expected snapshots/pending source, duplicate/reused IDs, invalid
  heights and stale handles are checked. No new syntax/interpreter/dependencies.
- Markdown reconciliation retains later host subtrees when an earlier block
  changes shape, preserves document child order and yields pointer-selection /
  copy shortcuts to embedded controls. Normal parsing/streaming remains core's.
- UI-layer pure `UiTheme::core_theme` / `markdown_theme` plus explicit per-world
  `apply_theme_bridge`. It replaces UI/core/markdown/default-reading-style
  resources only when the host calls it, without a plugin watcher or core->UI
  dependency. High-contrast text selection uses a documented Focus overlay
  because semantic Selected alone equals the content surface in that mode.
- Retained markdown palette/typography invalidation was corrected for ordinary
  paragraphs as well as styled blocks. Core `style_of` accepts an opt-in world
  TextStyle, preserving the old TextFace fallback when absent. Core Pane owned
  fills refresh without rebuilding editable rows or reacquiring observed host
  overrides (including takeovers equal to a future palette).
- Executable `markdown_gallery`: timed streamed tokens, a live value slot,
  actual increment/theme Buttons in a control slot, focus/keyboard interaction,
  narrow resizing/scroll clips, optional observation and headless capture. CI's
  semantic-gallery build includes it. Shared fixture is tested, not a second UI.
- [Public contracts, examples and limitations](../../docs/markdown-host-slots.md)
  and `scripts/verify-markdown-slots.sh` provide exact reproduction instructions.

## Final serial verification

All Cargo commands used the existing shared target, offline locked dependencies,
`CARGO_BUILD_JOBS=1`, and the established profile settings from
`scripts/data-gallery-env.sh`. There were no new target directories or parallel
heavy builds. The authoritative script run was:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
RNDR_CAPTURE=1 scripts/verify-markdown-slots.sh evidence/markdown-slots/final
# For reproduction choose a NEW output directory, never overwrite this evidence.
```

`final/commands.log` records each exact command, free KiB before each gate and
PASS/FAIL. Each command has its own bounded log. Terminal trailing blank lines
were removed when packaging logs for Git whitespace checks; result text is unchanged.

| Log / gate | Result |
| --- | --- |
| `final/ui.log`: `cargo test --offline --locked -p rndr_ui --tests` | 193 passed |
| `final/core.log`: core markdown/layout/text_engine/editor/editor_navigation/editor_drag/a1_interaction/pane targets | 89 passed |
| `final/parser.log`: core lib `markdown` | 8 passed |
| `final/highlight.log`: core `--features highlight --test markdown` | 12 passed |
| `final/paint.log`: `rndr_paint --tests` | 62 passed |
| `final/capture.log`: renderer `--features observe --lib capture::tests` | 20 passed |
| `final/gallery.log`: observe-enabled executable gallery build | Passed |
| `final/fallback.log`: gallery `--no-default-features` check | Passed |
| `final/docs.log`: `rndr_ui --no-deps` rustdoc | Passed, no warnings |
| `final/light.log`, `final/dark-narrow.log` | Actual offscreen captures + nine privacy checks each |

**384 passing Rust test executions, zero failures**, counting the focused and
feature configurations as executions rather than claiming 384 distinct tests.
The new integration fixtures contribute 12 tests (nine slot tests, three gallery/
core-theme tests), plus one embedded-control capture regression. Existing tests
cover related canonical focus/activation/layout and capture safety behavior.

Supplementary gates run after the script:

```sh
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
cargo clippy --offline --locked -p rndr_ui --tests --no-deps -- -D warnings
cargo doc --offline --locked -p rndr_core --no-deps
```

Both exited 0 (`final/clippy.log`, `final/core-docs.log`). Core docs have **four
existing unresolved `Fired` links in unchanged `button.rs`**; this is not a
warning-free core rustdoc claim or a strict whole-workspace clippy/MSRV gate.

### Tested contracts

Streaming character append and full reparse preserve retained root/child state;
shape changes before a slot preserve it when its index/content is unchanged.
Tests cover duplicate and reused identities, identical blocks at distinct
indexes, stale expected content / pending parsing / invalid heights, direct
root deletion, detach and fallback restoration, replacement/removal cascade,
view teardown, dead-handle rejection, existing control focus/Enter/Space-release
activation, no markdown selection on embedded press, copy of fallback rather
than live values, fixed-height changes, width reflow, scroll clips, all core
palette fields, light/dark/high contrast and typography updates, editor/default
label drawing, explicit style precedence, retained editable Pane state and
world isolation.

### Real pixels, metadata and privacy

- `final/light.png`: **760 x 660** actual offscreen GPU readback.
- `final/dark-narrow.png`: **340 x 440** actual offscreen GPU readback.
- Each has a same-frame text-free JSON metadata companion with **41 nodes**,
  including actual embedded Button UiIds 10 and 11. These are not raster controls
  standing in for interaction: the underlying retained Buttons are independently
  focusable/activatable and covered by the fixture's keyboard tests.
- Both final PNGs were opened with `cognition_image_view`, and their actual
  pixels were visually inspected: readable light/dark content, the focused
  increment Button's outline, narrow text wrapping and viewport clipping are
  visible. Inspection is limited to these two headless states. The image tool
  may re-encode its viewing rendition; manifests hash the original saved files.
- After each capture, the executable checks sensitive slot roots, sensitive
  control descendants, hidden sensitive content, unrelated component targets and
  explicit Deny: **18 total actual capture-denial assertions** across both runs.
  The deterministic renderer regression also proves denial precedes identity
  lookup and text/action strings are absent from geometry metadata. Existing
  renderer coherence/image-budget/privacy regressions remain passing.
- Observation still requires an all-content Public attestation. Hosts must deny
  or mark new sensitive live values; no automatic secret detection or partial
  masking was introduced. Core/UI do not depend on the observation renderer.

## Capacity and integrity

Initial inspection: **1.4 GiB free** (owner reported 1.5). After the serial gate:
1.3 GiB; after supplementary docs/lints: approximately **1.2 GiB**. The script
refuses builds below 750 MiB. No user caches, source, worktrees or history were
deleted. Only this undertaking's redundant intermediate generated evidence was
omitted from the final bundle. The committed images are two small PNGs, not large
capture dumps. No dependency or lockfile changes were needed.

`source.sha256` covers every changed/new non-evidence file relative to the named
baseline; unchanged source is pinned by that baseline. `evidence.sha256` covers
this report, the source manifest and every final log/PNG/JSON, excluding itself
(to avoid a self-hash cycle). From the checkout root:

```sh
shasum -a 256 -c evidence/markdown-slots/source.sha256
shasum -a 256 -c evidence/markdown-slots/evidence.sha256
git diff e46a3205bdff534278f5281d63b48d5039f3a9dc --stat
```

## Intentional limitations and unverified gaps

Bindings preserve identities only while the **same index and semantic block
content** remain. No fuzzy relocation, source-edit migration, automatic Hug
sizing, slot virtualization or borrowed-child contract is promised. Identical
same-index replacements deliberately preserve state; IDs cannot be reused in
one view. Slots own complete block bodies, including decorations; source copy
still uses fallback markdown. Host durable data and external portals/resources
remain host responsibilities. See public docs for complete lifecycle rules.

The bridge owns four explicit world resources, chooses Reading typography and
legacy radius 8, and does not restyle arbitrary static host Color overrides.
UiPlugin alone remains backwards-compatible and does not opt in. There is no
process-global theme, OS appearance integration, or native font installation.

Native interactive window testing, OS compositor presentation, native screen
readers/AT, IME and complex scripts, Windows/Linux and MSRV remain **unverified**.
Headless pixels use the existing builtin bitmap engine, not native startup's
Parley upgrade. The no-default gallery was compile-checked, not launched. No
native overlay/platform certification, registry publishing or unrelated repository
work was undertaken. Sender should independently review before native integration.
