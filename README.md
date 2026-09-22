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
│  Clip Scroll Focus          │
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
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus |
| `rndr_render` | winit window, `Input` resource, instanced quads, 8×8 ASCII atlas |
| `examples/expand` | Expand / scroll / keyboard-focus demo |

Platform dependencies stop at `wgpu`, `winit`, and `pollster`. No layout engine, no text crate, no third-party ECS.

## Frame

`Startup` → `Update` → `Layout` (then the built-in propagate) → draw

Headless `App::update()` runs everything except the window. The desktop runner is the only thing that calls winit.

## Demo

```bash
cargo test -p rndr_core
cargo run -p expand
```

A window-filling page of rows. Click or Enter to expand. Arrows and Tab move focus. Escape collapses, then leaves the window. Wheel scrolls; focus keeps the current row on screen.
