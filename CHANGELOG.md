# Changelog

## 0.1.0

First published release.

- Entity world: hierarchy, components, events, tasks, and timers
- Layout in logical points, including rows, columns, clip, scroll, splits, tabs, and overlays
- Text input: caret, fields, wrapping, a gap buffer, decorations, and input-method preedit
- Paint step that walks the world and returns colored quads
- Desktop runner that opens a winit window and presents those quads with wgpu
- Text shaping and rasterization, with Inter, Noto Sans Arabic, and Noto Color Emoji bundled
- Global hotkeys on macOS, Windows, X11, and the Wayland global shortcuts portal
- Tray icon and menu on macOS, Windows, and Linux (StatusNotifierItem)
- Window commands: show, hide, focus, hide on blur, frameless panels, and always on top
- Filesystem backends for the real disk and for an in-memory tree
