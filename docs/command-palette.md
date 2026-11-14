# Command palette

`rndr_ui::palette::CommandPalette` is a focus-scoped combobox. It is a text
field over a uniform [`ListView::mount_virtual`](virtual-list.md) list, opened
as an [`AnchoredPopup`] on the next [`Layer`]. The popup is the focus scope:
Tab and Escape belong to that scope, a higher modal blocks it, and leaving the
scope closes it. `open_popup` installs the layer and the modal. The palette
does not add a second input dispatcher.

Headless Sim tests cover the contract below. They do not certify VoiceOver,
Narrator, Orca, or a physical keyboard.

## Mount and dismiss

```rust
let palette = CommandPalette::mount(world, anchor, UiId(1), PaletteProps::new("Commands"))
    .expect("anchor");
palette.set_commands(world, &commands).expect("commands");
palette.open(world);
```

The anchor needs a `Layout`. Escape returns focus to it, so it should be able
to take focus. `PaletteHandle::close` does the same. A higher modal refuses
`open`. `PaletteDismissed` is visible to Update on that frame.

`PaletteHandle::set_defer_escape(true)` leaves Escape unconsumed and leaves
the popup open. A host uses that when the field is temporarily a prompt and
Escape means something else. Repeat, release, synthetic, and modified Escape
still do not close the popup.

## Keyboard

Focus stays on the field. Arrow, Page Up, Page Down, Home, and End move the
active command and reveal it. Enter activates it and does not close the
palette. The host decides what activation means.

`PaletteActivated` for Enter is visible to Update on that frame. A pointer
double-click also activates. That event is observed in Update, so
`PaletteActivated` for the click is visible on the following frame.
`CollectionEvent::Activated` on the list is visible in Update for both.

Shift, Alt, and Command chords are not stolen, so a prompt can still use
Shift+Enter. An IME preedit or commit does not move or activate.

While `set_suspended(true)`, typing and those movement keys stay with the
field. The palette example uses that for Ask.

## Filter, modes, and async results

The default matcher is `fuzzy_match`. Lower scores sort first. Case and
diacritics fold. An exact accent match sorts ahead of the same letters with
the accent folded off. Match ranges are byte spans in the title, and the row
highlights them. `set_matcher` replaces it. `None` hides the command in
synchronous mode.

A synchronous catalog filters on each edit. Sections stay together: group
order follows the best score in the group, and rows inside a group stay in
score order. A header is not a command. Its id has the high bit set
(`is_section`). Arrows skip it. Enter does not activate it.

`modes` on a command is empty for the unprefixed mode only. A non-empty list
is the prefixes that show it. The host chooses the prefixes with `set_modes`,
for example `>`. The first character selects the mode. The filter text is the
rest, with one leading space removed. `PaletteQueryChanged` carries the text,
the filter, the mode, and the epoch. It is visible on the following frame.

`set_async(true)` does not replace the visible rows when the field changes. It
sets the loading line and bumps the epoch. `resolve(epoch, commands)` applies
that generation and clears loading. An older epoch returns `Ok(false)` and
changes nothing. Invalid commands are refused before mutation. The host owns
the search task. The palette example's feed is one such host: a slow reply
cannot replace a newer query.

Empty and loading are different. A finished query with no rows shows
`PaletteProps::empty`. Waiting for `resolve` shows `PaletteProps::loading`,
including while the previous rows are still on screen. The status line is a
polite live region.

## Rows

Each command can carry a Lucide `Icon`, drawn as that icon's Private Use
scalar, and a shortcut string. The shortcut is a hint. The palette does not
bind it. The accessible name is the title. The shortcut is the description.
Disabled commands stay visible and are not activated.

## Accessibility

The field is a combobox: expanded while the popup is open, `controls` the
list, and `active_descendant` is the realized active option. Focus stays on
the field. The value is the field text. The list is a listbox labelled by the
field. Each command row is an option. A section row is a group. The platform
bridge publishes `active_descendant` on the AccessKit node.

Only realized rows exist in the tree. A full offscreen collection is not
claimed.

## Example

`cargo run -p palette` is the window, tray, and global hotkey. It calls this
component. Ask Medousa, the feed, and the tray stay in the example.

```sh
cargo test --offline --locked -p rndr_ui --test palette
cargo test --offline --locked -p palette
```
