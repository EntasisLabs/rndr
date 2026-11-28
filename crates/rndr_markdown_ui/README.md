# Editable Markdown surfaces: links and navigation

`MarkdownSurface` keeps Markdown source authoritative while painting rich text.
Ordinary left-click, drag selection and typing remain editing operations.

## Host-owned link activation

**Cmd + left-click on macOS; Ctrl + left-click on Windows/Linux** activates a
painted link. macOS Ctrl-click remains a native secondary/context gesture.
Activation occurs on release over the same link, within four logical points of
the press, with the same source revision and the primary modifier still held.
Shift/Alt combinations, link drags, focus loss, composition and stale source do
not activate. Checkbox and table-handle gestures remain editing operations.

The event is the existing `rndr_core::LinkActivated`:

```rust,ignore
for event in rndr_core::events::<rndr_core::LinkActivated>(world) {
    if event.view == note_surface.root {
        // Resolve note/code paths and fragments in this note's context.
        // Validate allowed schemes/authority before opening external resources.
        route_note_link(&event.href);
    }
}
```

`view` is the rich **surface root**, not a label, table cell or syntax node.
`href` comes from the parsed source: a Markdown destination (without its title),
a wikilink target (without its alias), or an autolink target. Display offsets are
mapped through projected source segments, including hidden markers, nested
formatting, Unicode, task text and table cells. A glyph hit is distinct from a
nearest caret hit: whitespace past the rendered text cannot activate a link.

**RNDR never launches a URL from this API**, including `javascript:`, `file:` or
other unsafe/unrecognized schemes. Emitting a written destination is not an
approval to open it. Hosts own policy, note/code resolution, authority checks,
path confinement and fragment handling.

## Accessibility

Rich mounts automatically install `AccessOverlayActivate`. The native AccessKit
bridge advertises Click for enabled synthetic Link nodes and maps that action to
`AccessCommand::ActivateOverlay { entity: root, node }`. The rich callback emits
the same `LinkActivated` event without changing selection or bytes; no Medousa
adapter glue is required.

Synthetic IDs are world-unique and revision-scoped, not ECS entity IDs. A queued
action cannot acquire a replacement document's link, a different rich surface,
a hidden owner, a disabled node or an unpublished/stale source revision.
Non-native adapters can use `access_snapshot` plus
`activate_access_link(world, root, &snapshot, node_id)`. Snapshots expose current
selection state; link activation additionally requires the overlay's publication
revision to match. Native screen-reader operation has not been manually certified;
headless AccessKit node/action and rich callback tests cover the routing contract.

## Reader heading/block navigation

The readonly `rndr_core::MarkdownView` exposes
`scroll_to_block(&mut self, index: usize) -> bool`. Resolve a heading to an index
from `blocks()` after the view has synced, then request that block. Offscreen
headings need no realized entity. Sync uses the retained measured/estimated height
map, realizes the target window and refines its position after measurement.
Normal end-of-document clamping keeps the viewport filled. It does not promise
exact pixel coordinates for still-unmeasured preceding blocks.

An invalid index or unsynced source edit returns false; the last valid request
wins. Replacing source retires pending requests. The request preserves selection,
cancels an active pointer-selection gesture and disables bottom-follow until the
host explicitly reenables it. Hosts own heading-slug and fragment resolution.

## Regression coverage

- `cargo test -p rndr_markdown_ui`: actual press/release input events, core event
  payload/root identity, Markdown/wikilink/autolink forms, Unicode and nested
  formatting, task/table mapping, multiple surfaces, editing and drag behavior,
  stale source/overlay actions and unsafe-scheme event-only behavior.
- `cargo test -p rndr_core --test markdown_scroll_request`: offscreen navigation,
  retained virtualization, selection/follow behavior, invalid and replaced source.
- `cargo test -p rndr_render --lib a11y::`: native node actions, owner mapping,
  disabled/hidden/stale synthetic IDs and event-only activation.

Native file-drop integration is separate. RNDR's winit delivery is path-only with
`position: None`; a cached last pointer position is not a native drop location and
cannot certify bounded composer hit targeting. This change does not modify it.
