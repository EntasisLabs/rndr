# File drop

`FileHover` and `FileDrop` carry a path and `position: Option<[f32; 2]>` in
logical points. The desktop runner forwards a point only when the native
report included a finite one. The host decides which row that point hits.
This crate does not.

`FileHoverCancel` is one event. `FileHoverSet::cancel` clears every path in
the current hover. A drop does not clear that set. The runner owns the
`FileHoverSet` resource on each window and writes it back when it applies
file events.

## Position

`FileHover::from_report` and `FileDrop::from_report` keep a point when both
components are finite. Every other report stores `None`.

| Report | Stored position |
| --- | --- |
| No point | `None` |
| Finite point, including `[0.0, 0.0]` | That point |
| NaN or infinity | `None` |

`[0.0, 0.0]` is the window origin when the platform reported the origin. A
missing point stays `None`. The runner does not read the cursor, a previous
hover, or any other pointer to fill it.

winit 0.30.13 (the version in `Cargo.lock`) defines `DroppedFile(PathBuf)`,
`HoveredFile(PathBuf)`, and `HoveredFileCancelled`. The runner maps the first
two through `winit_path`, which sets `position: None`. Inspected winit
sources for that tag discard the operating-system point before the event
exists:

- macOS `draggingEntered:` and `performDragOperation:` read the pasteboard
  and do not read `draggingLocation`.
- Windows `IDropTarget` receives a `POINTL` and ignores it (`_pt`).
- X11 `XdndPosition` root coordinates are present in the client message and
  left commented out. The coordinates are root-window coordinates, not window
  logical points. `HoveredFile` is emitted later, without them.
- Wayland has no file-drag event in that tree.

Reading those discarded fields would be a second drag destination, with a
coordinate space this repository has not measured. No feature and no new
crate do that. Default features and `--no-default-features` both keep the
path-only winit mapping.

## Platform matrix

Two tables. The binding table is what this code does with the winit events
above. The native table is a certification ledger. A headless test locks the
cells; it does not move a native cell to `Supported`.

| Platform | Paths in this binding | Position in this binding | Native session |
| --- | --- | --- | --- |
| macOS | Forwarded when winit emits them | Unavailable. The event position is `None`. | Unverified |
| Windows | Forwarded when winit emits them | Unavailable. The event position is `None`. | Unverified |
| X11 | Forwarded when winit emits them | Unavailable. The event position is `None`. | Unverified |
| Wayland | Unsupported. No Wayland drop path was added. | Unsupported. No point is synthesized. | Unsupported |

`file_drop::platform_support` is the binding table.
`file_drop::native_session` is the ledger. `PositionDelivery::Supported` and
`NativeSession::Supported` are not returned.

Wayland stays unsupported for the events and for the point. The runner does
not grow a data-device client and does not borrow the pointer position when
`Platform::Wayland` is set.

## Compatibility

This is a 0.y crate. The field is an intentional source break for event
construction:

- `FileHover { path }` and `FileDrop { path }` do not compile. Set
  `position`, or call `from_report`. `position: None` is the path-only event.
- `FileHover` and `FileDrop` no longer implement `Eq`. `PartialEq` remains,
  so `assert_eq!` on the values still works. A bound that requires `Eq` does
  not.
- `FileHoverCancel` is the same unit struct.
- `rndr::FileHover`, `rndr::FileDrop`, and `rndr::FileHoverCancel` are the
  core types, re-exported by the facade.
- No workspace feature, facade feature, or package dependency was added.

## Evidence

Headless, in this change:

- `cargo test -p rndr_core --test file_drop` admits finite points, rejects
  non-finite points, clears every hovered path on cancel, round-trips the
  events, and locks both tables.
- `cargo test -p rndr_render --lib file_drop` checks the winit path mapping,
  pass-through of a supplied logical point, and cancel against a set that
  holds one positioned path and one path-only path.

No named native session was run. macOS, Windows, and X11 stay `Unverified`.
Wayland stays `Unsupported`. A green headless run is not a drag on a desktop.
