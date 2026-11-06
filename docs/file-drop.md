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

## What a live drag delivers

No platform delivers a coordinate from this tree. winit 0.30.13 (the version
in `Cargo.lock`, and the current stable release) defines
`DroppedFile(PathBuf)`, `HoveredFile(PathBuf)`, and `HoveredFileCancelled`.
The runner maps the first two through `winit_path`, which passes
`DragPoint::Absent`. The window scale is available and is not used to invent
a point. The cursor is not read.

The operating system does report a point, and winit drops it before the
public event:

- macOS `draggingEntered:` and `performDragOperation:` read the pasteboard
  and do not read `draggingLocation`.
- Windows `IDropTarget` receives a screen `POINTL` and ignores it (`_pt`).
- X11 `XdndPosition` carries root coordinates in `data.l[2]`. The unpack is
  commented out. `HoveredFile` is emitted later, from `SelectionNotify`,
  without them.
- Wayland has no file-drag event in that release.

## Blocker

Attaching the real point means sitting in the callback that still has it.
On 0.30.13 that callback is private:

- macOS: `WinitWindowDelegate` already implements `NSDraggingDestination`.
  A second destination would steal the drag. Wrapping the IMP can read
  `draggingLocation` and then call the original, but the method encoding has
  not been executed here, and a mismatch would stop path delivery.
- Windows: `RegisterDragDrop` allows one `IDropTarget` per `HWND`, and winit
  holds that pointer in a private field. Replacing it means reimplementing
  `CF_HDROP` delivery.
- X11: the `XdndPosition` client message is delivered on winit's X
  connection. A second connection does not see it.

winit 0.31.0-beta.3 (published 2026-09-04; stable is still 0.30.13) replaces
those events with `DragEntered`, `DragPosition`, `DragDropped`, and
`DragLeft`, and the position is physical pixels of the client area, top-left
origin. The macOS value comes from `convertPoint:fromView:` on the flipped
content view. Windows uses `ScreenToClient`. X11 unpacks the `XdndPosition`
root point and `XTranslateCoordinates`s it into the window. That beta also
splits the crate and changes the event loop, key events, and other window
APIs. `accesskit_winit` 0.34 and `tray-icon` 0.26 are built against 0.30.
This pull request does not take the beta.

Wayland file drag landed in that same beta. This unit does not adopt it.
R0 keeps Wayland unsupported, including after a later winit bump: do not map
Wayland drag events into `FileHover` or `FileDrop`.

When a stable winit that carries the client point can move with those
crates, map each path in the drag set through `from_drag` with
`DragPoint::ClientPhysical` and the window scale. One winit position applies
to every path in that callback. `DragLeft` stays a single `FileHoverCancel`.
Do not fill a missing point from `CursorMoved`.

Until that bump, `PositionDelivery` stays `Unavailable` on macOS, Windows,
and X11. The conversion below is what that bump will call. It is not a claim
that a drag today has a point.

## Conversion

`from_drag` is the correlation point. The logical point on the event is the
point from that same callback.

| Sample | Scale | Stored position |
| --- | --- | --- |
| Absent | any | `None` |
| Client physical `(0, 0)` | `2` | `[0.0, 0.0]` |
| Client physical `(10, 20)` | `2` | `[5.0, 10.0]` |
| Client physical NaN or infinity | any | `None` |
| Client logical `(16, 60)` | `2` | `[16.0, 60.0]` (not divided) |
| Non-positive scale | physical `(8, 4)` | `[8.0, 4.0]` (scale reads as 1) |

AppKit window-base points are converted with the flipped content frame
before they are logical. The top-left of a content view whose frame origin
is `(0, 0)` and whose height is `200` is window point `(0, 200)`, and the
logical point is `(0, 0)`. Windows screen pixels and X11 root pixels become
client pixels by subtracting an origin the callback already translated
(`ScreenToClient`, or `XTranslateCoordinates`). The unpack of `XdndPosition`
keeps the signed 16-bit root coordinates. Neither helper queries the pointer.

Default features and `--no-default-features` both keep this mapping. No new
crate was added.

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
- `cargo test -p rndr_render --lib file_drop` checks the winit path mapping
  at several scales, physical-to-logical division, a reported origin,
  non-finite samples, per-event correlation, the flipped AppKit content
  frame, the Win32 screen subtraction, the `XdndPosition` unpack, and cancel.

No named native session was run. macOS, Windows, and X11 stay `Unverified`.
Wayland stays `Unsupported`. A green headless run is not a drag on a desktop,
and it does not mean the binding supplied a point.
