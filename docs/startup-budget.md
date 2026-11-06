# Startup budget

A resident panel is created hidden and shown later from a hotkey. Device
creation belongs to process start, not to that show.

`device_init_plan` is the split:

- The window is visible when it is created: the UI thread waits for the
  adapter and the device, so the first painted frame is not a race.
- The window starts hidden: the UI thread creates the window and the wgpu
  surface, then adapter, device, and pipeline creation run on a `rndr-gpu`
  thread. The event loop is not blocked on that work. Show and focus do not
  start a second device. If they arrive while the worker is still running,
  they are kept and applied when the worker finishes.

Hide and show keep the same surface and the same device. Each window still
has its own device.

The swapchain stays `AutoVsync` for frames that are presented. A hidden
window does not present, so vsync is not on the hidden-start path. The first
present after a show waits for the display's vsync like any other frame.

## What is measured

`StartupTrace` is written into the world. Durations are from the start of
resume:

- `window_ready`: the window or layer surface exists.
- `device_ready`: the device is installed. Empty while the worker is running.
- `first_present`: the first successful present. Empty while the window has
  not presented, including a hidden window that has not been shown.
- `device_init_blocked_ui_thread`: true only for the visible-at-start plan.

## What this environment did not measure

No window was presented here, so there is no millisecond number for
time-to-first-present on a real display. The budget is the phase rule above,
not a duration invented for this machine. A summon that arrives after
`device_ready` does not include adapter or device creation. A summon that
arrives before `device_ready` waits for the init that already started.
