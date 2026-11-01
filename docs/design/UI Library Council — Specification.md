---
tags: ["design-council", "rndr", "specification", "ui-library"]
---

# rndr UI Library — Council Specification

**Version 1.0 — reconciled council proposal, ready for principal review. Not approved for implementation.**

Date: 2026-11-01. Request/context: `ses_0b76a3c56c494a8a920608be31128c63`.  
Council: host (synthesis, product boundaries, migration), **Taco** (official design research, visual/interaction direction), **Ada** (repository feasibility, accessibility, component/test contracts).

## Executive decision

Build an **additive, opt-in** `rndr_ui` **component library**, not a Blooey-only reskin and not a replacement renderer. Its design language is **content-first, quiet chrome, unmistakable interaction**. Share semantic roles and behavioral contracts across platforms; adapt platform metrics and conventions deliberately. Apple HIG and Microsoft Fluent are references, not a claim of native-control equivalence.

Default to beautiful **opaque surfaces**. Native materials are a later optional capability, not a prerequisite for good design. First deliver foundations, repair/test necessary shared interaction semantics, then ship a small behavior-complete component gallery. Blooey is the proving ground in a separately authorized adoption pass.

The product's Liquid document format and Apple's Liquid Glass are unrelated concepts. No Liquid parser, document schema or app recovery state belongs in the component crate.

### Principal review decisions

1. Accept the neutral, content-first visual direction and additive `rndr_ui` boundary?
2. Accept **A0 → A1 → A2** as the initial scaffold scope, with A0 separately demonstrable but not falsely called a complete usable library?
3. Accept opaque-only v0, with menus/dialogs/native materials and full Blooey migration deferred?

Defaults below are recommendations, not platform mandates or already-implemented APIs. Coder execution remains held until principal review.

## 1. Research and repository evidence

Read-only baselines:

- rndr: `291ab05493a59732f891dd762bf90c64e79a48d0`, clean main, workspace 0.3.1.
- Blooey: `2ff114d1a4e249e827f0174572e15237142ae3ff`, clean main.

Council performed source inspection, official-document research and a small palette-contrast calculation. **No builds, test suites, source edits, coder launches or integrations were performed.** A saved specification is not implementation approval.

### Observed architecture


| Layer         | Actual responsibility                                                                                                   |
| ------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `rndr_core`   | ECS world, hierarchy/layout, input, widgets, text/editor, accessibility tree, existing theme and validated source edits |
| `rndr_paint`  | Headless frame/display-list generation                                                                                  |
| `rndr_render` | Window/GPU/platform adapters, Parley and optional AccessKit bridge                                                      |
| `rndr_std`    | Process-filesystem backend, **not a widget library**                                                                    |
| `rndr`        | Desktop umbrella re-exports                                                                                             |


Core has Button, TextField/Prompt/Editor, List/ListKeys, Tabs, Disclosure, Layout/Clip/Scroll, Splitter, Layer/Modal, ImageView and MarkdownView. `Theme` is a small dark chrome/file-pane palette, not a semantic token system. Layout has row/column, Px/Hug/Fill, uniform padding and minimum dimensions, not a full responsive CSS/grid system. Text offsets refer to the actual shaped string; hidden-markup projection is not a solved editing feature.

`Chrome` installs widget/action/editor systems; installing it twice currently appends bindings/systems. `Shell` is a **file-tree policy**, with filesystem/selection/focus behavior including Escape potentially quitting. It is not the generic UI focus solution. Existing Button activates on pointer press or legacy Enter. Layer/Modal currently does not constitute a fully accessible dialog with focus containment/restoration.

Blooey native manifest pins `.deps/rndr-291ab05`; root Cargo.toml alone is insufficient evidence of native dependencies. Native currently enables Parley with umbrella default features disabled. An accessible library does not automatically make a consumer's disabled platform bridge available.

A pending desktop-readiness undertaking proposes `rndr_app`. It is not an accepted dependency for this work. Do not duplicate its lifecycle/preferences scope or silently adopt it.

### Evidence leads requiring reproduction

Ada identified code paths worth testing: AT Tab activation vs advertised actions; direct accessibility SetValue writes that may bypass editor history; folded child labels and accessible-name resolution; missing explicit readonly/name/description/relations for proposed controls. Host also inspected activation/SetValue and Chrome source. These are **source-audit findings and suspected behavioral gaps**, not newly reproduced user-visible failures. Record expected/observed behavior, backend features and tests before promoting them to confirmed defects.

Current finite accessibility roles do not cover every proposed heading/dialog/checkbox/table semantic. Missing roles gate affected controls; they do not block passive visual foundations.

## 2. Architecture and compatibility

Add a GPU-free `rndr_ui` sibling over `rndr_core`. Gallery/example may depend on the desktop umbrella. Dependency direction:

`app/document adapter → rndr_ui → rndr_core → paint/render consumption`

Suggested modules (proposed, not current API): `tokens`, `style`, `surfaces`, `controls`, `navigation`, `layout`, `semantics`. Content Card/Callout compositions are parser-independent. Gallery is an example/fixture, not a runtime dependency. Optional umbrella export uses `rndr::ui`, not ambiguous wildcard re-exports of a second Button/Theme.

No second widget tree, focus manager, text engine, editor buffer, event loop or parallel command dispatcher. Generic reusable core prerequisites belong in core with separate tests/review. Platform preference/material integration belongs at the render/native boundary. Storage remains outside the UI crate.

Existing apps that do not opt in retain their appearance and behavior. Preserve legacy `Theme`, explicit `Color` and public struct-literal compatibility. Prefer additive policy/semantic components/resources to casually adding required fields or exhaustive enum variants. Unavoidable breaking changes require explicit versioning and principal review.

### Style precedence

Explicit application/component override &gt; component recipe &gt; semantic theme &gt; primitive fallback. Library defaults/recipes consume semantic tokens; explicit app and legacy colors remain supported and are not silently rewritten. Overrides do not receive automatic accessibility guarantees. Theme updates touch only owned styled properties; they do not recolor unrelated entities or overwrite authored values every frame.

### Installation and lifecycle contract

- One documented installation owner for widget systems/resources; no implicit Shell install.
- UI installation is idempotent, or duplicate install fails explicitly before appending systems/bindings. It must not call non-idempotent Chrome installation again when already owned by the host.
- Scheduling and input consumption order are documented/tested, including legacy/UI control mixtures and editor ownership.
- Create/update/destroy operate on the existing World, parent and a caller-stable identity. Return stable root/content-slot handles, with generation validation.
- Label/theme/density updates occur in place, preserving IDs, focus, capture where valid, editor selection/history and scroll. Density must not reset state or shrink text.
- Destroy cleans only owned descendants, focus/capture references and accessibility nodes; stale handles/actions are rejected. Caller-owned content slots have explicit ownership transfer rules.
- Full plugin teardown is either implemented/tested or explicitly unsupported; component teardown is mandatory. No silent residual bindings.

## 3. Design foundations

### Hierarchy

The content/reading plane dominates. Navigation has a quiet selected state. Commands use primary/secondary/quiet emphasis; danger is intent, not a screen full of red buttons. Additional containers do not automatically add another filled rectangle. Prefer spacing/type hierarchy, then separators, then a surface only where meaningful.

Semantic roles:

- Surfaces: canvas, navigation, content, raised/grouped, overlay.
- Text: primary, secondary, disabled, on-accent.
- Stroke: subtle, strong; focus ring independent of selection.
- Action: accent, hover, pressed, selected.
- Feedback: info/success/warning/danger with explicit foreground/background/stroke pairs.

Light/Dark are first-class. High contrast is an explicit theme/resolver mode, not merely brighter accent. OS appearance/preferences are inputs; if native following is not implemented, expose explicit settings and label system following unavailable. Reduced motion/transparency never makes the app unusable.

### Initial scale proposals

All dimensions are **rndr logical points**, not physical pixels or assumed-equivalent CSS pixels. Minimum sizes grow for localization, multiline content and 200% text scaling.


| Token group         | Recommended starter                                                                                                                                      |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Spacing             | 0, 2, 4, 8, 12, 16, 24, 32, 48; 4-based rhythm, 2 for fine alignment                                                                                     |
| Radius              | 4 compact, 6 control, 10 content group, 12 overlay; capsule for appropriate grouped controls only                                                        |
| UI type             | Cross-platform label 14/20; macOS adapter may use 13/18; secondary 12/16                                                                                 |
| Reading type        | 16/24, independently adjustable from chrome                                                                                                              |
| Headings            | 20/28 and 28/36 semibold; document levels stay semantic                                                                                                  |
| Code                | 13/20 monospace, real font fallback and explicit overflow policy                                                                                         |
| Comfortable targets | 32–36 minimum height, 32 starter; target width/spacing also tested                                                                                       |
| Compact targets     | 28 minimum only by explicit opt-in, without shrinking text                                                                                               |
| Future touch mode   | 44–48 minimum target dimensions as project choice, not universal desktop law                                                                             |
| Focus               | 2-point visible ring with separation, not clipped or hidden by selection                                                                                 |
| Motion              | 100–180ms small transitions, 180–240ms panels; nonessential translation/scale removed in reduced motion; animation itself deferred from minimal scaffold |
| Reading measure     | Adjustable 55–75 characters, target \~65; fluid 16–40 padding, narrow mode removes unnecessary margins                                                   |


Use semantic system UI font roles where verified through the current engine, with a documented redistributable fallback. Do not redistribute SF Pro/SF Symbols/Segoe assets or copied artwork without appropriate license. Icon assets need an explicit license and fallback; icon-only controls need a persistent accessible name.

### Candidate palette: concrete starting point, not brand lock-in

Seed values are project proposals, not copied OS token identities. Coder must add state-specific pairs and validate the actual composited/resolved styles. Arbitrary token combinations are not guaranteed accessible.


| Role                 | Light                 | Dark                  |
| -------------------- | --------------------- | --------------------- |
| Canvas               | `#F5F5F7`             | `#111216`             |
| Navigation           | `#ECEEF2`             | `#15161A`             |
| Content              | `#FFFFFF`             | `#18191D`             |
| Raised               | `#FFFFFF`             | `#212329`             |
| Overlay              | `#FFFFFF`             | `#292B31`             |
| Primary text         | `#1D1D1F`             | `#F5F5F7`             |
| Secondary text       | `#555A64`             | `#B1B5C0`             |
| Accent/focus         | `#005FCC`             | `#8AB4FF`             |
| On-accent text       | `#FFFFFF`             | `#101827`             |
| Strong stroke        | `#737986`             | `#7D8493`             |
| Selection background | `#DCEAFF`             | `#243E63`             |
| Danger text / tint   | `#B42318` / `#FFF0EE` | `#FFB4AB` / `#3A2223` |
| Warning text / tint  | `#8A4B00` / `#FFF6E5` | `#F4C67A` / `#352B1C` |
| Success text / tint  | `#146C43` / `#EAF7EF` | `#86D6AD` / `#1C3328` |


Host calculated sRGB WCAG contrast for selected pairs: secondary/content **6.92:1 light / 8.56:1 dark**; on-accent/accent **5.98:1 / 8.51:1**; strong stroke on navigation(light)/overlay(dark) **3.76:1 / 3.77:1**; listed feedback text/tint pairs all **5.85:1 or above**. This is a palette sanity check, **not a complete rendered-state audit**, high-contrast design or native accessibility validation. Hover/pressed/disabled/busy combinations remain coder deliverables; do not obtain them through blind alpha dimming.

### Materials

Opaque is the first-class v0 result, not a degraded fallback. Apple Liquid Glass is for the functional/navigation layer, not document content. Windows Mica is an opaque dynamic window base; background Acrylic suits transient surfaces, while in-app acrylic has more specific supporting/overlap uses. Do not flatten these into one blur effect.

Future backend: semantic material intent + supported capability + solid fallback. Account for OS version, focus/inactive windows, transparency/contrast preferences, hardware/battery cost and legibility. A translucent GPU quad or custom blur must never be advertised as native Liquid Glass/Mica/Acrylic. No glass content cards in v0.

## 4. Initial implementation gates

The **requested scaffold recommendation is A0+A1+A2**, staged and independently reviewable. A0 may be demonstrated/accepted separately, but A0 alone is NOT completion of usable Slice A. Any change to this boundary must be explicit, not silently decided during coding.

### A0 — foundations and passive gallery

- Add opt-in crate, tokens/resolver, type/surface/separator recipes and namespaced imports.
- Light/Dark, explicit high-contrast resolver contract, density/text-scale settings.
- Passive Card/Callout styling and sample document layout; no fake operating controls.
- Contrast/layout/ownership/compatibility fixtures and documented candidate palette.

### A1 — shared interaction and semantic prerequisites

- Generic focus policy without Shell filesystem/file-tree side effects.
- Opt-in modern activation policy **per control**, interoperating with legacy press controls in the same window.
- Explicit accessible names/descriptions and semantic selected/readonly/allowed-action contracts as required by shipped controls.
- Canonical pointer/keyboard/AT activation route; input ownership, capture/cancellation, schedule ordering and event cardinality resolved/tested.
- Reproduce relevant source-audit concerns, then fix only proven necessary reusable gaps.
- Safe editor-backed AT mutation contract or explicit rejection; no raw Buffer/Label mutation bypass.

### A2 — behavior-complete contracted subset

1. Button: primary/secondary/quiet/destructive; labelled IconButton.
2. NavigationRow: selected distinct from focus; optional dirty/secondary metadata with accessible text.
3. ModeSelector: manual-activation Source/Preview-style tablist **with real tab/panel relations**. Arrows navigate, Enter/Space activate; inactive panels excluded from focus/hit/AT. Semantic prerequisite failure blocks release, not replaced with a falsely labelled tab implementation.
4. Toolbar and reading-surface layout recipes; explicit narrow/overflow behavior.
5. Card/Callout with content slots, noninteractive by default.
6. Compact, **visible labelled source action** plus keyboard path, not hover-only.
7. Functioning gallery of this exact subset, including states, long content and keyboard paths.

Core prerequisites may be delivered as separate reviewed commits/stages, but a pretty mockup is not functional A2 completion. rndr-only document fixtures use a host test double; they do not claim Blooey integration was exercised.

### B — later components and document compositions

Labeled TextField/SearchField using existing editor semantics; checkbox/toggle only with real state/roles; tooltip, menu/flyout, dialog, disclosure/status components. Each includes complete keyboard/focus/AT behavior. Later Compare/Plan/Timeline recipes reuse primitives, preserve document order, metadata and comparison associations. Stack responsively without dropping labels. They remain read-only unless separately scoped.

Native materials, rich tables/trees, elaborate motion, mobile fidelity, command palette, full Live editing, parser changes, networking/action execution and comprehensive platform certification are deferred.

## 5. Component and interaction contracts

Every component specifies purpose/non-purpose, props/slots/ownership, sizes/density, role/name/value/state, key map, pointer model, emitted intent, layout/overflow, updates/destruction, styling precedence and tests.

### Independent state

Disabled, hovered, pressed, focus-visible, selected/checked, busy and validation are independent axes with deterministic resolution. Selection never hides focus. Readonly is not disabled. Busy behavior explicitly blocks or allows action; if shipped, it preserves geometry and communicates outcome. Unsupported busy/validation/checked behavior must not be advertised merely because the style type has a field.

Accessible name precedence: explicit persistent name &gt; visible label composition &gt; documented semantic fallback. IconButton has no generic fallback: require a name. Placeholder is not a persistent field label. Values are not names. In-place label changes update the accessible name without replacing entity identity. Descriptions supplement, not replace, names.

### Activation

One canonical core activation entry point, not UI wrappers also firing events. Pointer: down arms/shows pressed state, valid release inside activates; release outside, Escape, lost capture, disable/unmount cancel. Define the selected platform policy for Enter timing, Space down/up, key-repeat and focus-loss cancellation; input sources need not share identical down/up timing, but each action produces **one intent**.

Modern policy applies per opted-in control. Legacy controls retain current press semantics in mixed-window tests. AT activation follows the same enabled/visibility/generation checks. Overlapping native/legacy/AT paths never double-fire. Editors retain IME, caret and text-key ownership.

The current single `Fired` slot is not an arbitrary lossless queue. **A1 must deliver either an independently reviewed additive canonical multi-intent solution, or an explicit tested supported-cardinality policy with overflow behavior.** No silent lost activations and no parallel UI dispatcher. Final API shape is an A1 design deliverable before A2 uses it.

### Assistive-action allowed matrix

Before dispatch validate: entity generation/existence, visible state, enabled/readonly state, current role and currently advertised action. Reject unsupported/stale/hidden/disabled actions without mutation. Focus routes through the same focus ownership path. SetValue is permitted only for an explicitly editable target and through the validated edit/history/selection/IME contract; otherwise denied. A read-only Preview never accepts mutation just because the backend can emit SetValue.

Tests include action queued before target destroy/hide/disable; readonly SetValue; unavailable action; tab activation; label updates; exactly-once input overlap. Missing bridge functionality is documented, not emulated with synthetic keys.

### Specific semantics

- Noninteractive Card/Callout are not automatic tab stops, clickable cards or alerts. Optional actions are explicit and never wrap nested controls in one giant clickable target.
- Callout severity uses icon/label/text, not color alone; urgency and announcements are intentional, not inferred from red styling.
- NavigationRow keeps selected distinct from focused; long labels retain full accessible text. Stable IDs survive updates and scrolling.
- ModeSelector distinguishes focused tab from active tab and controls a real associated panel. Host is responsible for preserving document/selection/undo state when switching.
- Source action carries an opaque typed host intent/locator, never executable Liquid text. Host validates source revision/span. UI does not own buffers or redo stacks.
- Future dialogs require focus containment/restoration, inert background, Escape policy, roles and real platform testing. Existing Modal pointer blocking alone is insufficient.

## 6. Document and application boundaries

Markdown/Liquid grammar, validation, parsing budgets, source offsets and unsupported-content policy live in the application/document adapter. It translates validated content into immutable presentation records/children. UI components cannot resolve URLs or interpret user text as tools, shell commands or app grants.

Preserve exact-source fallback for unknown/malformed/over-budget data. Rendering never edits source. Future source edits use the original per-note entity and validated operations, not preview-local undo histories or synthetic keystrokes. Styling must not imply that hidden-markup Live editing has become available.

### Blooey adoption recipe — later authorized app pass

- Native window controls and platform shortcuts stay conventional.
- Compact header: document identity, Source/Preview mode, essential frequent action/status; overflow only once a real menu exists.
- Sidebar becomes a quiet selectable list with search, not repeated push buttons.
- Reading pane gets space, controlled reading width and typography; fewer nested dark rectangles.
- Normal status is concise. Archive/reclaim/history diagnostics live in an inspectable panel.
- **Active unsafe-draft/save/recovery warnings stay prominent**; quiet UI never hides data-loss risk. Never claim saved/journaled if app state says otherwise.
- Destructive actions remain explicitly named and confirmed, outside primary/default focus.
- Source reveal becomes compact and keyboard-discoverable; fallback source remains visible.
- Preserve safe-close/recovery, shared Source Undo/Redo, source-range integrity, inert external content and existing safety tests. Preview remains read-only.

No Blooey migration is bundled silently into a rndr scaffold assignment.

## 7. Accessibility, adaptation and platform strategy

Use WCAG 2.2 AA as a practical applicable design/test target, **not native-library certification**. APG patterns inform keyboard behavior; native AccessKit/platform roles must still be implemented and tested. Custom GPU drawing does not inherit XAML/AppKit accessibility automatically.

Text contrast target: 4.5:1 normal, 3:1 genuinely large; necessary nontext controls/focus indicators 3:1 against adjacent surfaces. Audit resolved/composited pairs, not isolated swatches. Meaning uses shape/text/icon as well as color. Do not use disabled contrast exemptions to hide important explanatory content.

200% text-scale fixtures must retain controls/content without clipping; narrow-window and 400%-equivalent reflow tests specify logical viewport dimensions. Deliberate horizontal scrolling is permitted for genuinely two-dimensional code/comparison content, not accidental page-wide overflow. CSS pixels/native logical points/device pixels are not assumed identical.

Gallery covers long translations, CJK, Arabic/RTL, emoji/combining marks, CRLF/BOM source fixtures, multiline content, focused/selected overlap and nested scrolling. Claims of shaping, bidi navigation or localization readiness depend on actual tests, not presence of example strings.

Platform policy: shared semantics/grammar; platform-resolved font roles, accelerator labels/modifiers, density and eventual material support. macOS/Windows/Linux are separately reported. No copied macOS window decoration on Windows. OS preference following is separately verified; explicit appearance settings work without it.

Focus is visible, logical, unobscured and stable under updates. Hidden panels leave hit and accessibility trees. Card prose is screen-reader readable without adding a tab stop to every text fragment. AccessKit feature status is explicit for each consumer. VoiceOver/Narrator real-world acceptance remains separate from headless tree checks.

## 8. Verification and acceptance

### Required evidence

1. Existing core/default-example compatibility without opt-in UI; explicit Color/transparent/text semantics unchanged.
2. Token resolver/override/theme/contrast tests across Light/Dark/high-contrast settings.
3. Lifecycle/owned-subtree/stale-handle tests; updates while control/editor is focused preserve identity, selection/history/scroll and overrides.
4. Mixed legacy/UI pointer and keyboard fixtures; cancellation, repeat, exactly-once AT parity, event cardinality and schedule ordering.
5. Name/value/state/allowed-action tests; readonly denied, tab/panel semantics, no Shell filesystem effects, no duplicate Chrome installation.
6. Actual layout/paint/glyph geometry: bounds, wrapping, targets, clipping, long labels and nested scroll. World-state assertions alone are insufficient.
7. Deterministic gallery fixtures: fixed fonts/theme/viewport, Light/Dark/contrast, normal and enlarged type, 1x/2x/fractional scale, narrow/wide/long content. Goldens supplement behavioral tests.
8. Native manual matrix: keyboard-only, real pointer cancellation, IME/editor regression, VoiceOver(macOS), Narrator(Windows), high contrast/preferences where available. Each cell labelled verified/unverified/unavailable; no blanket OS parity from one machine.
9. Exact revisions, file list, commands/results, gallery run instructions and limitations. Disk preflight/cache reuse/bounded builds; no broad cleanup of unrelated worktrees.

Candidate baseline regression suites include core button/focus/keyboard/list/tabs/input\_platform/field/IME/editor/source\_edit and relevant paint/render tests; executor must discover exact current commands. They were **not run by this council**.

### Outcome labels

- **A0 accepted:** passive foundations demonstrable; interactive controls not complete.
- **A1 accepted:** required core contracts reproduced/tested and scoped compatibility verified.
- **A2 scaffold accepted:** opt-in crate + operating contracted controls + gallery + automated evidence. Missing native evidence is explicit and prevents unsupported production-ready/platform-accessible claims.
- Full polished library, native platform parity and Blooey adoption are later outcomes, not inferred from scaffold acceptance.

## 9. Held coder brief

After principal accepts this spec/scope, create a new isolated rndr undertaking from refreshed reviewed main. Inspect active work first; preserve all pending branches and do not reuse/adopt unrelated undertakings. Implement **A0 → A1 → A2** as bounded stages, with separately reviewable reusable core prerequisite changes and exact evidence. A0 alone does not fulfill the accepted usable scaffold scope.

Deliver namespaced opt-in `rndr_ui`, stable component lifecycle, documented style/interaction contracts, functioning gallery and regression coverage. Preserve legacy behavior/default appearance and original editor/source-history semantics. No parser/business-logic rewrite, Blooey edits, unapproved dependency adoption, remote push/publishing/deployment/signing, credential requests or spending. Owner retains independent review and native integration. No coder has been launched by the council.

Before A2 implementation, settle API spelling/action cardinality at A1 review; do not invent an already-existing API. Principal may request visual tuning against gallery before Blooey adoption. Library naming/branding and final accent are not locked by this specification.

## 10. Council decision record

### Research round

Taco researched live official Apple materials/accessibility and Windows Mica/Acrylic/type/accessibility guidance, distinguishing publisher statements from our defaults. Ada inspected pinned source and reported actual layer boundaries, focus/input/accessibility/lifecycle limits and proposed contracts. Host independently researched official materials/tokens/type/keyboard/target-size guidance, inspected rndr/Blooey boundaries and computed candidate palette sample ratios.

### Critique round

Both peers read the initial saved draft (hash `a79b67caa2a5905e7f24b827436e97e9a1ba7320fc58649c8bcb623ad9c08e5f`) and returned explicit amendments. Taco endorsed the direction for principal review, subject to density/reading-width clarification, staged gates, lifecycle, override wording and input-timing precision. Ada conditionally endorsed subject to explicit A0/A1/A2 sequencing, per-control opt-in policy, stable ownership, canonical event cardinality, AT allowed-action matrix and real tab/panel relations.

This revision incorporates those amendments. It is a **reconciled council recommendation**, not a claim the peers audited code that does not exist or approved a native implementation. Additional seed-palette work is host-authored; full visual/state tuning remains open.

### Resolved recommendations

- Additive `rndr_ui`, reuse core, no new renderer/runtime and no misuse of `rndr_std`.
- Opaque-first; no fake native glass and no glass document cards.
- Smaller behavior-complete release, not decorative breadth.
- A0 passive → A1 semantics/input prerequisites → A2 usable subset.
- Explicit compatibility and mixed legacy/modern per-control policy.
- Source access visible/keyboard-reachable, state/history preserved.
- Contextual diagnostics without hiding active safety warnings.
- Actual platform evidence, not certification language.

### Open engineering decisions (do not obscure them)

A1 exact additive API/action cardinality solution; reproduced accessibility findings and minimum semantic extensions; actual platform test availability; exact licensed fallback font/icon set; final hover/pressed/busy/disabled palette; independent native preference/material work. These are bounded engineering/design tasks, not a disagreement about the overall architecture.

## 11. Official research sources

Retrieved 2026-11-01. Guidance is current as retrieved, not a guarantee of future version behavior. Repo links pinned to baseline: [https://github.com/EntasisLabs/rndr/tree/291ab05493a59732f891dd762bf90c64e79a48d0](https://github.com/EntasisLabs/rndr/tree/291ab05493a59732f891dd762bf90c64e79a48d0) . Key files under `crates/rndr_core/src`: theme.rs, button.rs, chrome.rs, shell.rs, focus.rs, access.rs, layout.rs, tabs.rs, overlay.rs, text.rs; bridge `crates/rndr_render/src/a11y.rs`; `docs/source-backed-editing.md`.

- Apple HIG Materials: [https://developer.apple.com/design/human-interface-guidelines/materials](https://developer.apple.com/design/human-interface-guidelines/materials) — official Markdown retrieved at [https://developer.apple.com/tutorials/data/design/human-interface-guidelines/materials.md](https://developer.apple.com/tutorials/data/design/human-interface-guidelines/materials.md) . Liquid Glass functional/navigation layer, no content-layer overuse.
- Apple HIG Typography: [https://developer.apple.com/design/human-interface-guidelines/typography](https://developer.apple.com/design/human-interface-guidelines/typography) — official Markdown retrieved; platform-specific defaults/hierarchy/system fonts.
- Apple HIG Accessibility: [https://developer.apple.com/design/human-interface-guidelines/accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility) — official Markdown retrieved by host; scaling, contrast, non-color cues. Desktop/iOS size recommendations differ.
- Fluent tokens: [https://fluent2.microsoft.design/design-tokens](https://fluent2.microsoft.design/design-tokens) — global/alias semantic structure.
- Fluent accessibility: [https://fluent2.microsoft.design/accessibility](https://fluent2.microsoft.design/accessibility) — focus, hierarchy, contrast, adaptation.
- Windows materials overview: [https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/materials) .
- Windows Mica: [https://learn.microsoft.com/en-us/windows/apps/design/style/mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica) — opaque base, active/inactive/system fallback.
- Windows Acrylic: [https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic](https://learn.microsoft.com/en-us/windows/apps/design/style/acrylic) — transient/supporting use, no stacked/adjacent overuse, GPU/legibility/fallback constraints.
- Windows typography (Taco research): [https://learn.microsoft.com/en-us/windows/apps/design/style/typography](https://learn.microsoft.com/en-us/windows/apps/design/style/typography) .
- Windows accessibility (peer research): [https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility](https://learn.microsoft.com/en-us/windows/apps/design/accessibility/accessibility) .
- Windows keyboard interactions: [https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-interactions](https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-interactions) — tab/inner navigation, focus, safe initial focus.
- W3C target minimum: [https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html) — 24 CSS-pixel rule/exceptions, not a native-point universal size.
- APG tabs: [https://www.w3.org/WAI/ARIA/apg/patterns/tabs/](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/) — manual/automatic activation and latency.
- APG button/dialog (Ada research): [https://www.w3.org/WAI/ARIA/apg/patterns/button/](https://www.w3.org/WAI/ARIA/apg/patterns/button/) and [https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/](https://www.w3.org/WAI/ARIA/apg/patterns/dialog-modal/) — interaction references, not ARIA implementation instructions for a native library.
