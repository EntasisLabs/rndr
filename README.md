# rndr

A Bevy-shaped ECS for desktop applications. Core owns the world. wgpu draws a flat list.

```
Application
    │
    ▼
┌─────────────────────────────┐
│         rndr_core           │
│  Entities  Components       │
│  Hierarchy Layout Expand    │
│  Clip Scroll Focus Pane     │
│  Caret Body FsPath          │
└──────────────┬──────────────┘
               │
        render plugin
               │
               ▼
     winit + wgpu (rndr_render)
```

`mob3` is the prototype of this cut: application structure in the core, the renderer as a plugin that never owns parenting. Expand is a hierarchy walk. Opening a node spawns children; collapse cascade-despawns them. The next layout pass rewrites rectangles. The GPU never hears about expand.

## Crates

| Crate | Role |
| --- | --- |
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus, pane, caret, body, files |
| `rndr_render` | winit window, `Input` resource, instanced quads, 8×8 ASCII atlas |
| `examples/expand` | File-backed expand demo: folder tree, pane, caret, Ctrl+S |

Platform dependencies stop at `wgpu`, `winit`, and `pollster`. No layout engine, no text crate, no third-party ECS.

## Frame

`Startup` → `Update` → `Layout` (then the built-in propagate) → draw

Headless `App::update()` runs everything except the window. The desktop runner is the only thing that calls winit.

## Demo

```bash
cargo test -p rndr_core
cargo run -p expand
```

A tree column on the left and a detail pane on the right. The demo opens `examples/expand/fixture`. Right lists a directory; Enter opens a file into the pane. Type in the title to rename, Tab into the body to edit, Ctrl+S writes the file. Hidden names and binary files are skipped for the body. Escape leaves the caret, then the pane. Unsaved edits die if you collapse the parent.
