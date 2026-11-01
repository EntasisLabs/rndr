# A2 contracted operating components — review candidate, not native acceptance

This continues the SAME owner-authorized A0→A1→A2 undertaking from accepted **04642fe03f7603f228eaf9b83611641a95676e8d**. The new acceptance section in `implementation-tracking.md` records the owner decision; earlier checkpoints remain historical. The Specification and Review Guide are unchanged. A2 requires a fresh independent review and owner assessment; this document does not approve, integrate, publish or certify the project.

## Architecture and scope

`rndr_ui::controls` owns component props, stable ECS identities, styles and slots. Core's existing GenericInteraction, hierarchy/layout, focus, text engine, paint, editor and AccessKit adapter retain their jobs. UI resolution does **not** process input, drain intents or dispatch commands. There is no parallel event loop, focus manager, renderer, parser, application lifecycle, menu, dialog, native material or Blooey adoption.

Normal `rndr_ui` dependencies remain core-only/GPU-free. The desktop umbrella gets a **dev dependency solely for the explicit gallery example**, not a default plugin or application adoption. Cargo.lock adds that already-present workspace package edge only; no external version changes.

### Necessary reusable core additions (separate commits)

- `ManualTabs(TabOrientation)` opts a real Tabs group into manual orientation keys and one roving Tab stop. Legacy groups are unchanged. Horizontal Left/Right or vertical Up/Down, plus Home/End, move focus among enabled, focusable related tabs in direct-child hierarchy order; arrows wrap. Unrelated direction keys are not consumed. Enter or released Space activate through the existing canonical tab path. Modified/synthetic events retain core's ownership guards.
- **Roving entry:** when focus is outside a group, enter the active enabled tab, otherwise the first available tab. While focus is inside, the currently focused tab is the sole Tab stop; Tab/Shift-Tab leave rather than visiting every tab. Focus movement does not select a panel. A disabled active tab does not silently change selection: entry falls back while the host still owns selected state.
- `SelectedState(bool)` supplies explicit host-owned selected metadata independently of focus; `ControlDisabled` supplies non-Button disabled state. `AccessDetails.orientation` reaches the **actual AccessKit adapter**, not browser ARIA or an invented native role.
- `interaction_pressed` is a read-only view of canonical armed state. It never creates another activation path.
- Modern Tab release now recognizes a descendant hit through its `ReleaseActivation` ancestor, rather than requiring a Button ancestor. This is necessary for actual labelled tabs, not decorative rectangles. A standalone unchanged fixture against exact A1 reproduces the failure.
- `ResolvedColor` lets an explicitly styled primitive keep its fully resolved semantic color instead of the historic paint focus brightening. Legacy primitives still brighten; a real display-list test checks both branches.
- Existing focus reveal now walks nested scroll ancestors inside-out, using each local viewport and carrying the target translation outward. Before/after reproduction shows the old nearest-only path left the outer viewport unrevealed. This is a repair in the existing focus manager, not a new one. Editor logic/ownership is unchanged.

A1's FIFO64 implementation, intentional same-target multiplicity, explicit overflow, source ownership and modern-versus-legacy Fired separation are preserved. The P3 logical-only test correction and independently promoted `code=None` cases are separate test-only commits, not a gratuitous A1 rewrite. The original prior14 fixture is unchanged.

## Shared lifecycle, style and activation contract

Mount requires a live Layout parent and caller-stable `UiId`, unique across passive and operating component families under that parent. Repeated mount of the same family updates in place; wrong-family reuse is `IdentityConflict`. Nonempty required labels/names, valid widths and supported selector count are checked before updates. Malformed owned trees or stale generations return `Stale` instead of guessing/rebuilding. Selector count changes require explicit destroy/remount; they never discard content implicitly.

`ControlHandle` exposes the canonical control root and its owned wrapped Label. `ModeHandle` exposes its tablist, canonical Tab roots and stable panel content slots. Do not remove/reparent owned primitives or turn a control's Label into an interactive slot. Component validation checks owned generations, required components and parent chains. External destructive edits are unsupported and rejected; they are not repaired by adopting unrelated ambient nodes.

Destroy removes only owned primitives. Borrowed content/controls are detached alive; their editor buffers, logical selection/history and scroll state remain caller-owned. Destruction forgets focus, armed gestures and queued requests for removed targets through the existing core cleanup path. Live but malformed trees are not partially destroyed. A toolbar's direct controls are borrowed: destroy detaches them intact. Callers should destroy borrowed components explicitly before their parent when they want them removed too.

`UiRecipe` remains the default; `UiStyle` property overrides and existing explicit core-value takeover retain A0 ownership rules. Theme/density/type/label/width changes resolve in place without replacing control roots, resetting selection, rebuilding panels or creating a second editor. Authored color overrides can defeat contrast/focus: tests certify the defaults, not arbitrary caller palettes. Removing an override restores the recipe; `detach_style` retains its documented explicit handoff behavior.

All controls use core's release-cancellable modern activation: primary pointer down arms; release inside the target/owned descendant commits once. Release outside, focus change, Escape, capture/blur cancellation or invalid/disabled/readonly/stale targets cannot commit the armed gesture. Enter activates once on initial press; Space arms and activates on matching valid release. Auto-repeat is not another activation. AT Activate uses the same `try_activate` matrix. Readonly permits focus but not activation; disabled permits neither. Updating an armed control's intent/availability cancels the old gesture.

Buttons emit only reviewed core `ActionIntent`s, never modern→legacy Fired fanout. FIFO64 admits distinct requests in order, including repeated requests for the same target. Request 65 reports overflow without eviction/deduplication. The host drains **only** `take_action_intents`, observes overflow and validates each request. Legacy Buttons keep their legacy timing/Fired path. CapturesKeys/Editor ownership remains authoritative; UI controls do not steal typing, IME or Undo/Redo.

### Independent visual state and geometry

Selected navigation/tab state includes a visible `|` marker and selected fill, not blue alone. The accessible selected bit is independent of focus; focus has its own reserved 2-point ring, 2-point contrasting gap and 1-point control boundary. This padding is always reserved so focusing does not move text. Root/face primitives opt out of legacy brightening. Focus+selected coexist in the operating gallery and actual paint tests.

Hover, canonical pressed, selected, focused, enabled/disabled and readonly resolve independently. High contrast is an explicit library theme, not proof of OS forced-color handling; pressed controls invert the tested opaque pair in HC. Destructive pressed state inverts its feedback pair; hover gets an explicit boundary change. Readonly adds visible wording. Busy, pending, checked/mixed, validation, default-action and toggle semantics are **not** supported or implied.

Wrapped labels keep their full accessible names. Controls use theme target minima (comfortable 32 points; compact explicitly opt-in), grow vertically for text and reserve ring space. No ellipsis-only name or hidden overflow action is used. Real TextLayout/glyph/quad/bounds/clip tests include 200% type. Tiny widths below a meaningful target cannot be made universally usable; the toolbar rejects nonfinite or <32-point available widths. Physical display target size, native font metrics and DPI remain unverified.

## Exact component contracts

| Component | Purpose / props / slots | Keys and pointer | Native semantics / overflow / unsupported |
|---|---|---|---|
| `controls::Button` | Command with `ButtonProps`: visible label, optional persistent name, description, typed intent, Primary/Secondary/Quiet/Destructive emphasis, enabled, readonly. Root + owned Label; no arbitrary interactive content slot. | Tab/Shift-Tab; Enter initial press; Space release; primary pointer release-inside. Cancellation and AT use canonical core. | Button role/name/description, advertised Focus/Activate follow availability. Wrap/grow, never hide. Destructive is **not** a confirmation flow, menu or undo implementation. No toggle/busy support. |
| `IconButton` | Caller-supplied glyph text plus REQUIRED persistent nonblank accessible name; ButtonProps supplies description/intent/availability. No OS-symbol asset/licensing assumption. | Same canonical Button behavior, including keyboard and cancellation. | Button role/name independent of glyph. Target minima remain; glyph may grow/wrap. Empty name/glyph is rejected, not generically announced as “icon.” No tooltip substitute or hover-only name. |
| `NavigationRow` | ButtonProps label/intent/availability plus `NavigationMeta { selected, secondary, dirty }`. Secondary and unsaved wording are visible and included in persistent name. Host owns selection; activation requests a command only. | Same Button behavior. Focus never implicitly changes selection. | Command/Button semantics with explicit selected state, **not** a fabricated virtual List/ListItem model. Wrap/grow; marker survives focus overlap. No tree navigation, multi-select, drag/reorder or implicit saving. |
| `ModeSelector` | Named real tablist, required labels, explicit orientation and creation-only initial index. Stable root/group/tab targets + actual panel content slots. Count updates unsupported without explicit teardown. | Orientation arrows/Home/End move only focus; Enter/Space activate. Roving entry above. Pointer release-inside activates the real Tab; AT shares that path. | TabList/Tab/TabPanel roles; controls/labelled-by relations; selected distinct from focused. Inactive panels and all descendants are excluded from layout/hit/focus/AT, not just visually covered. Disabled/readonly tab updates are explicit; host retains selected panel. Horizontal labels wrap in their share; vertical orientation stacks. Not automatic activation, a route manager or an editor recreation API. |
| `Toolbar` | Responsive named **Group recipe**, not a new native toolbar widget. Stable content root; direct controls borrowed. Host supplies logical available width on resize. | Children retain normal Tab order and canonical control behavior; no toolbar-only arrow trap. | Uses the actual installed text engine/type style to measure unwrapped labels plus control insets. Row only if all direct known controls fit; otherwise all actions stack visibly in order. Unknown content forces stacking. No fake “More” button/menu, hidden actions or icon-only conversion. Parent scrolling handles vertical overflow. |
| `reading_layout` / Surface | Existing A0 reading measure recipe over a passive content slot; host recomputes available width in place. | No command on surface background. Explicit nested controls retain their own behavior. | Adjustable measure/type, narrow vertical reading. No native document/editor role is invented. No parsing, interaction inference or clipping as an overflow substitute. |
| `Card` / `Callout` | Existing passive title/content slots, Callout feedback token. Existing default/interactive opt-in/adoption/destroy rules remain. | Neither title/body becomes clickable or focusable by default. Actions must be explicit borrowed controls. | Existing passive semantics retained. Content wraps/grows; nested scroll is explicit. No whole-card source navigation, modal warning dismissal, save/recovery or document execution. |
| `SourceAction` | Compact **VISIBLE “Show source”** labelled Quiet command. Props: opaque `SourceLocator { document, revision, anchor }`, enabled. Uses secondary type, target minimum and wrapped visible label; no source bytes or executable text. | Tab to Show source, then Enter/Space; pointer release-inside; AT Activate. Keyboard route is persistent and described, not an undocumented global shortcut. | Named Button with description; carries only typed `HostIntent::RevealSource`. Host validates document/revision/anchor, permissions and current source before any reveal/edit. No byte-offset interpretation, parser, automatic scroll-to-source business logic or mutation. |

`HostIntent::Command(u64)` and `RevealSource(SourceLocator)` are opaque host tokens. Core's string action is an exact identity guard for the stored typed payload, **not** executable source or a host parser interface. `host_intent(world, request)` validates current UI identity/generation/availability and exact payload identity, returns a typed value, and executes nothing. Queued payload replacement and stale generations are rejected. A host must additionally reject unknown/stale/unauthorized locators; the gallery deliberately tests independent revision rejection. Do not infer deduplication from identical payloads.

## Operating gallery and launch instructions

Shared fixture: `crates/rndr_ui/examples/support/operating.rs`. It contains all four Button variants, named icon button, two navigation rows, actual manual Source/Preview panels, existing Buffer editor, toolbar/reading recipe, passive Card/Callout, visible source action and nested scroll. Its host test double consumes only core FIFO requests, updates a synthetic count/selection/status and validates a fixed synthetic locator. It never opens a vault or calls Blooey, save/recovery/parser/application logic. Preview is intentionally inert content. Switching modes reuses the same Buffer and logical selection/history; the test exercises real typing then Undo/Redo after theme and mode changes.

From this undertaking's checkout, using its **existing target**, with no clean or second Cargo target:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
# Native launcher: SOURCE-CHECKED ONLY; no window was exercised by this work.
cargo run --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit -- --dark --hc --compact --narrow --200 --long
# Omit flags for Light/comfortable/100%/wide/short labels.
# Resize the real window to test dynamic row→stack policy; no fake overflow menu.
```

Try every command by pointer and keyboard; deliberate repeated Add requests must stay distinct. Select a navigation row and move focus away/back; selected and focused must remain different. In ModeSelector, arrow to Preview: Source must remain active until Enter/Space. Type in Source, select text, switch both ways and check Undo/Redo. Tab to Show source and activate it; only the synthetic status changes. Check nested scrolling, focus visibility, resize, long labels, 200% type and the 320-logical-point 400%-equivalent reflow fixture separately. None of these instructions substitutes for an executed native receipt.

Headless evidence generator refuses an existing output directory:

```sh
cargo run --offline --locked -p rndr_ui --example operating_gallery -- /tmp/rndr-a2-gallery-CHOOSE-A-NEW-NAME
```

Eight new A2 full-document display-list/semantic/geometry cases are stored separately from the six original passive goldens. They include explicit Light/Dark/HC, compact/comfortable, narrow/wide, long labels, 200% type, focus+selected overlap, Source/Preview and nested clipping. `320 = 1280/4` logical width is the specified 400%-equivalent reflow fixture at ordinary type, **not** a claim about physical DPI or a captured 400%-zoomed OS window. The generator asserts functioning canonical activation and two identical paint passes; it does not emit or label screenshots.

## Verification and per-platform boundary

Final commands, exact implementation SHA/source/rlib identities, suite counts, new evidence hashes, expected-failure reproductions and preserved historical hashes are in the A2 evidence README/manifests. Cargo Fresh alone is not accepted identity evidence: the new verifier rebuilds **current source directly with rustc**, links probes/tests against those exact rlibs, and tests unchanged reproductions against independently reconstructed exact A1 source. It does not reuse historical overwriting runners or counterfactual caches.

| Platform / area | Evidence here | Still required by owner/native integration |
|---|---|---|
| macOS host | Offline build/source checks, actual AccessKit adapter mapping tests, deterministic builtin-font/headless paint and input/host tests | Native launch/resize/screenshots; VoiceOver tree/actions; capture/blur/cancellation under OS delivery; system preferences/materials; native font/shaping/IME; physical DPI and packaged launch |
| Windows | Platform-enum logical/physical ownership tests only; shared headless component contracts | Native build/window, UI Automation/AT, forced colors/preferences, keyboard/pointer capture/blur, fonts/shaping/IME/DPI |
| Linux X11 / Wayland | Platform-enum ownership tests only; shared headless component contracts | Each backend's native build/window/input capture/blur, AT bridge/reader, theme/preferences, fonts/shaping/IME/scaling |

No cross-platform/platform-accessibility readiness is asserted. Native windows, screenshots, actual capture/blur, OS AT/preferences/fonts/shaping and physical DPI remain **UNVERIFIED**. This is a reviewable bounded A2 implementation result, not whole-project satisfaction or native certification.
