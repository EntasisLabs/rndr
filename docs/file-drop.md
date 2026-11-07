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

A drag callback that includes a point which is not a finite logical point
clears the point stored for that drag. A callback that includes no point
leaves the stored point unchanged. The next finite point replaces it.

## What a live drag delivers

The runner uses winit 0.31.0-beta.4. `DragEntered` and `DragPosition` carry
client physical pixels, top-left origin, when the backend has them.
`DragEntered.position` may be `None`. Those pixels are divided by the window
scale. A non-finite sample clears the stored point, so the hover reports
`None`. An entered event with no position does not clear a point already
stored for that same drag. A reported `(0, 0)` stays the origin. The cursor
is not read.

File names come from the transfer's `text/uri-list`, one `FileHover` or
`FileDrop` per path. Every path in that transfer shares the drag's point.
`DragLeft` is one `FileHoverCancel` and clears the hover set. A drop does not.

On `DragEntered` the runner accepts `DndAction::Copy` and fetches the URI
list. Without that accept, winit 0.31 rejects the drag and no path is
delivered. The list is applied only when its transfer id and fetch serial are
the request still outstanding for the active drag. A response for an older
transfer does not replace a newer drag. A response after the drag has been
dropped or has left does not start that drag again. A list that arrives after
the release, while that same fetch is still outstanding, still completes the
drop. This correlation is a simulated tracker. It is not a defect observed on
a live desktop.

Windows `file://server/share/dir/file.txt` becomes the UNC path
`\\server\share\dir\file.txt`. A Windows drive URI `file:///C:/notes.txt`
becomes `C:/notes.txt`. On Unix that same URI stays the absolute path
`/C:/notes.txt`. A percent-encoded byte that is not UTF-8 stays in the Unix
path. A broken percent-escape is skipped.

A window whose raw handle is Wayland is ignored: the drag is not accepted and
no file event is emitted. winit 0.31 can deliver Wayland drags; this unit
does not forward them.

winit turns the operating-system point into those client pixels before the
event: AppKit `convertPoint:fromView:` on the flipped content view, Win32
`ScreenToClient`, and X11 `XdndPosition` translated into the window. This
repository compiles that mapping. It has not run a drag on a desktop, so the
native cells stay `Unverified`.

`accesskit_winit` 0.34 is still built on winit 0.30. The `accesskit` feature
stays on and uses a local adapter over `accesskit_unix`, `accesskit_macos`,
and `accesskit_windows`. `tray-icon` 0.26 does not depend on winit, so the
tray feature is unchanged. The event loop no longer carries a typed user
event; the runner posts `RunnerEvent` on a channel and calls
`EventLoopProxy::wake_up`.

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
| macOS | Forwarded from `DragEntered` / `DragDropped` after the URI list arrives | Client physical pixels from `DragEntered` / `DragPosition`, divided by the window scale. `None` when that event has no finite point | Unverified |
| Windows | Forwarded the same way | The same conversion | Unverified |
| X11 | Forwarded the same way | The same conversion. The point is the translated window position winit supplies, not a root coordinate | Unverified |
| Wayland | Unsupported. Drag events on a Wayland window are not accepted and not forwarded | Unsupported. No point is synthesized | Unsupported |

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
  a finite point followed by an unusable point followed by a finite point,
  an event that carries no new point, Windows UNC and drive URIs, a Unix
  path that only looks like a drive, a percent-encoded non-UTF-8 Unix path,
  per-event correlation, a late path list for an older or finished drag,
  the flipped AppKit content frame, the Win32 screen subtraction, the
  `XdndPosition` unpack, and cancel.
- `cargo test -p rndr_render --lib tablet` maps a tablet-tool move, tip
  press, and tip release onto the pen contact, including pressure, tilt,
  twist, and a logical point. It does not open a window.
- `cargo test -p rndr_render --lib access_winit` checks that a desktop inner
  origin is the outer position plus the window-relative surface offset, for
  a nonzero origin, a move, and a resize. Linux `Moved` and `SurfaceResized`
  both use that sum.

No named native session was run. macOS, Windows, and X11 stay `Unverified`.
Wayland stays `Unsupported`. A green headless run is not a drag on a desktop,
and it does not mean the binding supplied a point.
