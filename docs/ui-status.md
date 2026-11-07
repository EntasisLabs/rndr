# Semantic UI status

## The tree

`rndr_core` is the headless world. `rndr_paint` is one frame. `rndr_std` is the
filesystem. `rndr_term` is the optional pseudoterminal. `rndr_render` is the
only OS window and GPU, and it also does offscreen capture. The `rndr` facade
re-exports those. `rndr_ui` and `rndr_app` are opt-in and depend only on core.
`rndr_ui` is the component library. `rndr_app` is a starter (form navigation,
spacing, draft preferences), not that library. Both publish at 0.4.0 with the
facade. `publish = false` stays on the examples, including desktop. The
[package policy](#package-policy) records the upload order.

What the kits and the core document model cover, with tests in this repo:

- Semantic themes, cards, controls, collections, data editors, and selection.
- [`ListView::mount_virtual`](virtual-list.md) and [`ListView::mount_variable`](virtual-list.md): a viewport window over a longer logical list. Variable rows keep an item anchor plus a milli-point remainder. The retained `ListView` is still a bounded list. Headless anchor math is not native certification.
- [Markdown host slots](markdown-host-slots.md): a slot is anchored to a source
  byte range, so an insert above it or a streaming append keeps the control.
  Height is measured from the mounted subtree. The host owns the children. The
  parser does not run them. Copy emits the original markdown.
- [Component capture](component-capture.md): `capture` crops the viewport
  composite to the target. `capture_isolated` paints that subtree on its own,
  so a sibling cannot cover it. Metadata is ids, bounds, clips, and hidden
  state. Accessible names stay out, including for public content.
  `ObservationSensitive` still denies the whole capture. CI builds
  `rndr-capture`, `component_capture`, and `structured_gallery`.
- [Overlay requests](overlay-platform.md) for a Wayland layer-shell panel and a
  Windows non-activating topmost window. The `platform_support` table is the
  authority for each capability. `Unverified` means the runner sends the
  request and no session in this repo's checks has shown the OS honor it.
- [Hidden-window startup](startup-budget.md): device creation for a window that
  starts hidden runs off the UI thread. The surface is still created on the UI
  thread and reused across hide and show. No millisecond time-to-first-present
  was measured here, so there is no numeric budget.
- [Headless IME and complex scripts](ime-complex-scripts.md): preedit, commit,
  cancel, and caret keys on the text surfaces, plus the known half-em column
  estimate in `reading_layout`. That is not an OS input-method certificate.

## Contracts that still hold

`UiPlugin` installs `apply_scroll` and `apply_scrollbars` at the front of
Input, before focus reveal. `FormNavigation` in `rndr_app` installs those same
two systems. A second registration is safe: the wheel is consumed, and a drag
is measured from the last applied position. Reinstalling `UiPlugin` is a
no-op. Neither plugin installs `Shell`.

The facade's `ui` feature exposes `rndr::ui`. Do not glob it together with
core. Root `Button`, tray `Item`, `Theme`, and the core virtual `List` keep
their core meanings.

On collection rows, screen-reader Activate follows click and Space and
selects. Keyboard Enter emits `Activated(id)` and does not select.

## What this environment did not prove

These checks ran on a Linux VM. They did not drive a Wayland compositor, a
Windows desktop, or a macOS panel. Do not read `Unverified` rows, or a green
CI matrix, as those sessions.

Still an open manual matrix, on macOS and on Windows, and for a real Linux
input method as well:

- Japanese, Chinese, and Korean preedit, commit, and cancel, including the
  candidate window.
- Arabic caret movement and bidirectional display.
- Physical DPI.
- The same six text surfaces: field, prompt, editor, inspector, source
  session, and structured editor.

The live Unix probe (`observe`) is a separate check from headless capture. It
records what the window submitted. It does not prove compositor presentation.

## Package policy

Workspace version **0.4.0**. `scripts/publish-crates.sh` uploads these crates
in this order:

1. `rndr_core` — no workspace dependencies.
2. `rndr_paint` — depends on `rndr_core`.
3. `rndr_std` — depends on `rndr_core`. No behavior change in this release.
4. `rndr_term` — depends on `rndr_core`. Its dev-dependency on `rndr_paint` is resolved when the package is verified, so paint is already published. No behavior change in this release.
5. `rndr_ui` — depends on `rndr_core`. Its dev-dependency on `rndr_paint` is resolved the same way.
6. `rndr_app` — depends on `rndr_core`. It does not depend on `rndr_ui`.
7. `rndr_render` — depends on `rndr_core` and `rndr_paint`.
8. `rndr` — depends on `rndr_core`, `rndr_paint`, `rndr_std`, and `rndr_render`. Optional `term` depends on `rndr_term`. Optional `ui` depends on `rndr_ui`. The dev-dependency on `rndr_ui` is also resolved from the registry.

Examples stay `publish = false`, including `desktop` and `source-session`. `rndr_browser` is also `publish = false`. The facade feature `browser` depends on it and is off by default. The eight-crate upload above does not include it, and publishing `rndr` with that feature enabled waits until `rndr_browser` itself is published.

0.4.0 is a 0.y release. Shipping these crates does not promise a stable 1.0 API. Experimental source sessions and the structured document stay experimental in their module docs.

`cargo package` in CI uses `--no-verify` for the eight crates. Their 0.4.0 dependencies are not on the index until this release is published, so a registry rebuild of a dependent crate cannot succeed in CI beforehand. The job still checks tarball membership, licenses, and the paint and render snapshot sets. `scripts/check-facade-archive.py` checks that the facade manifest normalizes, including optional `ui` at the same version as `rndr_ui`. `scripts/publish-crates.sh --dry-run` runs `cargo publish --dry-run` for each crate whose workspace dependencies are already on the index.

## Evidence

Tracked files under `evidence/` stay where they are. History is not rewritten
here. New captures, paint dumps, and logs go in [`local-captures/`](../local-captures/README.md),
which is gitignored except for its note. New files under `evidence/` are
ignored too, so the archive does not grow by accident. Do not force-add
either. A review note that belongs in git should be a short summary with
commands, exit results, and hashes, not the images.
