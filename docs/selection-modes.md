# Range and multiple selection

`SelectionMode` is `None`, `Single`, `Multiple`, or `Range`. `None` and `Single`
behave as before. This crate is 0.y, so an exhaustive match outside the
workspace needs a new arm for `Multiple` and `Range`.

`Selected(Option<ItemId>)` is only the single-id slot. `None` and `Single` use
it. `Multiple` and `Range` leave that slot empty and emit `SelectionSetChanged`
when membership changes. That event does not carry the ids. The host reads
`selection()` on the list handle, including `DataTable`'s `rows` handle.

## Gestures

The selection gesture is primary release, Space release, or an accessibility
activate. Arrow and focus movement do not change the set.

- `Multiple` toggles an enabled id. A disabled id is not added.
- `Range` without Shift selects that one enabled id and moves the range anchor.
- `Range` with Shift selects the enabled ids from the anchor through the
  gesture id in the current logical order. Disabled ids in the span are
  skipped. The anchor does not move.
- A Shift gesture with no anchor selects that one enabled id and sets the
  anchor. That is the empty-anchor case, not an extension.

`select(Some(id))` remains the `Single` setter. `Multiple` and `Range` reject
it. `select(None)` clears the set.

## Repair

Removing an id, or disabling it, drops it from the set and emits
`SelectionSetChanged` once for the whole change. Recycling a row does not drop
ids. Reorder keeps the ids.

When the range anchor is removed or disabled, it moves to the next enabled id
that still exists in the previous order, then the previous enabled id, then
none. The set is not rebuilt from that repaired anchor.

Lists and `DataTable` share this policy. The table stays inside 256 rows by 32
columns and is not virtualized. Retained list and tree budgets are unchanged.
A diff view stays on `SelectionMode::None`.

## Native gap

These tests drive headless pointer, keyboard, and `try_activate` calls. They do
not certify a physical pointer, a keyboard, or a screen reader. That cell stays
unverified until a named session records it.
