# Diff presentation

`rndr_ui::diff::DiffView` draws a host-supplied diff. The host owns the hunks.
It passes stable hunk ids, row text, a row id, a height, and a gutter marker:
unchanged, added, removed, or header. The view paints that marker beside the
text and moves between hunks. It does not parse a diff, compute one, or store
a repository, a forge, a path, or a comment.

Rows are a `ListView::mount_variable` list. Scroll identity is that list's
anchor: an `ItemId` plus a milli-point remainder. `next_hunk`, `previous_hunk`,
and `jump_hunk` place the first row of the target hunk at the anchor with
remainder 0. The view does not syntax-highlight. A long row stays one line
(`TextOverflow::Visible`) at the height the host supplied. The editor gutter
is not used.

## Repair

- An empty row list shows the empty copy. Focus and the anchor are clear.
  Next and previous do nothing. A jump fails.
- One very long line is one row. Next and previous stay on its hunk. The label
  keeps the full string.
- Removing the focused hunk moves focus to the next surviving hunk in the
  previous order, then the previous hunk, then no hunk. The repaired hunk's
  first row becomes the anchor at remainder 0.
- Changing a height or inserting a row above the anchor, without changing the
  focused hunk, keeps the anchored row id and remainder. Shrinking the anchored
  row clamps the remainder to the new height.

Retained list, table, and tree budgets stay as they are. `SelectionMode` is
unchanged. This view mounts its list with `SelectionMode::None`.

## Native gap

Headless tests cover the anchor and the gutter text. They do not make the diff
cell `Supported`. Trackpad, scrollbar, and screen-reader passage of a diff
remain unverified until a named session records them.
