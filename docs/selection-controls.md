# Checkbox and selection controls

## Audit / bounded design (2026-11-03)
Baseline c9536acba3914a3be39c53c6bffe42a9b9c40c42 has themed command buttons, retained ListView/TreeView, and inspector Boolean/Choice cycling buttons. Core GenericInteraction owns chronological rich key dispatch, release activation, accessibility admission, focus boundary callbacks and FIFO64. Layer/Modal currently block pointer hits but do not establish popup focus scopes. No new dispatcher or dependency is justified.

This increment adds explicit host-owned Checkbox, SingleSelect and MultiSelect in `rndr_ui::selection`. Stable OptionId values, never indexes, identify options. Mount validates before mutation and reconciles retained entities. Selection props are authoritative; requests never mutate records. A synchronous optional ChangeHandler permits inspector draft reconciliation at the accepted activation boundary; standalone hosts normally drain SelectionChange on Update and remount. Requests include complete desired state; hosts validate current identity/permission before accepting. Rapid queued toggles derive from the latest requested state until remount, not stale acknowledged props. Remount is an explicit acknowledgement/reconciliation boundary.

Core AnchoredPopup is a detached, viewport-constrained Scroll root above its anchor layer. It owns modal hit blocking and a logical focus link to its anchor (inspector property boundaries therefore include the popup). It is positioned after ordinary roots during layout, below the anchor if room, otherwise above. Opening requests focus inside; arrows move actual focus/highlight, independently of selection. Escape and Tab close and return focus; Tab then traverses normally from the trigger. Outside press is swallowed (including the matching release), closes and returns focus, never clicks through. Window blur, stale/hidden/disabled/readonly anchor, lost focus or removed anchor closes. Only one popup at a time. No nested menus, searchable combobox, virtualization or editable text entry.

SingleSelect opens with Enter/Space or Up/Down, exposes all options, arrows/Home/End navigate enabled options, Enter/Space requests selection and closes. MultiSelect keeps its popup open for independent toggles. Escape/Tab/outside only close: they do not undo previously requested changes. In the inspector, selection edits only the draft; explicit Commit requests application mutation, and property departure/window blur cancels uncommitted drafts. Checkbox has two states; mixed is deliberately deferred, not misrepresented as selection.

Accessibility is projected through the actual core tree and AccessKit adapter: checkbox checked state; popup trigger expanded/controls; listbox multiselect; option selected; persistent names and readonly/disabled admission. These are synthetic tree/adapter guarantees, not live screen-reader validation. Disabled and readonly ancestor policy is inherited through physical hierarchy and popup ownership. Modified, synthetic, repeated activation and IME-related key input cannot trigger accidental selection.

Retained bound: at most 256 options; unique IDs and nonblank labels/names required. Empty options show a named empty state with focusable popup surface. Removed selected IDs are ignored in summary/request state (no implicit model mutation); reorder preserves identities, removed highlight falls back to a valid enabled option. Disabled selections remain visible but cannot be toggled through user input. No range selection, select-all, async loading or filtering in this increment.

## Public API

```rust
use rndr_ui::{selection::*, UiId};
// `world` and `form` belong to your ordinary UiPlugin application.
let options = vec![SelectOption::new(10, "Design"), SelectOption::new(30, "Engineering")];
let check = Checkbox::mount(world, form, UiId(1), SelectionProps::new("Updates"), true)?;
let single = SingleSelect::mount(world, form, UiId(2), SelectionProps::new("Team"), &options, Some(OptionId(10)))?;
let multi = MultiSelect::mount(world, form, UiId(3), SelectionProps::new("Teams"), &options, &[OptionId(10)])?;
```

`SelectionProps` contains label, description, enabled, readonly and empty text. `SelectOption.enabled=false` excludes that identity from keyboard focus and activation but preserves its exposed selected state. Stable `UiId` is parent-scoped and shared with other themed components: attempting to reuse it for a different component kind returns InvalidProps. Invalid option sets fail validation before reconciliation. A dead/non-layout parent returns Stale. Library-owned slots must not be modified, removed or reparented by the host. `SelectionHandle.root`, `.popup(world)`, `.option(world,id)`, `.close(world)` and `.unmount(world)` expose observation/teardown, not mutable record ownership. Destroying the ordinary ancestor also retires orphaned detached popups on the next canonical input pass.

Normal hosts read `events::<SelectionChange>(world)` in Update, validate that `control` is still their live generation-checked handle and the IDs/permissions still exist, assign their own state, then remount with accepted values. `SelectionValue` is `Checked(bool)` or `Selected(Vec<OptionId>)`. It is a complete desired state. No request mutates the application model or silently normalizes it. Remount is an explicit reconciliation boundary, resetting the pending-request baseline. Delaying/declining a request keeps the old checked/selected visual state; queued toggles within one batch derive from preceding requests rather than stale acknowledgement. Options are retained across reorder; only removed identities are destroyed. Summaries show up to three labels and a remaining/total count. Every option still has its own projected selected state.

`ChangeHandler(fn(&mut World, Entity, &SelectionValue))` is an optional synchronous adapter for hosts needing exact activation chronology. Installing it replaces event delivery for that control; do not apply both routes. PropertyInspector uses it to update the local draft before the next queued event, with canonical activation and focus-boundary notification unchanged. The control has no separate dispatcher. Use normal core `apply_scroll` when your application wants wheel/drag scrolling (the gallery does).

## Inspector compatibility and commit

Existing `PropertyKind::Boolean`, `PropertyKind::Choice(Vec<Choice>)`, `PropertyValue::Boolean` and `PropertyValue::Choice` keep their APIs and host-owned commit requests. Choice now opens an option popup rather than cycling on every activation. Additive `PropertyKind::MultiChoice(Vec<Choice>)` and `PropertyValue::Choices(Vec<u64>)` support at most 64 unique available nonzero IDs. Empty multiple selection is valid unless the property validator rejects it. IDs serialize into draft `raw` as comma-separated decimal values and canonicalize numerically; no reflection or application-record write occurs. Rust consumers exhaustively matching the public enums must handle the new variants.

MultiChoice equality at inspector transaction boundaries is selected-ID set equality: accepted host vectors may be reordered without dirtying, cancelling, resetting a later draft, manufacturing a commit, or missing a pending acknowledgement. Draft parsing and requested `after` values canonicalize numerically; `before` and application-owned records retain the host's order. This does not change public `PropertyValue` equality or text/number/other raw-draft contracts. Collapsed and open MultiChoice trigger semantics retain the property label plus current draft option labels (or `None selected` / an explicit malformed-selection summary); validation feedback remains separate.

A well-typed selection rejected by validation remains visible as the draft (including empty selection), with feedback and disabled Commit. Invalid or unchanged drafts do not emit CommitRequested. Pending values/delayed acknowledgements follow the existing inspector contract. If requesting a commit disables the focused Commit action, focus moves inside the same property to its editor before that action becomes unavailable; this avoids falsely treating a pending request as property departure. The maintained regression tests cover subsequent draft edits and delayed acknowledgement.

Inside a popup, Escape closes only; a second Escape on the inspector editor cancels its draft. Tab closes and advances from the trigger to that property's Commit/Cancel actions. Outside dismissal closes and returns to the trigger; it deliberately does not also focus/activate the outside target. Leaving the property, window blur or explicit inspector Cancel restores the host value. Multi-select close never implicitly commits or rolls back host requests; inspector application mutation remains an explicit CommitRequested decision.

Disabled ancestors block focus and activation; readonly ancestors permit focus but not edits/opening. This policy follows a popup's logical anchor ownership, even though layout/paint parenting is detached. The popup also closes on unavailable/clipped/removed owners and higher modal layers. A pending outside contact is swallowed through release, including when a policy change dismisses the popup. No pointer activation is forwarded to background controls; background accessibility activation/focus is rejected while the popup scope is active. Arrows are bounded, non-wrapping; Home/End go to first/last enabled option. Shift+Tab closes and traverses backward from the trigger. Space acts on release; repeats/synthetic activation and IME-enabled/preedit/commit snapshots are conservatively suppressed. Modified action keys do not select. There is no text search, OS-combobox delegation, nested popup support, virtualization, range selection or select-all.

## Launch and test

Run in this isolated checkout until the owner integrates it. Explicitly reuse the retained compatible cache; the new scripts refuse an unset cache rather than creating a duplicate target directory:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
cargo run --offline --locked -p rndr --example selection_gallery --features observe
# Presets can be combined:
cargo run --offline --locked -p rndr --example selection_gallery --features observe -- --dark --multi-open
cargo run --offline --locked -p rndr --example selection_gallery --features observe -- --narrow --compact --hc --200 --many --single-open
cargo run --offline --locked -p rndr --example selection_gallery --features observe -- --invalid --inspector-open
# Also: --empty --readonly --disabled; --single-open --multi-open --inspector-open
bash scripts/verify-selection-controls.sh evidence/selection-controls/verification-NEW
bash scripts/capture-selection-controls.sh evidence/selection-controls/native-NEW
```

The interactive synthetic host applies standalone requests to standalone state only, and inspector CommitRequested to separate committed property records. Reorder/remove/reset commands demonstrate identity reconciliation. Both panes respond to viewport/text scale; root and inspector scroll rather than hide overflow. Readonly/disabled/empty examples remain independently present. Run tests with `cargo test --offline --locked -p rndr_ui --test selection --test selection_scope --test selection_gallery` for the focused increment, or the bounded gate above (includes all existing data verification, historical interaction fixtures, canonical core/paint/UI, native/fallback rendering, facade builds and Python tests). Two jobs, no incremental/debug bloat, and a 2 GiB free-space floor are enforced by the new scripts. No cache cleanup is performed.

## Evidence / honest limits

`evidence/selection-controls/REPORT.md` records exact source/HEAD, final gate results/count caveats, native capture manifests and pixel-tool outcome. Native PNGs come from the macOS GPU live surface copy **before presentation**, using synthetic state/focus presets. They do not prove compositor presentation, physical mouse/keyboard, real IME, live screen-reader behavior, DPI transitions or Windows/Linux. Successful receipt/hash checks are not appearance sign-off. Pixel inspection is reported only if actual image pixels reach the image tool's model input; metadata-only delivery is explicitly a limitation, not a reason for endless retries.
