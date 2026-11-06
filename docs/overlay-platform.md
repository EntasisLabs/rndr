# Overlay windows

A [`WindowKind::Panel`](../crates/rndr_core/src/window.rs) is the summonable
surface. macOS builds it as a non-activating panel at pop-up level, on every
Space, including full-screen spaces. That path was already in the tree. This
note covers the Windows and Wayland paths added later, and what was actually
run.

`platform_support` is the table. `Supported` means a live session of that
platform has shown the behavior. `Unverified` means the runner sends the
request and no live session in this repository's checks has shown that the OS
honors it. `Ignored` means the command is accepted and the OS is not asked, or
the platform cannot do it. Do not read `Unverified` as support.

## Wayland

On Linux, a panel tries `zwlr_layer_shell_v1` on its own connection. Ordinary
xdg toplevels are unchanged: winit 0.30 still cannot position, hide, focus, or
set a level on them. `support_describes_panels_only(Platform::Wayland)` is
true for that reason.

The placement request is [`panel_placement`](../crates/rndr_core/src/panel_place.rs):

- `Position::Center` anchors no edge. A sized layer surface with no anchors is
  centered on the output.
- `Position::Platform` anchors the top edge only, which centers it on that edge.
- `Position::At` anchors the top-left corner and sets margins.
- Opposite edges are not both set, so the compositor is not asked to stretch.
- The layer is `overlay` when the window is a panel or always-on-top, and
  `top` otherwise. The protocol draws fullscreen shell surfaces on the `top`
  layer and `overlay` above that. Exclusive zone is 0, so the panel does not
  reserve space.
- Keyboard interactivity is `exclusive` only while the panel is visible and
  focus was requested. That takes the keyboard. It is not a non-activating
  focus, so `panel_nonactivating` stays `Ignored`.
- A layer surface is not an xdg toplevel, so it is absent from taskbars that
  list toplevels, and workspace managers that only track toplevels often leave
  it up on every workspace. Both are consequences of the role, not extra
  requests, and both are `Unverified`.

Hide attaches a null buffer. The protocol then requires a commit with no
buffer and a new configure before the next buffer. The wgpu surface object is
kept across that. Show and focus do not create a second device.

If the compositor does not advertise `zwlr_layer_shell_v1`, or the socket
cannot be opened, the panel falls back to a winit toplevel. Those toplevel
limits apply again. GNOME does not implement this protocol. wlroots
compositors commonly do. KDE's support is not something this tree has run.

The seat maps evdev codes and XKB keysyms into the same key events the rest of
the editor sees. Key repeat is not synthesized. `zwp_text_input_v3` is not
bound, so an input method on the panel is not delivered. The accesskit adapter
is attached only to winit windows, so a layer-shell panel does not publish
that native tree. Custom cursors are not sent. None of this was exercised
against a compositor.

`panel_position`, hide, focus, stacking, skip-taskbar, above-fullscreen, and
all-spaces are `Unverified` on Wayland.

## Windows

A panel show sets `WS_EX_NOACTIVATE`, raises with `HWND_TOPMOST`, and uses
`SW_SHOWNOACTIVATE`. An explicit focus clears the no-activate bit and calls
`SetForegroundWindow`, because Windows delivers keys to the foreground window.
`panel_nonactivating` stays `Ignored`. `panel_above_fullscreen` is
`Unverified`: the request is `HWND_TOPMOST`, and no Windows session here has
shown it over a fullscreen window. Exclusive fullscreen is outside that
request.

`panel_all_spaces` stays `Ignored`. Windows has no stable public API that
pins a window to every virtual desktop. The undocumented shell interfaces
change by build and are not called.

Position, hide, focus, always-on-top, and skip-taskbar stay `Supported` on
the strength of the existing winit calls. This change did not re-run them on
a Windows machine.

## What this environment ran

Headless tests lock the placement math, the Win32 request numbers, the
Wayland keysym and evdev tables, and the support table. A Windows-only test
checks those numbers against the windows-sys headers. CI compiles the Linux
client and, on the Windows runner, the Win32 calls.

No Wayland compositor was driven. No Windows desktop was driven. The macOS
panel was not launched again. Do not treat a green Linux test run as proof
that a panel summons, stacks, or takes keys on those sessions.
