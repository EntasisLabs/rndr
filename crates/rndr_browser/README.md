# rndr_browser

Optional embedded browser for [rndr](https://github.com/EntasisLabs/rndr).

The facade feature `browser` is **off** by default. It is the only way a
desktop build of `rndr` links this crate. `rndr_core`, `rndr_ui`, and
`rndr_paint` do not depend on it. Enabling `browser` does not enable `dialog`.
The crate is publish-eligible and is ordered immediately before the `rndr` facade. The optional dependency is versioned, so `cargo package` of the default facade resolves it even with `browser` off. Nothing in this tree uploads the crate.

The host owns the session. A guest page can navigate. It cannot call into
application commands, and this slice does not evaluate script or search the
page. A custom scheme is a navigation report. Downloads and file inputs are
host intents. Nothing is written, and no file panel is opened, until the host
answers. Destroying the session cancels intents that are still open.

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
