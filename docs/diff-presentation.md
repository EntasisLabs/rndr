# Diff presentation

`rndr_ui::diff::DiffView` draws a host-supplied diff. The host owns parsing,
hunks, language highlighting, file groups and comments. Pass stable hunk and
row IDs, source text (including blank lines), explicit row heights and an
unchanged/added/removed/header marker. No repository or Forge state is stored.

## Code presentation

Rows use the theme's `Typography::Code` family and metrics (13-point type,
20-point line box by default), without control-label vertical insets. Added
and removed rows have full-width semantic fills; headings use a quiet band.
The root fills its parent on first mount, replacing the generic list's fixed
280-point default. An existing host-set viewport size survives remounts.
Keep viewport gap and padding zero. Hosts still own row heights: use at least
`text_engine(world).line_height(&theme.typography(Typography::Code))` for source
rows, including after reading-scale changes. Explicit shorter rows are not
silently resized. The editor gutter is not used.

Long lines stay single-line. The view measures the complete supplied data with
the active text engine and styled runs; every realized row shares that minimum
width. Core's normal `Scroll` handles both axes and clamps offsets after resize
or replacement. The extent includes unrealized rows, so vertical recycling does
not make the horizontal range jump. A code-style or engine replacement refreshes
metrics; idle frames do not remeasure all source lines. No soft wrapping is added.

## Source coordinates and highlights

Existing `mount` calls remain unnumbered and need no new fields.
`mount_annotated` additionally accepts `DiffAnnotation` entries by row ID:

```rust,ignore
let annotations = [DiffAnnotation {
    id: ItemId(7),
    old_line: None,
    new_line: Some(13),
    spans: vec![TextSpan::new(0, 3, keyword_color)],
}];
let handle = DiffView::mount_annotated(
    world, parent, key, DiffProps::new("Changes"), &rows, &annotations,
)?;
```

Old/new coordinates occupy separate right-aligned cells, sized for the largest
supplied line number. Missing coordinates leave blank cells. Source text is
never prefixed with coordinates, so syntax and inline-change `TextSpan` byte
ranges address the original UTF-8 string. Spans can provide color, weight,
italic and highlight backgrounds through core's existing text machinery.
The host supplies theme-appropriate colors and tokenization; RNDR does not infer
language, parse hunk headers or invent coordinates. Duplicate/unknown annotation
IDs and out-of-bounds/reversed/non-character-boundary spans refuse before
publication. Remounting without annotations clears old numbers and runs.

## Navigation and repair

Rows are a `ListView::mount_variable` list. Scroll identity is that list's
anchor: an `ItemId` plus a milli-point remainder. `next_hunk`, `previous_hunk`
and `jump_hunk` place the first row of the target hunk at remainder zero.

- An empty row list shows the empty copy. Focus and anchor are clear. Next and
  previous do nothing; a jump fails. Blank source lines remain real rows.
- One very long line remains one row. Next/previous stay on its hunk.
- Removing the focused hunk repairs forward in previous order, then backward,
  then to no hunk. The repaired hunk starts at remainder zero.
- Height changes and inserts above the anchor keep the anchored row ID and
  remainder. Shrinking the anchored row clamps its remainder.

Retained list/table/tree budgets and selection contracts are unchanged.
DiffView uses `SelectionMode::None`; row selection is not a code-copy API.

## Evidence and remaining native gap

`rndr_ui/tests/diff_presentation.rs` covers anchors, invalid publication,
blank rows, numbered/highlighted remounts, typography changes, measured scroll
extent and virtualization. `rndr/tests/diff_pixels.rs` uses Parley and a real
headless graphics adapter for wide/narrow dark, light, and horizontal-tail
fixtures. Run it with:

```sh
RNDR_DIFF_CAPTURE_DIR=/absolute/path/to/captures \
  cargo test -p rndr --no-default-features --features parley --test diff_pixels
```

These fixtures are component evidence, not installed-app certification.
Trackpad/scrollbar interaction and screen-reader passage still need a named
native session. File accordions, expand-context actions and comment controls
remain host composition rather than implied DiffView behavior.
