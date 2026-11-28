# rndr_browser

Optional embedded browser for [rndr](https://github.com/EntasisLabs/rndr).

The facade feature `browser` is **off** by default. It is the only way a
desktop build of `rndr` links this crate. `rndr_core`, `rndr_ui`, and
`rndr_paint` do not depend on it. Enabling `browser` does not enable `dialog`.
The crate is publish-eligible and is ordered immediately before the `rndr` facade. The optional dependency is versioned, so `cargo package` of the default facade resolves it even with `browser` off. 0.6.0 publishes it with the rest of the SDK.

The host owns the session. A guest page can navigate. It cannot call into
application commands, and this slice does not evaluate script or search the
page. A custom scheme is a navigation report. Downloads and file inputs are
host intents. Nothing is written, and no file panel is opened, until the host
answers. Destroying the session cancels intents that are still open.

## Isolated in-memory HTML artifacts

Use the separate `ArtifactPreview` API, **not** `Browser::navigate`, for
self-contained untrusted interactive HTML. It has no URL navigation, native
command/guest-message bridge, host script evaluation, file or download intents.
The macOS backend is off by default; explicitly enable the Cargo feature
`macos-private-webkit-artifacts`. Feature-off and non-macOS builds keep the public
API but return `ArtifactError::Unsupported` before native creation. The opted-in
backend uses fresh ephemeral state, a sandboxed opaque-origin main document,
immutable response-header CSP, source-free enforcement preflight, native resource/network/capability gates and explicit page close
on replacement/drop. Other backends return explicit unsupported errors without
falling back to ordinary browsing.

**Deployment caveat:** the complete macOS contract currently requires guarded
private WebKit SPI (notably disabling WebRTC and closing retired pages). It is
not an App Store-ready/public-API-only solution. Missing native gates refuse the
preview before HTML loading. This limitation needs a release decision and
revalidation on target OS versions.

See [usage, enforced policy, support limits, live vs headless evidence and the
read-only Medousa glue mapping](docs/isolated-artifacts.md). Run
`cargo run -p rndr_browser --features macos-private-webkit-artifacts --example artifact_preview` for the benign local
interaction/replacement example. The opt-in `artifact_policy_harness` executable
runs the real main-thread WKWebView integration probe; its observation channel
exists only in that executable, never in the library. Run it with
`--features macos-private-webkit-artifacts,native-artifact-harness`. The harness
feature alone does not enable private artifacts. See the contract document for
the repeatable default/opt-in release Mach-O selector regression; feature-off
is not a blanket App Store certification for an embedding application.

### Native artifact scrolling

`set_bounds(ClientRect)` defines a **viewport**, in logical parent-client
coordinates (including a nonzero x/y for inline placement). Native WebKit owns
wheel, trackpad phases, momentum and nested scrolling inside that child. The
embedding host should not translate its own accumulated wheel deltas into guest
script or forward the same event a second time. Host-owned regions outside the
child continue receiving ordinary host input.

The guest is deliberately a **main document**, not a scrolling sandbox iframe:
WebKit's asynchronous subframe presentation path can flash an entirely white
viewport after a large discrete wheel jump. A single-use in-memory response
carries `Content-Security-Policy: sandbox allow-scripts; ...`; the opaque-origin
sandbox does not depend on mutable HTML. See
[`MAIN_DOCUMENT_SCROLL_FIX.md`](../../examples/native-artifact-host/MAIN_DOCUMENT_SCROLL_FIX.md)
for the native comparison, security controls and evidence limits.

RNDR preserves authored overflow/layout and nested scrolling. Its trusted preview
copy appends one presentation rule, `html:root { overscroll-behavior:none!important }`,
to keep the sandbox document within its native viewport at the top/bottom/side
edges. This prevents elastic scrolling outside the document viewport during
fast finger/momentum gestures. It
is a presentation policy, not a security gate, raster-tile tuning, or a change
to stored/exported HTML. The original doctype remains first. Authored nested
overscroll policies are untouched. A guest `html,body { overflow:hidden }`
can intentionally prevent document scrolling while allowing a nested
`overflow:auto` panel. Do not globally override it. In particular, old Medousa
artifacts may contain an app-owned `style#medousa-artifact-host` that suppressed
scrolling for Tauri's outer auto-height layout. A viewport-based embedding must
remove that obsolete **app-owned** style on its preview copy, not strip authored
styles or alter the stored/exported original. This is an embedding compatibility
fix, not a reason to relax the opaque sandbox or add a guest bridge.

The opted-in backend also avoids redundant native `setFrame`/`setHidden` calls
for steady-state host polls; changed viewport geometry and visibility still
apply normally. This eliminates avoidable lifecycle churn, not a claim that all
WebKit raster-performance stalls have been measured or fixed.

The gated attached-window probe and native event driver reproduce that legacy
failure and verify its narrowly scoped removal:

```sh
cargo build -p rndr_browser \
  --features macos-private-webkit-artifacts,native-artifact-harness \
  --example artifact_scroll_harness
swift crates/rndr_browser/examples/artifact_scroll_input.swift \
  target/debug/examples/artifact_scroll_harness target
```

Run in a macOS Aqua session with input-synthesis permission; the optional final
`target` argument saves owned-window screenshots and requires Screen Recording
permission. The driver checks plain HTML, the exact legacy host stylesheet, and
a copy with only that stylesheet removed. It sends actual CGEvents through
AppKit (no JavaScript `scrollTo`) and asserts vertical/horizontal pixel wheel,
line wheel, phased finger/momentum, independent nested scrolling, and separation
between the bounded guest and host sidebar-like strip. The observation handler
exists only in the harness executable. Neither this feature nor the test adds a
production guest-message channel or implicitly enables private WebKit.

This probe passed on macOS 27.0.1 with Rust 1.100.0. It verifies synthesized native
gestures, not physical trackpad hardware or other platforms. Logs are written to
`/tmp/rndr-scroll-{plain,legacy,stripped-legacy}.log`. The Rust probe can also be
run interactively with `--legacy-host-style` or `--stripped-legacy-host-style`;
it closes after two minutes if not closed manually.

### Host popups above native HTML

A native child view composes **above** RNDR's GPU-painted window. Do not hide the
whole artifact for a toolbar tooltip, and do not assume painting a popup in the
underlying GPU frame will put it above WebKit. Use
`ArtifactPreview::set_overlays(&[OverlayBitmap])` for trusted host popups:

- `bounds`: logical top-left window-client placement, including transparent shadow extent.
- `width`, `height`, `rgba`: straight-alpha sRGB RGBA8, top row first. Pixel size is
  independent of logical size, so hosts can raster at the current display scale.
- `pointer_bounds`: `None` for passive tooltip input; `Some(client_rect)` for an
  interactive popup body. Only that body's native pointer hits route to the
  winit parent. Shadows and passive images do not intercept guest input.

The slice is back-to-front; `[]` removes all images. Admission is atomic: at most
32 images, at most 4096 pixels per side, exact byte lengths, at most 16 MiB
aggregate. Identical submissions do not rebuild native images. Overlays hide
with the preview; failures, replacement and drop retire native siblings before
the parent disappears. Submissions survive replacement and are installed on the
fresh preview on its next frame. The host owns popup rasterization, caching,
modal policy, animation invalidation, theme and display-scale updates. Reuse an
Exporter, paint only the popup subtree to transparent pixels, and cache unchanged
paint; do not synchronously read back the full app on every idle preview poll.

These are **host-created pixels**, never a guest screenshot, guest-message
channel, or extra browser capability. A rectangle cut out of WebKit would expose
an ugly solid host matte under translucent shadows; that approach was rejected.
The native alpha image preserves the live guest under shadows and rounded edges.

The full-Metal example supports `--overlay-probe --scroll-stress` (translucent
popup, native host button to toggle it, interactive popup body),
`--passive-overlay` (no input capture), and `--html /path/to/local-fixture.html`.
The browser-only scroll probe accepts the same `--html` fixture argument and a
test-only observer to report inner `scrollY` versus its maximum. That observer
never exists in the production library.

The matrix and older smoke ledger below describe ordinary `Browser`, including
its unsupported **host script-evaluation** API, not the separate artifact API.

## Matrix

`Implemented` means the code is in this crate. It is not a native certification.
`Unverified` means no live session in this change has shown the operating
system doing the thing. Headless tests do not move a cell.

| Backend | Capability | This slice | Native session |
| --- | --- | --- | --- |
| macOS WKWebView | navigation, show, hide, logical client bounds | Implemented | **Unverified** |
| macOS WKWebView | downloads and file input | Implemented | **Unverified** |
| macOS WKWebView | find | Unsupported | Unsupported |
| macOS WKWebView | script | Unsupported | Unsupported |
| WebView2 | navigation, show, hide, logical client bounds | Implemented | **Unverified** |
| WebView2 | downloads and file input | Implemented | **Unverified** |
| WebView2 | find | Unsupported | Unsupported |
| WebView2 | script | Unsupported | Unsupported |
| WebKitGTK X11 | navigation, show, hide, logical client bounds | Implemented | **Unverified** |
| WebKitGTK X11 | downloads and file input | Implemented | **Unverified** |
| WebKitGTK Wayland | navigation, bounds, downloads, file input | Unsupported | Unsupported |
| WebKitGTK | find | Unsupported | Unsupported |
| WebKitGTK | script | Unsupported | Unsupported |

Find and script stay unsupported until their own slices. WebView2 uses an
already installed runtime. A missing runtime is an error. This crate does not
download or bundle one. WebKitGTK on Wayland is unsupported: GTK and the host
window are different Wayland clients, so the guest cannot be a child surface.
A missing GTK display is an error.

## macOS smoke — outstanding

R3.1 is **not fully accepted**. No native macOS session was run for this
change. Do not mark the macOS cells `Supported` from the headless tests or from
a successful compile.

`cargo test` on macOS does not run the test below: the harness calls it off
the AppKit main thread, and WKWebView has to be created on that thread. The
test is ignored so CI typechecks it without pretending the session happened.

On a Mac, in a GUI session, from the repository root:

```sh
# Typecheck only. This does not perform the smoke.
cargo test -p rndr_browser --test macos_smoke --no-run

# The ignored test panics if it is not on the main thread.
# Run it only from a main-thread harness (a GUI process), not from
# `cargo test -- --ignored`, which still uses a worker thread.
cargo test -p rndr_browser --test macos_smoke -- --ignored --nocapture
```

What a main-thread run is expected to show:

1. A winit window opens. Its content view is flipped. `set_bounds` of
   `(10, 20, 300, 200)` logical points, then `frame`, places the web view at
   AppKit origin `(10, 20)`, not at `parent_height - 20 - 200`.
2. Resizing that winit window's content and applying the same rect keeps the
   origin at `(10, 20)`.
3. The same rect in an unflipped `NSView` of height 600 lands at y = 380, and
   at y = 280 after the parent height becomes 500.
4. Dropping the session removes the view. No file panel appears.

Server trust uses the system certificate check and does not install a
credential. Credential challenges are cancelled for both navigation and
downloads. WebKit's challenge object is an `NSProxy`, so `protectionSpace` is
sent with `objc_msgSend`. A checked send aborts debug builds before the proxy
can forward it.

On a Mac, debug and release:

```sh
cargo run -p rndr_browser --features native-auth-harness --example auth_harness
cargo run -p rndr_browser --features native-auth-harness --example auth_harness --release
```

`https://example.com/` must finish. `https://expired.badssl.com/` must fail the
load. A localhost HTTP basic challenge must be refused. The process must not
abort. Downloads and file input still need a native
acceptance session. A delayed title is covered by the headless reconcile test,
not by this smoke.

Record the OS version, machine, Rust version, date, and the observed result
before calling any cell `Supported`. This repository does not do that here.
Merging the macOS slice did not certify its download callback, bounds smoke,
or file input.

## Windows smoke

On Windows, with the WebView2 runtime already installed:

```sh
cargo run -p rndr_browser --example windows_smoke
```

The example parents a WebView2 controller in a Win32 window. It checks a
logical rect and a later resize in parent-client pixels, hides on a zero-area
rect without destroying the view, loads `https://example.com/`, fails
`https://expired.badssl.com/`, refuses a localhost basic challenge, cancels one
download, and accepts another into a host path. It does not trigger file
input. A missing runtime prints `BrowserError::RuntimeMissing` and exits 3.
The host message loop has to run; `frame` does not pump it. A green run is
still not an R6 `Supported` cell. Windows file input was not triggered.

## Linux X11 smoke

On Linux, with GTK 3, WebKitGTK 4.1, and an X11 display:

```sh
cargo run -p rndr_browser --example webkitgtk_smoke
```

The example creates an X11 window, the handle shape winit reports, and parents
WebKitGTK in a child window. `frame` drains pending GTK iterations because the
winit loop does not. A Wayland handle is refused before GTK starts. A missing
display is `BrowserError::Prerequisite`.

The example checks a logical rect and a resize in X geometry, unmaps on a
zero-area rect without destroying the child, refuses a localhost basic
challenge, cancels one download, accepts another, loads `https://example.com/`,
and fails `https://expired.badssl.com/`. An X button event on a file input did
not open the chooser in the session that was run. Wayland was not executed.
A green run is still not an R6 `Supported` cell.
