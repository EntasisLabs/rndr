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
│  Caret Body                 │
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
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus, pane, caret, body |
| `rndr_render` | winit window, `Input` resource, instanced quads, 8×8 ASCII atlas |
| `examples/expand` | Expand / scroll / keyboard-focus / pane / caret / document-body demo |

Platform dependencies stop at `wgpu`, `winit`, and `pollster`. No layout engine, no text crate, no third-party ECS.

## Frame

`Startup` → `Update` → `Layout` (then the built-in propagate) → draw

Headless `App::update()` runs everything except the window. The desktop runner is the only thing that calls winit.

## Demo

```bash
cargo test -p rndr_core
cargo run -p expand
```

A tree column on the left and a detail pane on the right. Enter opens the pane and puts a caret on the title; type to rename the tree row. Tab or click the body to write a document; Enter in the body is a new line. Click a tree row opens the pane without editing. Right and Left expand and collapse the tree unless a caret is active. Escape leaves the caret, then the pane, then collapses, then leaves the window. Wheel scrolls; focus stays on the tree.
