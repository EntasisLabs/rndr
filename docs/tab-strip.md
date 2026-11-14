# Tab strip

`rndr_ui::tabs::TabStrip` is a tablist for editor and document tabs. The host owns the tabs and their order. The strip shows that order, scrolls it, and emits close, reorder, move, and split requests. A [`SplitSash`](split-sash.md) is how a host turns a split request into panes. Dragging reuses [item drag](item-drag.md). The strip does not start a second drag path, and it does not split a pane itself.

These checks are headless. They do not certify a platform screen reader, a physical keyboard, or a native window.

## Mount

`TabStrip::mount` places a bar and a body under a fill root. The bar is a leading chevron, a horizontal scrollport, a trailing chevron, and an overflow button. The scrollport's child is the tablist. Each tab is 120 points wide and carries an optional icon, the label, a dirty mark, and a close button. The body holds one panel per tab. Put document content in `TabStripHandle::panel`. A later mount with the same id reuses the root, the surviving tabs, and their panels. A tab left out of the next list is despawned with its panel.

`TabStripProps` needs a nonblank label. That label is the tablist's accessible name. It is not drawn on the bar. `active` chooses the selected tab's fill: the selected token when the strip is the active group, and a quieter raised token when it is not. Focus inside the strip also uses the selected fill. `enabled` set to false removes focus, activation, drag, and close.

`TabSpec` needs a nonblank label and an id that is unique in that strip. `selected` must be one of those ids. `icon` draws a built-in glyph and does not replace the accessible name. `dirty` draws a mark and appends ", unsaved" to the name. `pinned(true)` draws a pin, appends ", pinned", and clears `closable`. Call `closable(true)` after that to allow closing a pinned tab. Pinned tabs stay in a leading group: a move that would place a pinned tab after an unpinned one, or an unpinned tab before a pinned one, is refused.

## Keyboard, pointer, and close

The tablist is one tab stop. Arrows, Home, and End move focus inside it and wrap. They do not change which panel is shown. Enter and Space, and a click that did not become a drag, select the focused tab and show its panel. The strip emits `Activated`. Mounting again with a different id replaces that selection, which is how a host rejects it.

| Input | Effect |
| --- | --- |
| Left, Right | Previous or next tab |
| Home, End | First or last tab |
| Enter, Space, click | Select that tab |
| Middle-click | Ask to close, when the tab is closable |
| Close button | Ask to close |
| Delete, Ctrl+W, Cmd+W | Ask to close the focused tab, or the selected tab when focus is in its panel |
| Alt+Left, Alt+Right | Ask to move the focused tab before the previous tab, or after the next |

Close is `CloseRequested`. The tab stays until the host omits it from the next mount. A tab that is not closable, a disabled strip, and a focused editor that captures keys do not emit a close. Alt+arrow that would not change the order, or that would cross the pinned group, announces `Cannot move {name}.` and emits nothing.

## Overflow

The chevrons scroll the tab row by one tab width. The one that cannot scroll is disabled. The overflow button opens a menu of the tabs that are not fully inside the port. Picking one selects it. The selected tab is scrolled into view when it becomes selected, when its index changes, when the port width changes, and on the first frame the port has a size. A chevron can still scroll it aside after that. Choosing it again brings it back.

## Drag, move, and split

Each tab is an item-drag source and a before/after target. The kind is `TAB` (`DragKind(3)`). List rows and tree nodes stay 1 and 2. The drag owner is the strip, so a tab from one strip can land on another. The strip does not reorder itself.

| Drop | Event |
| --- | --- |
| Before or after a tab in the same strip | `Reordered` |
| Before or after a tab in another strip | `Moved`, with that tab as the target |
| Interior of another strip's pane | `Moved`, with no target, place inside |
| Edge of a pane, including the strip the drag started on | `SplitRequested` with `Left`, `Right`, `Above`, or `Below` |

`reorder_tabs` and `transfer_tab` are the helpers a host can apply. Both return `None` when the order would not change or the pin group would break. `SplitDirection::axis` is the `SplitSash` axis for that edge. `leading` is true for left and above, which is the pane the dragged tab occupies.

A successful move or split is announced on the same polite live region item drag uses. That is a headless name and a live-region component, not a claim that a screen reader spoke.

The access tree reports `TabList` and `Tab`. The selected tab's panel carries the panel flag the desktop adapter maps to a tab panel, and the tab and the panel point at each other. This is a headless tree, not a VoiceOver, Narrator, or Orca session.

## Run

```sh
cargo test --offline --locked -p rndr_ui --test tab_strip
cargo run -p rndr_ui --example tab_strip
```
