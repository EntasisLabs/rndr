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
| WebView2 | navigation, downloads, find, script | Unsupported | Unsupported |
| WebKitGTK | navigation, downloads, find, script | Unsupported | Unsupported |

WebView2 and WebKitGTK stay unsupported until their own slices. Find and script
stay unsupported until theirs. This crate does not download or bundle a
browser runtime.

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

1. A titled window opens and a `WKWebView` is added to its content view.
2. `show`, then `set_bounds` of `(10, 20, 300, 200)` logical points, then
   `frame`. The view's AppKit frame matches that rectangle converted from the
   top-left of the content view. The view is visible.
3. `navigate` to a `data:text/html` document. Within a few seconds, `frame`
   reports a load outcome of finished or failed, and the URL is the navigation
   report. The load is not an application command.
4. A new rect, then `frame`, moves the view. A zero-area rect, then `frame`,
   hides the view and leaves it in the hierarchy.
5. Dropping the session removes the view. No file panel appears. No download
   is written.

Record the OS version, machine, Rust version, date, and the observed result
before calling any cell `Supported`. This repository does not do that here.
