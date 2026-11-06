# Semantic UI status

## The tree

`rndr_core` is the headless world. `rndr_paint` is one frame. `rndr_std` is the
filesystem. `rndr_term` is the optional pseudoterminal. `rndr_render` is the
only OS window and GPU, and it also does offscreen capture. The `rndr` facade
re-exports those. `rndr_ui` and `rndr_app` are opt-in and depend only on core.
`rndr_ui` is the component library. `rndr_app` is a starter (form navigation,
spacing, draft preferences), not that library. `publish = false` stays on
`rndr_ui`, `rndr_app`, and the desktop example. The [package policy](#package-policy)
records which 0.4.0 crates are uploaded.

What the kits and the core document model cover, with tests in this repo:

- Semantic themes, cards, controls, collections, data editors, and selection.
- [`ListView::mount_virtual`](virtual-list.md): a viewport window over a longer
  logical list. The retained `ListView` is still a bounded list.
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

Workspace version **0.4.0**. Versions move together, including path
requirements. `publish = false` is what keeps a crate off crates.io; a matching
version number does not publish it.

Uploaded by `scripts/publish-crates.sh`, in order: `rndr_core`, `rndr_paint`,
`rndr_std`, `rndr_term`, `rndr_render`. `rndr_std` and `rndr_term` have no
behavior change in 0.4.0. They stay in the set so dependents resolve one
version.

Left unpublished:

- `rndr_ui`. The manifest says the dependency and publication policy is not
  release-ready. The crate is the semantic UI in this checkout. Publishing it
  would make that API a crates.io surface. This release does not do that.
- `rndr_app`, and every example, including `desktop` (Desktop Workshop) and
  `source-session`.

The `rndr` facade is version 0.4.0 here and is not in the upload list. It does
not set `publish = false`. Cargo still will not accept it: the optional `ui`
feature depends on `rndr_ui` with a path and a version, and an optional
dependency has to resolve from the registry at publish time. Removing `ui`,
or publishing `rndr_ui`, would change the public surface. Neither is decided
here. crates.io `rndr` stays at 0.3.1, which has no `ui` feature and no
`observe` feature.

`scripts/check-facade-archive.py` packages the facade with `--no-verify` and
`--exclude-lockfile`, then checks normalization and source bytes. CI's package
step excludes the facade, `rndr_ui`, `rndr_app`, and the examples. Neither
check is permission to upload the facade.

## Evidence

Tracked files under `evidence/` stay where they are. History is not rewritten
here. New captures, paint dumps, and logs go in [`local-captures/`](../local-captures/README.md),
which is gitignored except for its note. New files under `evidence/` are
ignored too, so the archive does not grow by accident. Do not force-add
either. A review note that belongs in git should be a short summary with
commands, exit results, and hashes, not the images.
