# Epic: native desktop shell capabilities

**Status: R0 accepted by the repository owner on 2026-11-06 at 15:03 America/Phoenix, in review of PR #80. The owner accepted this record and all of its proposed platform policies, and authorized the current implementation units R1 and R5.1. Browser work (R3) stays queued behind that pair. R2, R4, R5.2, and R6 stay sequenced and are outside this authorization. This file does not contain their implementation, does not merge them, and does not certify a platform. Every native certification cell below stays Unverified.**

Created 2026-11-06 against rndr `cc25bb2722b2d01ec3d4f8f3a0c452863cfeac9c` (`main`, 0.4.0). The original assignment was documentation only. The owner's acceptance is quoted under R0.7. R1 and R5.1 are authorized as separate pull requests branched from `main`, in parallel, and are not stacked on this record. This file still adds no production code, tests, feature flags, crate manifests, or native runs.

## Read / continue here

This file is the entry point. It uses the repository's local `docs/design/` proposal, checkpoint, and gate convention, as in [the document-editing epic](native-document-editing-epic.md) and `A1-api-decisions.md`. There is no separate tracker and no external issue dependency.

Build on these contracts. Refresh them before an implementation unit edits the code they describe.

- [Virtual lists](../virtual-list.md) — uniform-height `ListView::mount_virtual`, `ItemId` identity, recycled rows.
- [Data components](../data-components.md) — retained list and tree budgets, `SelectionMode::None` and `Single`.
- [Overlay windows](../overlay-platform.md) and `Availability` in `crates/rndr_core/src/window.rs` — `Supported`, `Ignored`, and `Unverified`.
- [Desktop readiness](../desktop-readiness.md) — historical macOS notes. They are not fresh certification for any cell in this epic.
- [Document-editing epic](native-document-editing-epic.md) — grammar, diagnostics, and live IME. That epic stays where it is.

`README.md` stays unchanged. This acceptance update remains the single design document.

## Starting point

A native-shell audit of 27 shell views found **11** that map onto widgets already in the tree, **12** that map at reduced fidelity, and a remainder with no primitive. The missing families that block a desktop shell are an embedded browser, variable-height virtualization, document editing, and OS services. Diff presentation, range selection, and a positioned file drop sit behind those.

This epic sequences the toolkit work for the browser, virtualization, OS-service, diff, selection, and drop gaps. It does not restate a product comparison, and it does not fold in the editor. Reduced-fidelity notes and code stay with the document-editing epic. Scene types stay with the creative-workspace work.

### Already in 0.4.0

| Facility | Where it lives | Boundary this epic keeps |
|---|---|---|
| Uniform virtual lists | `ListView::mount_virtual`, core `list_window`, `Scroll::offset` as `f32` | Variable height extends this path. It does not add a second list widget. |
| Stable item identity | `ItemId` in `crates/rndr_ui/src/collections.rs` | Application state stays on `ItemId`. A recycled entity is not state. |
| Retained collections | `MAX_ITEMS` 1024; tables 256×32; inspector 128; select popups 256 | Those budgets stay unless a unit below names a change. |
| Selection | `SelectionMode::{None, Single}` on lists and tables | Range and multi-select are R5.2, outside the current authorization. |
| File drop | `FileDrop` / `FileHover` carry a path and no position. macOS, Windows, and X11 emit them. Wayland does not. | Position is authorized R5.1, its own pull request from `main`. Wayland stays unsupported. |
| Theme | `Appearance` and `Contrast` are explicit. `UiTheme::core_theme` is a pure conversion. `tokens.rs` states that OS following is unavailable. | The application remains responsible for theme. |
| OS integration that already shipped | `hotkey` and `tray` default **on**. `dialog`, `term`, `ui`, and `observe` default **off**. | New platform dependencies default off. Existing defaults stay. |
| Windows and panels | `WindowConfig`, `WindowKind`, `platform_support` | Historical `Supported` cells are the runner's current table. This epic's ledger does not copy them forward as a new signoff. |

## Goal and measurable outcome

Applications can assemble a desktop shell from rndr: a variable-height list, the OS services a shell window needs, an optional embedded browser, a host-owned diff presentation, and range selection with a positioned drop where the OS supplies a position.

A complete epic is owner-accepted R0 contracts, then independently reviewed units, each with negative cases and a platform cell that says `Supported` only after a live session. Headless success never closes a native platform cell. Implementation closure and native certification stay separate, as they do for document editing.

Published toolkit crates stay free of Medousa-specific types. An application adapter, if it exists, lives in the application repository.

### Non-goals

- Implementing R1–R6 in this change. Acceptance is recorded here. R1 and R5.1 are separate pull requests from `main`.
- Application binaries, including any Medousa binary, in this repository.
- Extracting or rehosting another process's command surface inside rndr.
- An application web frontend, a mobile frontend, push, or a phone-only assistant.
- An updater, an SVG icon system, PDF preview, or voice and media playback.
- Diagram rendering, template runtimes, and transclusion.
- Create-scene implementation.
- Grammar, diagnostics, or live-IME source editing. Those remain the document-editing epic.
- Changing `hotkey` or `tray` from default-on, or inferring `UiTheme` from the operating system.
- Publishing `rndr_browser` as part of R0.

## Dependency graph and dispatch

```text
R0 owner-accepted decision record
 ├─ R1 variable-height ListView
 │    └─ R4 diff presentation
 ├─ R2 OS services, six reviewable slices, each depending only on R0
 ├─ R3 embedded browser, ordered: macOS, WebView2, WebKitGTK, find, script
 ├─ R5.1 positioned file drop (R0 only; no ListView edits)
 ├─ R5.2 range and multi-select (R0 only; ListView edits are serial with R1)
 └─ R6 certification ledger, filled only for sessions that were actually run

The document-editing epic is a sibling. It does not gate R0–R5.
Application integration is outside this repository.
```

R1, R2, R3, and R5 are dependency-parallel once a unit is authorized. They are not one pull request.

The owner's current authorization is two parallel agents and two pull requests. Each branches from `main`. Neither is stacked on this record, and this record does not open those pull requests.

| Agent | Authorized now | Base | Leave alone |
|---|---|---|---|
| A | R1, variable-height `ListView` | `main` | Browser crate. File-drop files while B has R5.1 open. R5.2. |
| B | R5.1, positioned file drop | `main` | `collections.rs` and `virtual_list.rs` while A has R1 open. Browser crate. |

Browser work (R3.1–R3.5) is queued behind that pair. It is not authorized to start with them. R2, R4, R5.2, and R6 remain sequenced and are outside this authorization. R4 still waits until R1 has merged. R5.2 stays serial with R1 on `ListView`. When R2 is later authorized, one R2 pull request is in review at a time, because each slice adds a feature to `crates/rndr/Cargo.toml` and `crates/rndr_render/Cargo.toml` and a branch in the runner. R3.2 and R3.3 start only after R3.1 has merged the shared browser contract. Find starts after the backends it claims. Script is last.

## R0 — contracts

R0's output is this record. The repository owner accepted the decisions, caps, negative cases, and proposed platform policies below on 2026-11-06 at 15:03 America/Phoenix. No cap was amended. A later amendment belongs in this file before the affected unit starts, not as a different cap discovered inside a feature pull request.

### R0.1 Optional embedded-browser crate

- The browser is a new workspace crate, `rndr_browser`, shaped like `rndr_term`: optional on the facade, absent from `rndr_core`, `rndr_ui`, and `rndr_paint`.
- The facade feature `browser` defaults off, as `term` and `dialog` do. That feature is the only way a desktop build links a platform webview.
- macOS WebKit, WebView2, and WebKitGTK are optional dependencies reached only through that feature. They are not added to default features.
- The crate's public types are generic host intents and opaque tokens. They name no application, no product command, and no document schema.
- R3 may land the crate with `publish = false`. Publication is a later release decision. Examples stay unpublished.
- R0 adds none of the crate, the feature, or a manifest line.

### R0.2 Guest pages have no application command bus

A guest page receives navigation and, only in the final browser slice, replies to host-issued script requests. It does not receive an object, handler, or channel that invokes application commands, lists those commands, or passes arguments into the host.

Navigation events report URL, title, load outcome, and back/forward availability. The host decides what a URL means. The toolkit does not dispatch on it. A custom scheme is a navigation report. It is not a command invocation.

Downloads and file-input requests are host intents. The toolkit does not choose a download directory, write a file, or open a file dialog. The existing `dialog` feature stays independent: enabling `browser` does not enable `dialog`.

Negative cases:

- No guest-to-host message bridge and no page-initiated remote call.
- A navigation event is not an application command and carries no command name.
- A download with no host acceptance writes nothing.
- A file-input request with no host response cancels and selects no path.
- Destroying the view drops unanswered intents.

### R0.3 Script requests

Script is a host-issued, request-scoped evaluation. The toolkit mints the nonce. The host cannot choose it. There is no primitive named `READ_GUEST_THAT_MUTATES_BACK`, and no operation whose meaning is to read guest state and apply that read as a mutation of the guest or of application state. The host inspects a bounded result and, in its own code, decides on any later write.

One admitted request binds a view, a view generation, a single-use 128-bit nonce, the exact top-level origin the host named, and UTF-8 source. A reply echoes the nonce.

Caps, proposed for acceptance:

| Bound | Value |
|---|---|
| Source | 64 KiB of UTF-8 |
| Result | 64 KiB |
| In flight | 4 requests per view |
| Queued undrained results | 4 per view |
| Timeout | host-chosen, toolkit ceiling 30 seconds |
| Document | top-level only |

Admission refusal order, before any guest work and without consuming an in-flight slot:

1. Missing view or wrong generation.
2. Source over 64 KiB, or not UTF-8.
3. Empty origin, or an origin that is not an exact top-level match. Subdomain, `blob`, and `data` origins do not match a different origin.
4. Four requests already in flight: overflow. No eviction and no silent queue.

Reply refusal order:

1. View destroyed: drop the reply. The nonce is already dead.
2. Unknown or reused nonce.
3. Generation or origin no longer matches, including a navigation that changed the document.
4. Past the deadline: expire the nonce so a late success cannot land.
5. Result over 64 KiB: discard the bytes and consume the nonce. Do not truncate into a success.
6. Otherwise deliver the bytes to the host and consume the nonce.

The toolkit does not apply the result to application state and does not write it back into the page. Subframe evaluation is unsupported. A result left undrained at the queue cap refuses further completions with backpressure.

These caps are decisions, not measurements.

### R0.4 Variable-height lists extend ListView

Variable height is a new mount on the existing `ListView`, proposed name `mount_variable`, returning the existing `VirtualViewHandle`. Uniform `mount_virtual` keeps its stride contract. There is no second collection type and no parallel scroll engine.

Application state, including drafts, checked values, selection, and expansion, is keyed by `ItemId`. `row(id)` stays `None` while the row is unrealized. Recycling destroys the row content entity and its descendants before the row root is bound to another id. Components on the recycled entity are not a store.

Scroll position is an anchor: an `ItemId` plus a non-negative remainder, in milli-points, within that item's height. One logical point is 1000 milli-points, held in an `i64` prefix sum. Height changes above the anchor do not move the anchored item. Shrinking the anchored item below the remainder clamps the remainder to the new height and keeps the id. Removing the id repairs to the next surviving item, then the previous, then the empty viewport.

Heights are finite and strictly positive. A non-finite, zero, or negative height refuses before mutation. An estimate may stand in until a measure arrives. The measure updates the prefix and preserves the anchor. It does not reset scroll to zero.

`Scroll::offset` remains `f32` for paint and input. Layout converts the integer anchor into that offset. R1 records the conversion error on the 10,000-row fixture. The integer anchor is the identity of the scroll position. The `f32` offset is the paint projection.

Nested controls keep the current virtual-list rule: they own their own input, and recycling retires the subtree. Persistent offscreen editors are out of scope. Retained lists stay at `MAX_ITEMS` 1024. Tables stay 256×32. Trees and tables are not virtualized here. Nested wheel remainder and scroll-drag slop stay as they are.

The acceptance fixture is 10,000 rows with at least three distinct supplied heights, including one short row and one tall row. The fixture supplies heights. It does not shape 10,000 strings.

Negative cases the R1 tests have to hit:

- Rebinding a row entity from item 1 to item 2 leaves item 1's application state in place and does not show item 1's components on item 2.
- Inserting or resizing above the viewport keeps the anchored id and remainder.
- Reorder follows the id, not the old index.
- Deleting the anchored id uses the repair order above.
- Recycling a focused nested control parks focus on the viewport, keeps the logical key, and does not refocus the old entity.
- After a jump away from a named id and back, the id and remainder match.

### R0.5 OS services are explicit `rndr_render` features

Each new integration is its own `rndr_render` feature, re-exported by the `rndr` facade the way `dialog` is. All of them default off. With the feature off, the entry point returns a typed unsupported error. It does not succeed as a no-op.

| Slice | Feature | What the host gets |
|---|---|---|
| R2.1 Notifications | `notify` | Present, dismiss, and a click token the host created |
| R2.2 URL scheme and single instance | `launch` | Scheme registration and URL delivery to one running instance |
| R2.3 Menu, badge, login item | `shell-integration` | Opaque menu intents, a badge count, login-item register/query/unregister |
| R2.4 Window frame | `window-frame` | A snapshot the application stores, and a restore result |
| R2.5 Appearance | `appearance` | A prefers-dark signal |
| R2.6 Sleep and wake | `power` | `Sleep` and `Wake` events |

`hotkey` and `tray` stay default-on. This epic does not move them. The updater is not a feature.

The application remains responsible for theme. `appearance` reports that the OS preference changed. It does not write `Appearance`, `Contrast`, `UiTheme`, or core `Theme`. `UiTheme::core_theme` stays a pure conversion the application chooses to call. `Appearance` stays an explicit setting, matching `crates/rndr_ui/src/tokens.rs`.

Platform vocabulary is the existing `Availability` enum. `Supported` means a live session of that platform has shown the behavior. `Unverified` means the runner may send the request and no live session in this epic has shown that the OS honors it. `Ignored` means the command is accepted and the OS is not asked, or drops it, while logical state still updates. **Unsupported** is the additional word for a capability this epic refuses to claim because the platform cannot do it. Wayland positioned drop is Unsupported. A green headless run does not move a cell to `Supported`.

macOS is the first platform a slice may mark `Supported`, and only after a smoke on that OS is written into the R6 ledger. Windows and Linux cells start `Unverified` or Unsupported. `docs/desktop-readiness.md` does not fill them. A permission prompt the session cannot complete leaves the cell open. Compilation is not support.

The desktop packager in [desktop-apps.md](../desktop-apps.md) does not register a login item, sign, or call the network. R2.3 must not change that script so that packaging registers a login item as a side effect.

### R0.6 Selection and drop contracts used by R5

These are accepted R0 decisions. R5.1 is authorized to implement the position contract, as its own pull request from `main`. R5.2 is outside the current authorization.

`SelectionMode` gains `Multiple` and `Range`. `None` and `Single` keep their current behavior. The enum is public on the optional `ui` path. 0.4.0 is a 0.y crate, so the new variants are an intentional compatibility note: an exhaustive match outside the workspace will need a new arm. Existing hosts that set `Single` or `None` are unchanged.

`Multiple` toggles membership of an enabled `ItemId` on the selection gesture (primary release, Space release, AT activate). Arrow and focus movement do not change the set. `Range` without Shift selects that one enabled id and moves the anchor. `Range` with Shift selects the enabled ids from the anchor through the focused id in current logical order, and skips disabled ids. The anchor does not move on a Shift extension.

The selected set lives on the handle, keyed by `ItemId`. `Selected(Option<ItemId>)` continues to mean `Single` and `None`. `Multiple` and `Range` emit a set-changed notification. The host reads the set from the handle. The event does not carry a `Vec`. Removing an id drops it from the set and emits the notification. Recycling does not drop ids. DataTable uses the same modes inside its current 256×32 budget. This unit does not virtualize the table.

`FileHover` and `FileDrop` gain `position: Option<[f32; 2]>` in logical points. `None` means the platform did not provide a position. `None` is never rewritten to the origin. Wayland stays Unsupported: the event is not emitted there today, and this epic does not invent a position. Hit-testing a position into a row is the host's use of the point. R5.1 does not do that hit test inside `ListView`.

Any new platform crate required to read a position defaults off. With that feature off, drops keep working as path-only and position stays `None`. Existing path-only behavior on macOS, Windows, and X11 must keep compiling on the default features.

### R0.7 Acceptance gates

The repository owner accepted R0 on 2026-11-06 at 15:03 America/Phoenix, in review of PR #80, with this wording:

> Accept R0 and its proposed platform policies in PR #80, and start the two-agent plan with R1 alongside R5.1. Keep browser work queued behind them.

- [x] The owner accepts this record. No amendment was requested. The quote above is the acceptance.
- [x] R0.1–R0.6 stand as accepted text, including the script caps, the 10,000-row fixture, milli-point anchors, feature names, and the absence of an application command bus and of `READ_GUEST_THAT_MUTATES_BACK`.
- [x] The proposed host answers below are confirmed. An implementation pull request uses those answers.
- [x] No implementation unit was opened against an unaccepted R0. R1 and R5.1 are authorized after this acceptance, as separate pull requests from `main`. This pull request does not contain them.

Acceptance of the contracts does not merge the implementation pull requests, certify a platform, publish `rndr_browser`, or admit Medousa types into a toolkit crate. The current authorization is R1 and R5.1 only. R3 stays queued. Native certification cells stay Unverified.

### Confirmed platform policies

The owner accepted every proposed answer below. They are the platform policies for later units. Confirming them does not move a native cell to `Supported`.

| Question | Accepted answer |
|---|---|
| Which OS releases are in a support claim? | R0 names none. Each smoke records the OS it ran. A floor is an owner sentence in the ledger. No cell says `Supported` for "macOS" or "Windows" as a family. |
| WebView2 runtime distribution | The toolkit does not download or bundle a runtime. A missing runtime is a reported error on that machine. |
| Linux single-instance transport | A private local socket owned by the running instance. The payload is one URL. A desktop portal would be a written amendment, because `launch` does not take on a new portal dependency by default. |
| Windows high-contrast | `appearance` carries prefers-dark only. `Contrast` stays an explicit `UiTheme` choice. R2.5 adds no second signal. |
| Maximized and fullscreen frames | The snapshot stores normal logical bounds, scale, an optional display id, and a maximized bit. Fullscreen is not restored. |
| Notification click | The click delivers the host token. The application raises a window through existing window APIs. The `notify` feature does not activate a window on its own. |
| Bundle entitlements | Notifications, login items, and scheme registration may need a signed, entitled application bundle. The application owns that bundle. This repository does not add one. A smoke that cannot obtain permission stays `Unverified`. |

## R1 — variable-height virtualization

**Depends:** accepted R0. **Authorized now.** One pull request, branched from `main`, parallel with R5.1, and not stacked on this record. Owner: `ListView`'s virtual path and the core window helper it already shares. Do not change selection cardinality, DataTable, or TreeView budgets in this pull request. Native cells for this unit stay Unverified until an R6 session.

Done when 10,000 mixed-height rows keep `ItemId` and the milli-point remainder across recycling, nested-control state survives only under `ItemId`, and the existing uniform `mount_virtual` tests still pass. Record the `f32` conversion error on that fixture.

Tests: the negative cases in R0.4, plus uniform-list regressions. Headless anchor math is the acceptance test. Trackpad, scrollbar drag, and screen readers stay R6 cells.

## R2 — OS service slices

**Depends:** accepted R0. **Outside the current authorization.** Six pull requests, in any order, one in review at a time, when a later request authorizes them. Each slice has a headless admission test and a native cell that starts `Unverified`. The accepted platform policies above already bind these slices.

### R2.1 Notifications (`notify`)

Present and dismiss return a host-owned token. A click reports that token. Permission denial is a result. The toolkit holds no routing table and does not inspect the token. Negative: a click for a dismissed or unknown token is refused; a denied permission does not present.

### R2.2 URL scheme and single-instance forwarding (`launch`)

The host names the scheme. The toolkit does not reserve one and does not interpret the URL. A second process forwards one URL, at most 64 KiB, to the running instance and exits. The running instance queues at most 16 URLs. Overflow is reported to the forwarder. Failure to bind the single-instance channel is an error, so the application does not silently become a second writer. Negative: the forwarded bytes are a URL, with no command name and no batch of calls. Startup with no instance running delivers the URL to the new process as a startup event.

### R2.3 Menu, badge, and login item (`shell-integration`)

One pull request, three platform cells, because they share one shell-integration owner. The macOS application menu emits opaque host intents. It is not an in-window command bus and not a second input dispatcher. Badge is set-count or clear. Login item is register, query, and unregister for the current user. It does not start the process. Platforms with no equivalent cell are Unsupported, not emulated with a custom widget. Negative: packaging still does not register a login item; a menu intent is not a guest-page call.

### R2.4 Window-frame restoration (`window-frame`)

The runner reports a snapshot: logical position, logical size, scale, display id when the OS has one, and the maximized bit from R0. The application stores it. Restore returns Applied or FellBack with a reason. A missing display falls back to platform placement. A non-finite snapshot refuses. Wayland toplevel placement stays what `platform_support` already says; the restore result says the position was not applied. Negative: a scale change between save and restore does not produce a zero-sized window; Ignored position is not reported as Applied.

### R2.5 Appearance (`appearance`)

The runner emits prefers-dark. The application may then write `UiTheme`. A signal during an open popup does not rewrite tokens on its own. Negative: installing the feature leaves an existing `UiTheme` byte-for-byte unchanged until the application writes a new one.

### R2.6 Sleep and wake (`power`)

The runner emits `Sleep` and `Wake`. The application decides whether to pause work or redraw. Negative: wake is not a document reload and not a permission grant. A headless injected event does not mark a lid-close `Supported`.

## R3 — optional browser

**Depends:** accepted R0. **Queued.** The owner kept browser work behind R1 and R5.1. R3 is not authorized to start with that pair. Five pull requests, in order, once a later request takes the browser off the queue. The crate stays feature-off. Each pull request updates a supported/unsupported matrix in the crate docs. A backend not yet landed is Unsupported. Find and script stay Unsupported until their own pull requests. Native browser cells stay Unverified.

Shared contract from R3.1, implemented again on each backend: show, hide, bounds in logical points relative to the host client area, navigate, back, forward, reload, stop, and observed title and URL. A non-finite or negative rect refuses. Zero area hides the surface and keeps the view. On resize, the host submits the new rect and the guest surface matches it on the next applied frame. Downloads emit an intent (URL, suggested name, mime). The toolkit writes bytes only after the host accepts a destination path. File input emits an intent. The host supplies paths or cancels. Destroy cancels both.

### R3.1 macOS

WKWebView, the matrix skeleton, navigation, bounds, and resized bounds. One macOS smoke. Find and script Unsupported. WebView2 and WebKitGTK Unsupported.

### R3.2 WebView2

The same contract on Windows, after R3.1 has merged. One Windows smoke. A missing runtime is a reported error. The toolkit does not download or bundle one. macOS results do not transfer.

R3.2 implements that contract in `rndr_browser` for an already installed WebView2 runtime. `BrowserError::RuntimeMissing` is the missing-runtime error. Native cells stay Unverified: a smoke log is not an R6 ledger entry, and merging R3.1 did not certify the macOS download callback, bounds smoke, or file input.

### R3.3 WebKitGTK

The same contract. X11 and Wayland are separate cells. A session that was not run stays `Unverified`. Child-surface coordinates under Wayland stay `Unverified` until a compositor session shows them. A compositor that cannot host the guest surface is Unsupported for that session, and the matrix names it.

### R3.4 Find

A host request: set query, next, previous, clear. Match count when the backend can report it. A backend that cannot find stays Unsupported. Find is not implemented by evaluating script.

### R3.5 Request-scoped script

Implements R0.3 on each backend whose navigation contract has landed. Headless tests drive the nonce, cap, origin, overflow, and timeout state machine against a fake guest. The fake guest does not close the native cell. One smoke per OS that was actually run: one admitted evaluation, and one refused replay of the same nonce. Script stays last so guest evaluation is not reviewed inside the navigation or find pull requests.

## R4 — diff presentation

**Depends:** accepted R0 and R1 merged. **Outside the current authorization.** One pull request after R1 has merged.

The host owns the hunks. It supplies stable hunk ids, row text, and a generic gutter marker: unchanged, added, removed, or header. The toolkit draws the gutter and moves between hunks with the R1 anchor: next, previous, and jump to id. There is no Git parser, no diff algorithm, no repository type, and no forge type. Staging, comments, and file identity stay with the host.

Negative: an empty diff, a single very long line, and removal of the focused hunk all have defined repair. The view does not syntax-highlight and does not call the editor epic's gutter. Rows use `mount_variable` so mixed hunk heights scroll under the R1 contract.

## R5 — range, multi-select, and positioned drop

**Depends:** accepted R0 only. R5 does not wait for R1. The selection pull request must not edit `ListView` at the same time as R1.

### R5.1 Positioned file drop

**Authorized now**, alongside R1. One pull request, branched from `main`, not stacked on this record, and not an edit to `ListView`. `position: Option<[f32; 2]>` as in R0.6. Wayland is Unsupported in the matrix and in the crate docs. Negative: a path-only drop stays `None` and does not become `(0, 0)`; hover-cancel still clears every path from the current hover. The native cells for macOS, Windows, and X11 stay Unverified until an R6 session.

### R5.2 Range and multi-select

**Outside the current authorization.** One pull request, after the ListView file is free. Lists and tables. The set is keyed by `ItemId` and read from the handle. Negative: disabled rows are not added; a removed id leaves the set; recycling preserves the set; `Single` hosts still see `Selected(Option<ItemId>)` and no membership toggle.

## R6 — native certification ledger

**Depends:** the unit being certified has merged. **Outside the current authorization.** R6 is a sequence of evidence updates, one surface per pull request, written by the person who ran the session. It is not an implementation agent and not a blanket signoff.

A cell becomes `Supported` only when the ledger names the OS and version, machine, Rust version, features, date, reviewer, actions, expected result, and observed result. Headless success never closes a native platform cell. Owner acceptance of R0 on 2026-11-06 does not close one either. `docs/desktop-readiness.md`, the historical sentences on `platform_support`, and the document-editing epic's IME ledger do not fill these cells. The runner's existing `Availability` table is left as it is; this table is this epic's signoff. Every cell below remains Unverified or Unsupported, as written.

Live IME remains owned by the document-editing epic. This ledger may record a shell-surface session. It does not close that epic's cells by reference.

| Surface | Headless status in this record | Native certification |
|---|---|---|
| Variable-height identity and anchor | Not run. R0 is docs only. | Unverified on macOS, Windows, X11, Wayland |
| Notifications | Not run | Unverified. A denied permission is not `Supported` |
| URL scheme and single instance | Not run | Unverified per OS |
| Menu | Not run | macOS unverified. Platforms with no application menu: Unsupported |
| Badge | Not run | Unverified where the OS has a badge. Otherwise Unsupported |
| Login item | Not run | Unverified. Packaging must keep refusing to register one |
| Window-frame restore | Not run | Unverified. Wayland toplevel position follows `platform_support` and is not a silent Applied |
| Appearance signal | Not run | Unverified. Theme writes remain the application's |
| Sleep and wake | Not run | Unverified. An injected event is not a lid-close |
| Browser navigation, bounds, resized bounds | Not run | macOS, WebView2, WebKitGTK X11, WebKitGTK Wayland each unverified until their smoke |
| Browser find | Not run | Unverified per backend that implements it. Others Unsupported |
| Browser script | Not run | Unverified per OS. Fake-guest tests do not close the cell |
| Browser downloads and file input | Not run | Unverified. Host-cancelled requests write nothing |
| Diff gutter and hunk motion | Not run | Unverified |
| Range and multi-select | Not run | Unverified for pointer, keyboard, and AT |
| Positioned file drop | Not run | macOS, Windows, X11 unverified. Wayland Unsupported |
| Live IME on shell surfaces | Not this epic's implementation | Unverified. Japanese, Chinese, Korean, and dead keys are separate cells |
| VoiceOver | Not run | Unverified |
| Narrator | Not run | Unverified |
| Orca | Not run | Unverified. X11 and Wayland are separate cells |
| Cross-application clipboard | Not run | Unverified, including X11 primary and Wayland clipboard |
| Wayland overlay and Windows non-activating panel | Existing runner table only | Unverified for this epic's signoff until a new session is recorded |
| Browser with a screen reader | Not run | Unverified per OS |
| Browser with a live IME | Not run | Unverified per OS and input method |

## Pull-request boundaries

R0 is accepted. Open implementation as separate pull requests from `main`. Do not combine them, and do not stack R1 or R5.1 on this record. The units authorized now are R1 and R5.1. R3 stays queued. The other rows wait for a later authorization.

| Unit | PR | Depends on | Shared-file constraint | Done when |
|---|---|---|---|---|
| R1 | 1 | R0 | Exclusive owner of the virtual list files while open | 10,000-row anchor and identity tests |
| R2.1–R2.6 | 6 | R0 | One feature-flag PR at a time | Headless refusal tests, native cell left open |
| R3.1–R3.5 | 5 | R0, then the previous browser PR | `rndr_browser` contract freezes at R3.1 | Matrix updated; one smoke per OS that the PR claims |
| R4 | 1 | R0, R1 merged | Rebases onto R1 | Host-supplied hunks, gutter, hunk motion, no parser |
| R5.1 | 1 | R0 | May run beside R1 | `Option` position; Wayland Unsupported |
| R5.2 | 1 | R0 | Serial with R1 on ListView | `Multiple` and `Range` on lists and tables |
| R6 | one evidence PR per session | That unit merged | Docs and ledger only | The cell names a real session, or it stays open |

## Compatibility

1. Uniform `mount_virtual`, `SelectionMode::None`, and `SelectionMode::Single` stay. Variable height and the new selection modes are opt-in.
2. `FileDrop` and `FileHover` gain an optional position. Existing path reads keep working. `None` is the value when the platform reports no point.
3. New features default off. `hotkey` and `tray` stay default-on. Core, UI, and paint gain no webview, notification, or login-item dependency.
4. `rndr_browser` is generic. No published crate gains a Medousa type, a product command enum, or a guest command bus.
5. Theme types stay explicit. An appearance signal is additive and writes nothing until the application does.
6. The document-editing public API is not renamed or extended by this epic.

## Risks and decisions for the owner

| Decision / risk | Choice in this record |
|---|---|
| One large shell pull request | Rejected. The table above is the review boundary. |
| Guest page grows a command bus | Rejected in R0.2. Script cannot call back into application commands. |
| Read-and-write guest helper | Rejected. `READ_GUEST_THAT_MUTATES_BACK` is not a primitive. |
| Recycled rows store drafts | Rejected. State is on `ItemId`. |
| OS theme overwrites the application | Rejected. The application owns `UiTheme`. |
| New native crates default on | Rejected. New features default off. |
| Headless tests certify macOS, Windows, or Linux | Rejected. R6 stays open until a named session. |
| Wayland drop position inferred from a global cursor | Rejected. The cell is Unsupported. |
| Editor work pulled into this plan | Rejected. Grammar, diagnostics, and live IME stay in the other epic. |
| Two agents edit `ListView` together | Rejected. R1 and R5.2 are serial. |
| Script reviewed inside the first browser PR | Rejected. Script is R3.5. |

### Authorization already given

On 2026-11-06 at 15:03 America/Phoenix the owner accepted R0 and its proposed platform policies, and started the two-agent plan: R1 alongside R5.1, each a separate pull request from `main`. Browser work stays queued behind them. This file records that authorization. It does not open those pull requests and it does not start R3.

## Docs-only verification

Checked when this file was added, against `cc25bb2722b2d01ec3d4f8f3a0c452863cfeac9c`:

- The diff is this file only. `README.md` and every crate are unchanged.
- Relative links resolve: `native-document-editing-epic.md`, `../virtual-list.md`, `../data-components.md`, `../overlay-platform.md`, `../desktop-readiness.md`, `../desktop-apps.md`.
- The one fenced block is closed. `git diff --check` reports no whitespace errors.
- No Cargo test, native smoke, benchmark, or feature-flag build was run. Those checks belong to the implementation units. This record does not certify a platform.
- Acceptance update, same file only: the owner's 2026-11-06 15:03 America/Phoenix acceptance of R0 and the proposed platform policies is recorded above. R1 and R5.1 are the authorized units. R3 stays queued. Ledger cells are unchanged and unverified. No runtime work, merge, or release is included.
