# Desktop apps: a small end-to-end starter

`examples/desktop` is **Desktop Workshop**, an offline runnable reference built
on existing rndr primitives. `crates/rndr_app` is its small, opinionated app kit, published at the same
0.6.0 version as the rest of the workspace. The example itself stays
`publish = false`. The kit deliberately does not replace ECS, layout,
`Shell`, `Chrome`, `TextField`, `Theme`, the task runtime or the renderer.

This is a desktop-readiness increment, **not a cross-platform quality signoff**.
See [the evidence and acceptance checklist](desktop-readiness.md).

## Run and reproduce

From the repository root, with the pinned Rust toolchain and native desktop
libraries installed:

```sh
# Check disk first. This work uses ONE target directory, no incremental/debug data.
df -h .
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0
cargo test -p rndr_core -p rndr_paint -p rndr_app -p desktop --locked --offline
cargo clippy -p rndr_app -p desktop --all-targets --locked --offline -- -D warnings
cargo fmt -p rndr_app -p desktop -- --check
cargo run -p desktop --locked --offline -- --memory
```

`--offline` requires dependencies already fetched. On a new development machine,
fetch the locked dependencies first with `cargo fetch --locked`. Do not point
Cargo at another undertaking's writable target. Recovery uses this checkout's
own target and leaves the stranded undertaking and its original evidence intact.

The normal launch stores preferences at:

- macOS: `~/Library/Application Support/org.rndr.desktop-workshop/preferences.json`
- Windows: `%APPDATA%/org.rndr.desktop-workshop/preferences.json`
- Linux: `$XDG_CONFIG_HOME/org.rndr.desktop-workshop/preferences.json` (absolute
  XDG path), otherwise `~/.config/org.rndr.desktop-workshop/preferences.json`.

`--memory` never reads or writes preferences; use it for review. For an isolated
persistence exercise, pass `--preferences target/review-home/preferences.json`.
No credentials, provider configuration, learning logic or network calls exist
in this example. Never enter secrets in ordinary fields.

### A five-minute manual acceptance pass

1. Launch; check the heading, description, empty state and focused action.
   Start the local sample and see Loading → Ready after about 1.2 seconds.
2. Start again, cancel before completion, wait two seconds: Cancelled must
   remain. Simulate an error; Retry must succeed. While busy, Start and
   Simulate Error are disabled; Cancel exists only while busy; Retry exists
   only after failure. Switching pages does not silently cancel work.
3. Open Settings; type/paste a Unicode display name. Tab/Shift-Tab cycle only
   visible enabled controls; Enter activates a button. The focused control
   changes fill. Invalid empty/overlong names produce validation copy.
4. Cancel changes and reopen: draft was discarded. Save a name, quit, restart
   with the same preferences path: the saved name survives. Save itself never
   starts a task. A write error must leave the committed value unchanged.
5. Resize to 420×640 and scroll with pointer/wheel. Check text wrapping, focus
   visibility, field horizontal caret scrolling, control reachability and no
   overlap. Then perform the native IME/clipboard/VoiceOver/DPI checks in the
   readiness document. A successful unit test is not a substitute.

### Offscreen GPU evidence

```sh
cargo run -p desktop --locked --offline -- --export target/desktop-evidence/render
```

This writes 880×760 at 1×, 1760×1520 at 2×, and narrow Settings at 420×640.
The runner uses Parley and the existing `Exporter`, including actual GPU
readback and PNG encoding. It checks that images are not unexpectedly uniform.
These are **app-authored offscreen renders**, not desktop screenshots, visual
approval, native input evidence or proof of monitor-scale behavior. Font
selection can vary with installed system fonts. Inspect the images yourself.

## Use the kit without learning another architecture

The kit is an optional low-level ECS starter, not a replacement for `rndr_ui`'s
retained semantic controls or a default dependency of the facade. From 0.6.0:

```toml
[dependencies]
rndr_app = "0.6.0"
rndr_core = "0.6.0"
```

Install `rndr_app::FormNavigation` before application Update systems. It installs
core's Chrome/generic policy. Register `apply_scroll` and `apply_scrollbars` at
the front of Input for scrollable content, as the example does. Shell is for
file-tree applications and is not needed by this starter. Create normal ECS
entities using `column`, `label`, `button` and `field`, or write components
directly. The helpers return entity IDs so callers retain the escape hatch.
They use Theme colors and 12-point gaps / 24-point page insets / 44-point
minimum controls / 16-point body / 24-point heading typography. Labels wrap;
fields are single-line/ellipsis; buttons grow to fit wrapped labels.
`field` supplies `Placeholder` plus a persistent `Semantics` name for its caption;
placeholder/value are not persistent accessible names.

`FormNavigation` installs core's idempotent `GenericInteraction`, not a second
input router. Kit fields opt into `FieldTransactions` so core applies text,
Tab and submission chronologically, respecting composition. Custom fields can
supply their own transaction callback. Kit buttons use `ReleaseActivation`
and deliver accepted requests through `ActionIntents`, not the legacy `Fired`
channel. Hosts drain their owned requests during Update and revalidate targets
after earlier actions; synchronous policies can supply an `ActivationHandler`. Canonical Tab order skips hidden/disabled controls
and respects `TabStop`, popup ownership and core focus validation. This generic
policy deliberately replaces Shell's file-tree keyboard policy for the app.
Single-window starter behavior is not multi-window native acceptance.

`navigate` hides/shows existing page subtrees and moves focus; drafts remain
ordinary component data. The example decides when to discard/commit a draft.
`ViewState` provides explicit Empty/Loading/Ready/Error/Cancelled copy but
**does not own application transactions**. The example owns its task handle,
drops it to cancel, and validates completion epochs before applying results.
No second executor, renderer, reactive runtime or widget tree is introduced.

`Preferences` reads bounded JSON (16 KiB), distinguishes missing from invalid,
and saves through a unique sibling temporary file then rename. On Unix the
temp is mode 0600. Failed writes preserve the old destination. It is **not**
secure storage, multi-writer synchronization, encrypted storage, schema
migration or a crash-durable database. Applications own a versioned schema,
semantic validation and explicit error UI. The example rejects unsupported
schema versions on load and reports defaults without overwriting the file.
A subsequent explicit Save replaces that file, as the warning explains.
Ordinary text events and clipboard contain plaintext; do not use these
helpers for a credential field. OS secure-store work is separate.

## Local macOS .app distribution

Developer prerequisites: macOS, the Rust toolchain, Xcode Command Line Tools,
`python3`, `iconutil` and `plutil`. No paid services, credentials or signing
identities are needed for this local build. The icon generator is stdlib-only
and draws original geometric artwork; no download or external asset is used.

```sh
# Default is an optimized release build; check disk first.
scripts/package-desktop-macos.sh
# Or reuse an existing bounded development build for local validation:
scripts/package-desktop-macos.sh dev
open "target/bundle/Desktop Workshop.app"
# Review with no owner preferences:
open -n "target/bundle/Desktop Workshop.app" --args --memory
```

The script builds only `desktop`, offline and locked; creates a real bundle
with application identity, executable, high-resolution flag, original `.icns`
icon and Info.plist; and validates the plist. It honors `CARGO_TARGET_DIR`,
bounds jobs/debug/incremental output, checks free space and **refuses to
replace an existing bundle**. Move an older bundle aside deliberately before
rerunning; there is no broad cleanup. It does not sign, install globally,
modify quarantine, register login items or call the network. A toolchain's
ad-hoc executable signature is not a Developer ID distribution signature.

End users receive the **whole `.app`**, not a `target/debug/desktop` path and
not this repository. They need no Rust or Cargo. For a local user install,
copy the `.app` into `~/Applications` using Finder, then double-click it.
The executable and fonts needed by rndr are in the binary/bundle; system
frameworks still come from macOS. Verify on a clean Mac before distributing.
A local build is for the build machine's architecture, not a universal app.
The reference bundle version is example metadata, not a published rndr release.

Inspect actual build requirements before promising OS support:

```sh
file "target/bundle/Desktop Workshop.app/Contents/MacOS/DesktopWorkshop"
otool -L "target/bundle/Desktop Workshop.app/Contents/MacOS/DesktopWorkshop"
vtool -show-build "target/bundle/Desktop Workshop.app/Contents/MacOS/DesktopWorkshop"
plutil -lint "target/bundle/Desktop Workshop.app/Contents/Info.plist"
```

### Signing and shipping (guidance, deliberately not executed)

- Local unsigned/ad-hoc bundles are not notarized. Gatekeeper can reject an
  app downloaded from another machine. Do not tell users to disable security
  controls or strip quarantine as a distribution strategy.
- For public macOS distribution, use your own Developer ID Application
  identity, sign the completed bundle with hardened runtime and timestamp,
  verify with `codesign --verify --deep --strict --verbose=2`, then submit a
  properly archived signed app through `xcrun notarytool` using a locally
  managed Keychain profile. Staple and validate the ticket, then assess with
  `spctl --assess --type execute --verbose=2` on a clean target Mac. Sign any
  nested code inside-out; do not rely on `--deep` to perform signing.
- Do not embed credentials or certificates in the repo, CLI examples or logs.
  Review entitlements deliberately; this sample needs no blanket security
  exceptions. Mac App Store sandboxing/entitlements are a different workflow.
- Build/test each supported architecture with a compatible deployment target.
  This increment does not ship a universal binary, DMG, updater, signed
  installer, notarization workflow or App Store release.

## Historical Learn Anything follow-up (original investigation)

The original investigation reported reading its README, VERIFICATION, settings
schema, UI helpers and navigation code. This section is preserved context, not
a fresh review of that downstream checkout. It duplicates static-label/field/button
scaffolding, form Tab routing, settings drafts and request lifecycle patterns.
Its notes still explicitly withhold native visual/IME/clipboard/accessibility
and live-provider signoff. The old text-visibility workaround is already gone
on current rndr main; this increment preserves that fix and its regressions.

No Learn Anything or Medousa files were changed. After independent review,
Learn Anything can separately adopt the generic kit helpers, retain its strict
nonsecret route schema and session-credential boundary, and own its learning
transactions, provider consent and error classification. A shared preference
helper must not serialize its session credential or live-consent state. Real
provider acceptance remains app-owned and requires separate cost authorization.
