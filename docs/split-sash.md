# Split sash

`rndr_ui::split::SplitSash` is a focusable split built on core [`Splitter`](../crates/rndr_core/src/split.rs). Pointer drag, clamping, and the leading-sibling write stay in [`apply_split`](../crates/rndr_core/src/split.rs). The sash adds a slider role, keyboard resize, double-click reset, and a change event the host can persist. A core `Splitter` on its own is still not focusable, and it still drags only when `apply_split` runs.

These checks are headless. They do not certify a platform screen reader, a physical keyboard, or a native window.

## Mount

`SplitSash::mount` places three children under a fill root: a leading pane, the sash, and a trailing pane that fills what is left. Put content in `leading` and `trailing`. Mount another sash into either pane to nest a split. `unmount` despawns that root and every descendant.

`SplitSashProps` needs a nonblank label. The label is the accessible name. It is not drawn on the handle. `axis` is fixed for that id: `Axis::Row` moves left and right, `Axis::Column` moves up and down. Changing axis on an existing id returns `InvalidProps`.

`min_leading` and `min_trailing` are logical points, finite and at least zero. The defaults are 48 and 48, not the core handle's 120 and 160. The ratio is the leading pane's share of the length the two panes share, after the handle, gaps, and padding. Mount clamps the requested ratio into `0..=1`, then into the range those minimums still allow. When the parent is smaller than both minimums, the leading side stays at `min_leading`, which is the existing `Splitter` rule.

`step` is the ratio added or removed by one arrow key. `page` is the ratio for Page Up, Page Down, and Shift+arrow. `default_ratio` is what a double-click restores. The defaults are `0.05`, `0.2`, and `0.5`.

## Keyboard and pointer

The sash is a tab stop when it is enabled. A pointer press focuses it. Disabled handles are not focusable and do not resize. Readonly handles focus, and keys, drags, and double-click do not resize them.

| Key | Effect |
| --- | --- |
| Right, and Up on a row | Increase the leading share by `step` |
| Left, and Down on a row | Decrease the leading share by `step` |
| Down, and Right on a column | Increase the leading share by `step` |
| Up, and Left on a column | Decrease the leading share by `step` |
| Shift+arrow, Page Up, Page Down | Change by `page` |
| Home, End | Smallest or largest leading share the minimums allow |

The arrow toward the trailing pane grows the leading pane. Keys in one frame resolve to the ratio that remains, and that ratio is the event.

A double-click is a second press on the same sash within 400 ms. It restores `default_ratio` and does not keep dragging. A later press outside that window does not reset.

Dragging uses `Splitter`. `UiPlugin` runs `apply_split` while a sash is mounted, including the reclamp. An app that also installs `Shell` runs `apply_split` again in Update. The second call applies no further pointer delta. A world with only a core `Splitter` and no sash does not start that system.

## What the host persists

`SplitSashChange` carries the sash entity and the clamped ratio. The layout already shows that ratio. Read the event and store the ratio. Mounting the same id with that ratio leaves the panes where they are. Mounting a different ratio replaces them, which is how a host rejects a drag or restores a saved layout.

The parent can change size. The next frame writes the stored ratio into the new length, then clamps to the minimums. Nested sashes keep their own ratios.

Assistive set-value, increment, and decrement use the slider's `NumericAccess` range. The value is the ratio, not a percent. The minimum and maximum are the shares the pane minimums still allow. A value that does not parse is rejected and writes nothing.

The access tree reports `AccessRole::Slider`, the label, the ratio text, and whether the handle is focusable. The AccessKit adapter maps that numeric range and its orientation. This is a headless tree, not a VoiceOver, Narrator, or Orca session.

## Run

```sh
cargo test --offline --locked -p rndr_ui --test split_sash
cargo test --offline --locked -p rndr_core --test split
cargo run -p rndr_ui --example split_gallery
```
