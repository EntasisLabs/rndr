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

## Historical first R1 correction after independent A2 REVISE (superseded below)

The independent review of `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d` found that summed natural widths did not prove fit in core's equal-share Fill row. The correction checks **each** resolved label's unwrapped width plus its actual nested layout insets against `(available - toolbar padding - gaps) / visible children`, including target minima. The existing conservative total-width budget is retained as an additional veto, preserving established narrow-toolbar behavior; a budget fit is never sufficient without each actual share fitting. Exact per-share fits pass the new check; otherwise all actions stack. No measured-width allocation, hidden-action policy, style scheduling, activation or core change is introduced. Unknown visible children still force stacking; hidden children do not consume shares. Empty/single behavior is retained.

Permanent `a2_toolbar_allocation.rs` promotes the independent unequal-label probe and covers mixed icon/source commands, same-update width/font-scale/label changes in both directions, comfortable/compact and 100%/200% type, boundary fits, finite narrow geometry, focus/identity/names/actions, hidden/unknown children and toolbar padding. New R1 evidence is separate under `evidence/a2-r1-20261101/`; old sealed receipts and all six passive goldens remain historical and unchanged. A fresh independent exact-candidate re-review is required before owner A2 acceptance. No native certification or integration is implied.

## R1 continuation: actual content and snapped layout fit

Fresh independent review of exact `92e98004d886a9ccfc3d2d05eaf090b591e10cc9` remains a **REVISE** deliverable only. The first correction above omitted core's inner text inset and actual snapping: at 1696 points the 826-point label box has only 802 points of text content and wraps to two lines. Its arithmetic boundary claims are historical, not the current implementation contract.

The current continuation calls the existing authoritative `layout_propagate` and `apply_text_layout` on tentative rows, after the current style cascade. A row survives only if every known visible control's actual snapped label rectangle yields `field_text_width >= unwrapped natural width` and its actual `TextLayout` has at most one line. This includes all nested layout padding and the installed engine's unit advance without copying core inset/snapping constants, guessed tolerances or a special-case label. The conservative historical sum remains a veto, never a proof of fit. Unknown/malformed content still stacks; hidden content is excluded; empty behavior is unchanged. Reflow reconsiders tentative rows each update, so width/type/label updates work both ways in the same update.

Vetoes are monotone within the update; if an ancestor geometry changes after a veto, remaining rows are measured again. This uses core's existing layout/text code, not a second layout engine or public API. With N tentative nonempty rows there are at most N+1 measurement walks (normally one or two), followed by App's normal final layout. Scroll snapshots are restored before each measurement and after resolution so a rejected short row cannot commit a premature scroll clamp. Layout/text caches are refreshed by the final authoritative walk. No input, focus, intent draining, editor buffers/history or host dispatch is introduced. No core/API/dependency source changes are necessary.

The previous five allocation tests now assert actual final content and line/glyph geometry. Their incorrect boundary stimulus now includes the public core content metric; below-boundary samples lose a full allocated pixel rather than assuming a quarter-point must survive rounding. Original source/logs remain preserved. Four new permanent tests cover the integer reproduction/exact boundaries, fractional logical scales and origins, both densities, 100–200% type, same-update width/type/label updates in both directions, focus/actions and preservation of scroll offsets through rejected measurements. The identical final four-test fixture passes 0/4 (four failures) on the starting UI and passes 4/4 on the final direct-source UI build. Its hidden-ancestor/malformed/scroll guardrail also reproduces a missing-rectangle panic in the intermediate correction, preserved before the final conservative missing-geometry fallback; revealing the ancestor reconsiders layout in the same update. All original 32 tests remain unchanged.

Evidence and full identity/command qualifications: `evidence/a2-r1-content-fit-20261101/REPORT.md` and `COMMANDS.md`. A2 remains **unaccepted**; owner arranges fresh independent review and retains acceptance/integration. Native windows/screenshots/OS AT/preferences/fonts/shaping/IME/physical DPI/platform remain **UNVERIFIED**.

## Current reconciliation — sender accepted scoped R1 review, not all A2 (2026-11-01)

The sender accepted the complete independent scoped HEADLESS R1 review of exact
`f427b7474dad99a983115edfad14c5084c9cb253` (implementation
`91c008bdc1054f42f67dcd5d45729290ddb4ffa4`). Prior P2 actual-content/snapping finding is resolved.
This supersedes historical “fresh R1 review required” wording only: it is NOT owner full-A2
acceptance, native certification, integration or project completion. Earlier A1-blocked/A2-not-started
paragraphs remain historical; accepted04642 already lifted the A1→A2 gate.

Complete independent report/commands/scripts/logs/negative controls/source identities and original
267-entry standard manifest are now durably preserved, without changing their provenance, under
[`evidence/a2-gate-reconciliation-20261101`](../../evidence/a2-gate-reconciliation-20261101/REPORT.md).
238 payload files are retained byte-for-byte;29 large compiled artifacts stay at their external
locations with bounded hashes/identities. The original /tmp directory and prior evidence are untouched.

The [current exact remaining-gate matrix](A2-remaining-gates.md) reconciles all A2 components,
Specification §8 numbered evidence gates, platform/manual cells, saved goals and previous reviews,
with executable next steps. No additional implementation defect is established in the latest scoped
review; this is not an exhaustive full-A2 review. Preserve the extra whole-world layout/text cost caveat
(100 unrelated-label calls→200/300; native responsiveness unverified) and the two pre-existing strict
core-test lint warnings at a1_wrapped_hug.rs79/125. No lint/performance/source fixes are made here.

Next concrete native gate: owner-authorized macOS synthetic operating-gallery keyboard/pointer/resize
and responsiveness, followed by real VoiceOver/IME/fonts/DPI checks; separate Windows/X11/Wayland
capabilities remain unavailable to this no-launch task. All native cells remain UNVERIFIED.
Owner/sender retains full A2 disposition and every project lifecycle decision. This reconciliation is
evidence/doc-only: no new build artifacts, launches, grants, source changes, approval, integration or push.
