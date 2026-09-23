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
│  Split Caret Body Wrap      │
│  FsPath   Files             │
└──────────────┬──────────────┘
               │
     ┌─────────┴──────────┐
     │                    │
 Shell plugin      Files plugin
     │                    │
     │              std::fs (rndr_std)
     │
     ▼
 winit + wgpu (rndr_render)
```

`mob3` is the prototype of this cut: application structure in the core, the renderer as a plugin that never owns parenting. Expand is a hierarchy walk. Opening a node spawns children; collapse cascade-despawns them. The next layout pass rewrites rectangles. The GPU never hears about expand.

## Crates

| Crate | Role |
| --- | --- |
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus, pane, split, caret, body, wrap, `Files` |
| `rndr_std` | `std::fs` plugin |
| `rndr_render` | winit window, `Input` resource, instanced quads, 8×8 ASCII atlas |
| `examples/expand` | File-backed expand demo: folder tree, pane, splitter, caret, create, delete |

A blank `App` holds the world, `Input`, and `Viewport`. `Shell` registers the interaction systems. `StdFsPlugin` inserts the disk backend. `MemoryFs` is the headless backend. Platform dependencies stop at `wgpu`, `winit`, and `pollster`. No layout engine, no text crate, no third-party ECS.

Layout, hit-testing, and scroll are in logical points. `Viewport.scale` is physical pixels per point. The desktop plugin divides window size, the cursor, and pixel-wheel deltas by that scale, then multiplies rects by it when uploading instances. `Viewport.insets` shrinks root placement for a status bar or home indicator. `Input.pointer` is one contact: the mouse writes id `0`. A drag inside a scrollport follows that contact after 4 points. A drag that starts on the seam still resizes the split.

## Frame

`Startup` → `Update` → `Layout` (then the built-in propagate) → draw

Headless `App::update()` runs everything except the window. The desktop runner is the only thing that calls winit.

## Demo

```bash
cargo test -p rndr_core
cargo run -p expand
cargo run -p expand -- /path/to/dir
```

A tree column on the left and a detail pane on the right. Drag the seam to resize. The body wraps to the pane width; drag the splitter and the next frame reflows. The demo opens the first argument if it is an existing directory, otherwise `examples/expand/fixture`. Click a row to select it. Right lists a directory; Enter opens a file into the pane. Click a title or body cell to place the caret; Shift extends the selection. Type in the title to rename, Tab into the body to edit, Ctrl+S writes the file. A mark on the tree row means the file is dirty. Ctrl+N creates `untitled.txt` (Ctrl+Shift+N a folder) and starts a title edit. Delete removes a file or an empty folder. Collapse writes dirty children first. Hidden names and binary files are skipped for the body. Escape leaves the caret, then the pane.
