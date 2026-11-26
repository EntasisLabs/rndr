# Isolated in-memory HTML artifacts

`ArtifactPreview` is **not** `Browser::navigate` with a new URL spelling. It is a
separate, deliberately capability-free surface for self-contained untrusted HTML.
Ordinary `Browser` continues to own URL navigation, file/download intents and its
usual platform configuration. Do not use it as a fallback if artifact admission
or policy setup fails.

## Explicit build/distribution opt-in

The public `ArtifactPreview` / `ArtifactError` / `ArtifactState` API remains
available in every configuration. The implementation that references private
WebKit selectors is compiled **only** under both `target_os = "macos"` and the
non-default Cargo feature **`macos-private-webkit-artifacts`**. Ordinary `Browser`
does not require this feature and keeps its existing configuration and behavior.
Default RNDR/browser dependencies do not enable it. `native-artifact-harness`
does not imply it either; the native harness executable requires both features.

For an application that deliberately accepts the SPI distribution tradeoff:

```toml
[dependencies]
rndr_browser = { version = "0.6.0", features = ["macos-private-webkit-artifacts"] }
```

Without that explicit opt-in, or on a non-macOS target even with it enabled,
admitted artifact input returns **`ArtifactError::Unsupported`** before parent
access, native creation or HTML loading. The existing 2 MiB input admission
precedence is preserved (`TooLarge` for oversized source). There is never an
ordinary-browser fallback. Missing or ineffective required runtime controls also
return `ArtifactError::Unsupported` before HTML loading; actual policy compilation
failures/timeouts remain `Policy`.

Cargo features are additive and unified: another dependency can opt the package
in. Audit the **actual product's** resolved graph (`cargo tree -e features`) and
final release binary. `--all-features` deliberately opts in and is not a
private-API-free distribution build. Feature-off removes this backend's private
selectors, not a certification that the entire embedding app/dependency graph
meets App Store rules. Feature-on builds are **not App Store-ready/public-API-only**;
SPI presence does not establish continued isolation semantics after an OS update.

## Usage and ownership

```rust,no_run
use rndr_browser::{ArtifactPreview, ArtifactState, ClientRect};
# fn show(window: &impl raw_window_handle::HasWindowHandle) -> Result<(), Box<dyn std::error::Error>> {
// On the native UI thread, with a live parent window:
let mut preview = ArtifactPreview::attach(window, r#"
  <button onclick="this.textContent=++window.count">0</button>
  <script>window.count=0</script>
"#)?;
preview.set_bounds(ClientRect::try_new(0.0, 0.0, 640.0, 480.0)?)?;
preview.show();
// In the application's frame/event-loop integration, repeatedly:
match preview.frame()? {
    ArtifactState::Preparing => { /* native resource rules not installed yet */ }
    ArtifactState::Loading => { /* trusted envelope loading */ }
    ArtifactState::Ready => { /* local guest interaction can continue */ }
}
// Replacement creates another isolated session, not a navigation in the old one.
preview.replace("<p>Fresh state</p>")?;
// Drop before its parent window is destroyed.
drop(preview);
# Ok(()) }
```

The input is UTF-8 `&str`, capped at **2 MiB of source bytes**, including on
unsupported platforms. Empty input is valid. Admission happens before parent
access or native view creation. Escaping can expand the admitted source by up to
six times plus the fixed trusted envelope. The caller must separately bound its
fetch/decompression/UTF-8 decoding before constructing the string.

`attach` returns a hidden preview; drive `frame()` and the native event loop even
while preparing. No guest source is loaded until the native deny-all resource
rule list successfully compiles and is installed. Compiler errors and a
15-second compilation timeout produce sticky `ArtifactError::Policy` errors and
retire the failed view. A failed main-document load or content-process exit
produces `LoadFailed`. `Ready` describes the envelope load, **not** script
completion or an attestation of every possible guest behavior.

`show`/`hide` and bounds apply on the next frame. Zero-area bounds hide the view;
invalid bounds preserve the previous rect. Hiding does not discard or suspend
interaction state. `replace` preserves bounds and requested visibility but
recreates the native view, data store, process pool, controller and policy state.
Validation/synchronous creation errors preserve the old preview. Once a new
preview has been admitted, a later policy/load failure never restores the old
content. Replacement and drop explicitly close the old page; simply releasing a
WKWebView proved insufficient to stop its timers in the live probe.

## Enforced contract and limits

On the explicitly opted-in macOS path:

* `loadHTMLString(..., baseURL: nil)` loads only a trusted envelope. No file,
  daemon endpoint, HTTP origin, asset protocol or ordinary browser profile is
  assigned to the artifact.
* The escaped source is one `srcdoc` child with **only** `sandbox="allow-scripts"`.
  There is no `allow-same-origin`, popups, forms, downloads, modals or top-level
  navigation capability. The guest cannot edit its parent's sandbox attribute.
* A restrictive CSP is in the trusted envelope **before** the child. Srcdoc
  inherits that CSP; guest markup can tighten, not loosen it. Inline script and
  style plus data images are allowed. Connect, external scripts/styles/images,
  fonts, media, objects, workers, base changes and forms are blocked. `eval`,
  dynamic function compilation and WebAssembly compilation are not enabled.
* A compiled WebKit deny-all content rule list independently blocks resource
  requests, rather than relying on guest CSP declarations or text filtering.
* The navigation delegate admits exactly one host envelope and its one initial
  direct srcdoc child. All later navigation, custom/file/data/blob destinations,
  child browsing contexts, new-window targets and downloads are refused. Local
  DOM updates remain available. Artifacts needing nested iframes are unsupported.
* Every view uses a new nonpersistent website data store, process pool and empty
  user-content controller. Opaque-origin access prevents cookies/storage,
  parent DOM and service workers. No script handler, native command bridge,
  guest message API, navigation intent, download intent or file-selection intent
  is exposed by `ArtifactPreview`. It has no host script-evaluation API either.
* The native UI delegate denies file selection, drag-in and native context menus
  (including Save As). A local AppKit event monitor consumes Cmd-V only while
  this preview's subtree has focus, preventing clipboard files/images from
  entering through native paste. Ordinary browsing and host controls are not
  covered by that monitor. Programmatic clipboard access and permissions are
  disabled independently; normal typed local inputs still work.
* The sandbox/iframe permissions policy and existing deny decisions prevent
  camera, microphone, geolocation, display capture and other permission grants.
  This does not ask the host to decide whether to grant a guest's request.

**Important macOS deployment limitation:** public WebKit resource policy APIs do
not close the complete no-network contract. The live probe demonstrated that
`RTCPeerConnection` remained constructible with CSP `connect-src 'none'` and a
content rule list. The strict backend therefore requires WebKit **private SPI**:
verified preference setters/getters disable peer connections (ICE/STUN), DNS
prefetch, filesystem access and JS/DOM clipboard access; delegate SPI vetoes drag
and context menus; `_close` is required to retire the page. Missing/refused
preference or close gates return `ArtifactError::Unsupported` **before guest loading**. There
is no fallback to JS API shadowing or less isolated browsing.

This is **not an App Store-ready/public-API-only implementation**. SPI can change
across WebKit/OS releases. Revalidate on deployment OS versions; only the recorded
macOS version below was exercised live. A public-API-only distribution needs a
separately sandboxed renderer/network architecture or another enforceable engine
contract; dropping the WebRTC/lifecycle gates is not an acceptable substitute.

The privileged embedding host is outside the threat model: it must not obtain the
native child and add script handlers, bypass delegates/preferences, retain and
manipulate closed pages, route app-provided file/paste actions into it, or feed
native clipboard/drop data to guest scripts. The example harness adds an
**observation-only test handler**; the library does not do this, even with the
harness feature enabled. The rule compiler may cache the constant deny-all rule
on disk; it is never given artifact HTML. Ephemeral website data is not a promise
against OS swap, engine crash logs or a compromised host.

There are no CPU, DOM, image-decode, JS heap, wall-clock execution or paint quotas.
A malicious infinite loop can still consume a WebKit content process. This is not
a defense against WebKit/OS vulnerabilities, a dedicated-OS-process guarantee,
or a remote-content/browser surface. External dependencies must be removed or
safely embedded upstream; CDN loaders do not work. RNDR does not rewrite or fetch
them. No permission prompt or native bridge should be added to make one work.

## Support and evidence

| Backend | Result |
| --- | --- |
| macOS, feature enabled, with all required native gates | Implemented; native probe passed on macOS 27.0.1 (26A434), arm64 |
| macOS, feature disabled | `ArtifactError::Unsupported` before parent access; no private artifact backend compiled |
| macOS, feature enabled, missing/refused required control | `ArtifactError::Unsupported`; no guest loaded |
| Windows WebView2, Linux WebKitGTK/X11/Wayland, all other targets | `ArtifactError::Unsupported`; no browser or view is opened |

No unsupported backend silently uses its ordinary-browser implementation.

```sh
# Headless admission/escaping/navigation policy and existing browser regressions:
cargo test -p rndr_browser
cargo test -p rndr_browser --all-features
cargo check -p rndr_browser --all-targets --all-features
cargo clippy -p rndr_browser --all-targets --all-features -- -D warnings

# Real main-thread WKWebView, not cargo's worker-thread test runner:
cargo run -p rndr_browser --features macos-private-webkit-artifacts,native-artifact-harness --example artifact_policy_harness
cargo run -p rndr_browser --release --features macos-private-webkit-artifacts,native-artifact-harness --example artifact_policy_harness

# Benign visible example: click, type, move slider; fresh session every 15 sec:
cargo run -p rndr_browser --features macos-private-webkit-artifacts --example artifact_preview
```

### Verification record for this feature-gate revision

Test host: **macOS 27.0.1 (26A434), arm64**; `rustc 1.100.0
(0ef1bc386 2026-11-09)`, Cargo `1.100.0 (3acc79c0c 2026-11-05)`.
Only this OS/architecture was exercised live. Actual commands/results:

| Command | Result |
| --- | --- |
| `python3 crates/rndr_browser/scripts/check-artifact-feature-gate.py` | Default 0/13, harness-only 0/13, opted-in 13/13 private selectors; live artifact refusal/load and ordinary HTTP + JS pass |
| `cargo test -p rndr_browser` | 35 tests + 1 doctest passed; 1 existing native smoke ignored |
| `cargo test -p rndr_browser --all-features` | 35 tests + 1 doctest passed; same native smoke ignored |
| `cargo check -p rndr_browser --all-targets` (also with `--all-features`) | Both pass |
| `cargo clippy -p rndr_browser --all-targets -- -D warnings` (also with `--all-features` before `--`) | Both pass |
| `RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_browser --no-deps` (also with `--all-features`) | Both pass |
| `cargo check -p rndr_browser --lib --target x86_64-pc-windows-msvc` (also with `--features macos-private-webkit-artifacts`) | Both compile; no Windows live run |
| `cargo check -p rndr_browser --lib --target wasm32-unknown-unknown` (also with `--features macos-private-webkit-artifacts`) | Both compile unsupported branch; no wasm live run |
| `cargo check -p rndr --no-default-features --features browser` and `cargo check -p rndr --no-default-features` | Both pass |
| `cargo fmt --all --check` and `git diff --check` | Pass |

Both live harness commands shown above passed, in debug and release, including
all eleven missing-control injections followed by the complete isolation,
zero-loopback-traffic, replacement/drop timer-retirement and ordinary-browsing
checks. Logs are `target/artifact-evidence/revision-*.log`; the binary regression
also retains the inspected executables and hashes as described below.
Existing workspace manifest warnings (redundant homepage/unused workspace
dependency) remain; scoped strict Rust/Clippy/Rustdoc checks passed. This is not
Linux/Windows native validation, universal-binary validation, or release/UI
certification. No additional macOS version is claimed supported by this evidence.

### Repeatable release binary gate regression

```sh
python3 crates/rndr_browser/scripts/check-artifact-feature-gate.py
```

This macOS-only regression fails rather than claiming success on another host.
It builds the representative `artifact_build_probe` release executable in three
configurations: default, only `native-artifact-harness`, and explicitly opted-in.
All executables **actually call** both `ArtifactPreview::attach` and `Browser`;
the default cases assert typed refusal and no child view, while the opted-in case
attaches/replaces/loads/retires a real preview. Every configuration also loads a
loopback HTTP fixture through ordinary `Browser` and verifies its JavaScript title.
The fixture handles WebKit speculative connections as well as the document load.

The check inventories **all 13 current backend private selectors** from the
macOS backend sources, including preference macro arguments, `_close`, drag and
context-menu delegate methods. It scans all file-backed Mach-O sections for exact
NUL-delimited selector strings: **0/13** in both feature-off executables and
**13/13** in the feature-on positive control. It also records `otool`'s
`__objc_methname` section dump. Do not confuse libc's imported `_close` *linker
symbol* with a WebKit `_close` selector; the linker symbol table is not an ObjC
selector section. The parser currently requires a thin 64-bit Mach-O (tested
arm64); universal product binaries need each architecture inspected separately.

Evidence goes to `target/artifact-evidence/feature-gate/`: selector inventory,
three copied binaries and SHA-256 digests, section dumps and native run logs.
Update the inventory extractor if adding a different selector declaration form;
this is a regression for this backend, not a general private-API certification.

### Missing-control fault injection

The native harness temporarily overrides `respondsToSelector:` **only in the
harness process**, denying one required control at a time (all ten preference
setters/getters and page close). It calls the unmodified public constructor and
asserts typed `Unsupported`, no attached child, and no pending child after pumping
the loop. The override is restored before the normal live suite. No library test
switch, environment variable, or weakened production policy is provided.
A unit test separately covers an available-but-ineffective preference readback.
The full isolation/cleanup suite is rerun in the same executable after restoration.

The native harness deliberately instruments the actual public preview and asserts
local interaction, inherited policy despite hostile meta/base/resource markup,
data images, opaque parent/storage/cookie isolation, blocked service workers,
fetch/XHR/WebSocket/file/custom-scheme requests, workers, WebRTC, popup/top-nav,
permissions, forms/file-input and download/navigation attempts. A local TCP
fixture sees **zero** artifact connections, a UDP socket sees zero STUN packets;
the same TCP fixture then serves an ordinary `Browser` URL with working JS. It
also checks fresh native data-store identity and JS/window state, retained bounds,
hide/show, admission failure preserving the current view, replacement stopping old
timers, and drop/late policy completion. The drag veto is directly invoked as a
native delegate test; this is not an end-to-end physical Finder drop test.

Headless checks are not native security evidence. The native probe uses real
WKWebView and event-loop pumping with a non-visible native parent: it is not
manual keyboard/mouse/drag/clipboard, VoiceOver, IME, visual QA or a comprehensive
network packet capture. Physical Cmd-V/file-drop/context menu verification on a
visible deployment build remains a release check. The existing ignored macOS
browser smoke test has not thereby become a completed UI certification.

## Medousa mapping — inspected read-only, not implemented here

The inspected checkout was `/Users/theelevators/medousa/medousa`; no files there
were modified. `apps/medousa-home/src/lib/components/environment/PresentationFrame.svelte`
loads prepared HTML with `srcdoc` and `sandbox="allow-scripts"` (around lines
584–586). That establishes the intended benign local-script/opaque-origin model,
not permission to open a normal native browser at a daemon URL.

The same component has **application glue beyond the renderer prerequisite**:
artifact fetch/edit/delete/source/full-view actions; theme/embed-mode preparation;
height messages and caps; a source-window-checked message receiver (around 434)
for runtime events/probes, subscribed feed-tail requests, scoped component store
get/set/delete/list and safe vault navigation. Those calls dispatch to Medousa's
daemon, stores and layout. They are not browser features and must not be ported
as a generic command bridge into this capability-free RNDR API. Persistent
component store and feed access in particular are **not** local browser storage.
This strict preview cannot make those bridge-dependent components fully work.

`apps/medousa-native/src/timeline.rs::artifact_part` (around 1156–1172) renders image
references but explicitly labels other artifacts **“reference; not an embedded
preview”**. Tool-run references and UI artifacts are gathered into the timeline,
not native HTML surfaces. `apps/medousa-native/src/chat.rs` still submits
`liquid_interactions: vec![]` (around 209); Liquid action routing is a separate
contract from interacting with an artifact's local DOM.

Remaining Medousa work:

1. Fetch bounded HTML bytes through the **owning authenticated workshop/session**,
   not through a URL navigable by the guest. Admit MIME and UTF-8/size on the host.
2. Own one `ArtifactPreview` per displayed artifact/revision; lay it out in logical
   client coordinates; apply clipping/occlusion/visibility/focus; drive frames;
   replace on revision and drop on close, session/workshop switch or stale result.
   Add the Native open/return-to-chat flow, loading/refusal UI and source/edit/full
   view actions. Do not add file/paste actions targeting the embedded native child.
3. Choose host-owned height/layout policy (there is no guest height channel here),
   theme/prepared self-contained content and explicit fallback **UI**, not a less
   isolated browser fallback. No daemon origin or CDN dependencies in the guest.
4. Treat any future feed/store/runtime/height/vault bridge as a separate scoped
   security design with source/revision binding, schemas, authorization, quotas
   and lifecycle checks. It would be a different capability contract, not a
   silent extension of `ArtifactPreview` or a raw native command dispatcher.
5. Wire Liquid interactions independently. Validate the real Native workflow and
   decide whether the private-WebKit deployment limitation is acceptable before
   enabling this preview in a release.
