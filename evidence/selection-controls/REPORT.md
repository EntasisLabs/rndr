# Checkbox and single/multi Select — implementation handback

## Candidate / authority

- Baseline: `c9536acba3914a3be39c53c6bffe42a9b9c40c42` (clean, already-integrated DataTable/PropertyInspector).
- Source commit: `19f553c0d57339ee666d9752dedca861331209f1`.
- Branch: `worktree/rndr-checkbox-and-single-multi-select-controls`.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-checkbox-and-single-multi-select-controls`.
- The final evidence-only commit leaves this capture-bound source unchanged; the final handback supplies its exact HEAD and verified clean status.
- Completed implementation candidate for owner assessment, **not sender acceptance, integration or visual sign-off**. No merge/apply/approve/push/deployment/publishing, credentials, spend, child worker, broad cleanup or other-project changes. Blooey was not touched.

## Delivered

The audit and bounded public design are in `docs/selection-controls.md` (written before implementation, expanded with APIs/commands). Independently reusable `rndr_ui::selection::{Checkbox, SingleSelect, MultiSelect}` use host-owned state and stable `OptionId`s. Checkbox exposes two-state checked semantics; no mixed-state claim. Selectors expose actual available-option popups, not cycling buttons. Disabled options remain exposed but are skipped by navigation/activation. Highlight/focus is separate from selection. Multi toggles stay open with a readable summary and explicit immediate-request/no-rollback-on-close policy. Reconciliation safely handles stale/reordered/removed IDs without changing application records. Hosts consume `SelectionChange` or install synchronous `ChangeHandler`, not both.

Core `AnchoredPopup` uses canonical chronological interaction and accessibility dispatch: viewport-aware above/below placement, constrained Scroll, one scope, Escape/Tab/outside dismissal, focus ownership/return, inherited readonly/disabled and modal/layer hit blocking. Outside contact is swallowed through release, including dismissal during policy invalidation. `FocusOwner` links a detached popup to the inspector property's existing focus boundary. No parallel dispatcher or new dependency.

PropertyInspector Boolean and Choice APIs remain, now composed with these controls. Additive `MultiChoice`/`Choices` supports multiple values, still draft → validate → explicit CommitRequested. Invalid but well-typed selection drafts remain visible. Application-owned records are never silently edited. Expanded tests found and repaired two edge cases: disabling a focused Commit now transfers focus to its editor within the property (preserving pending acknowledgements); placement remains constrained even if reflow/scroll temporarily moves an anchor offscreen. Existing transactions still pass. Two old choice-cycle fixtures now explicitly open/navigate/choose rather than assume cycling.

Supported semantics include Checkbox checked, ComboBox expanded/controls, ListBox multiselect/labelled-by, Option selected, names/descriptions and inherited access admission. AccessKit projection is tested. New state/relationships are included in observation metadata and post-paint mutation binding. **Not a live AT claim.**

## Verification

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
export CARGO_BUILD_JOBS=2
bash scripts/verify-selection-controls.sh evidence/selection-controls/verification
```

Exit **0**. Full commands/results: `verification-driver.log` and per-command logs. The new gate runs the existing `verify-data-editors.sh`, not a replacement/bypass.

| Final successful group | Passed executions |
|---|---:|
| Core + paint + UI, including maintained inspector/selection and doc tests | 627 |
| Historical original six + activation-policy fixtures | 8 |
| Native renderer/observation/AccessKit | 76 |
| Fallback renderer | 25 |
| Facade native + fallback | 1 + 1 |
| **Rust total in this gate** | **738** |
| Python tools tests | **20** |

Formatting, whitespace, native/fallback data-gallery checks and native/fallback selection-gallery checks also passed. Separately, the focused selection command passes **19 tests** (13 selection, 3 scope, 3 gallery), including a **24-case width/text-scale/preset matrix inside one gallery test**. Those executions overlap the full gate: do not add them to 738 or call 738 unique tests. Development reruns/failures are preserved in `development/` and excluded from final totals. Details: `test-counts.json`.

Coverage: actual queued key order, Enter open/navigation/selection, Space press/release, repeat/synthetic/IME snapshots, scope dismissal/traversal, pointer release selection, outside dismissal/no click-through, accessibility background rejection, higher-modal focus, disabled/readonly/hidden inheritance, window blur, empty/all-disabled states, many-options constrained scroll, stable reorder/removal, teardown, inspector draft validation/explicit commit/delayed acknowledgement, and existing editor/data transactions. These are synthetic framework event/pointer snapshots, not physical OS input.

## Native evidence / pixel limitation

```sh
bash scripts/capture-selection-controls.sh evidence/selection-controls/native
```

Exit **0**. **12 presets / 24 native PNG frames**: light, dark-single, multi, narrow-multi, compact/high-contrast/200%-multi, invalid-inspector, readonly, disabled, empty-popup, many-popup, inspector-multi, narrow-inspector. Both frames in every requested-open preset contain a ListBox in that frame's semantic metadata. Pairs have increasing revisions, normal timed shutdown and observation-session cleanup.

`native/receipt.json` binds source commit, binary SHA-256, **276-file source manifest**, frame PNG/metadata hashes and commands. The final audit verifies unchanged source/frame/binary hashes. Provenance: **macOS Metal / Parley / live-surface-copy-before-present**. Not after-present/compositor confirmation; source-defined synthetic model/focus presets are not physical input evidence.

**Appearance is not signed off.** One direct `cognition_image_view` call on `native/light/frame-2.png` returned ok=true, vision_available=true, dimensions/hash/artifact metadata — but **no inspectable raster pixels reached this model input**. The flag does not prove inspection. `pixel-attempt.json` records the exact result and limitation. No identical retries or invented visual observations. The owner can inspect the real gallery/PNGs later.

## Preservation / disk

`integrity-audit.json` verifies **all 3,534 pre-existing tracked evidence files** byte-for-byte against baseline, no changes. All new evidence is isolated here (about 3.9 MiB before this report). Source/binary/frame audits pass. No old evidence/history was deleted or rewritten. The pre-existing `evidence/.gitignore` ignores nested `native/` directories, so this undertaking's 87 native capture files are explicitly force-tracked, without changing that ignore policy. Source formatting/whitespace gates passed; immutable raw command transcripts retain logger-generated trailing spaces and terminal blank lines, which produce evidence-only `git diff --check` warnings when staging the logs. These are not source failures and were not scrubbed from receipts.

Rust/Cargo 1.98.0; offline locked builds, retained compatible cache, two jobs, incremental=0, dev/test debug=0. Free disk: about 6.6 GiB initially, 6.4 GiB after tests/captures. New scripts enforce a 2 GiB stop floor and do no cache cleanup. No redundant target was created.

## Changed paths

23 source/docs/script files; `changed-files.txt` enumerates these and every new evidence path:

- `crates/rndr/examples/selection_gallery.rs`
- `crates/rndr_core/src/access.rs`
- `crates/rndr_core/src/field.rs`
- `crates/rndr_core/src/interaction.rs`
- `crates/rndr_core/src/layout.rs`
- `crates/rndr_core/src/lib.rs`
- `crates/rndr_core/src/overlay.rs`
- `crates/rndr_core/src/popup.rs`
- `crates/rndr_render/src/a11y.rs`
- `crates/rndr_render/src/observe.rs`
- `crates/rndr_render/src/observe_tests.rs`
- `crates/rndr_ui/examples/support/selection_gallery.rs`
- `crates/rndr_ui/src/controls.rs`
- `crates/rndr_ui/src/data.rs`
- `crates/rndr_ui/src/lib.rs`
- `crates/rndr_ui/src/selection.rs`
- `crates/rndr_ui/tests/data_editors.rs`
- `crates/rndr_ui/tests/selection.rs`
- `crates/rndr_ui/tests/selection_gallery.rs`
- `crates/rndr_ui/tests/selection_scope.rs`
- `docs/selection-controls.md`
- `scripts/capture-selection-controls.sh`
- `scripts/verify-selection-controls.sh`

## Launch the actual gallery

```sh
cd '/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-checkbox-and-single-multi-select-controls'
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
cargo run --offline --locked -p rndr --example selection_gallery --features observe
# Presets: --dark --narrow --compact --hc --200 --empty --many --readonly --disabled --invalid
# Source-defined open states: --single-open --multi-open --inspector-open
```

Try standalone checkbox requests, selector navigation without selection, multi toggles without closing, Escape/Tab/outside, option reorder/removal, then inspector edits with explicit Commit/Cancel. Standalone state is separate from inspector committed records.

## Limits / next step

- Actual appearance remains for owner/native pixel inspection due tool delivery.
- Physical pointer/keyboard, real OS IME, live screen readers, DPI transitions and Windows/Linux not validated.
- Intentional bounds: 256 retained standalone options, 64 inspector choices, two-state checkbox; no searchable/editable combobox, typeahead, virtualization, range/select-all, nested menus or async loading. Exhaustive Rust matches must cover additive public enum variants.
- No independent reviewer spawned. Return this complete candidate for owner assessment and, if warranted, **one combined independent review**. Owner retains acceptance and native integration authority.
