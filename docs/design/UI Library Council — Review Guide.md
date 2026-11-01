---
tags: ["design-council", "review", "rndr", "ui-library"]
---

# rndr UI Library — Council Review Guide

**Ready for your review — research/specification only. No coder launched.**

Full engineering specification: [[projects/rndr/UI Library Council — Specification]]. This guide supplies the short review summary and final clarifications; read both when preparing the scaffold assignment.

## Who did what

- **Taco:** current official Apple HIG and Microsoft/Windows design research; materials, typography, density, state design and accessibility.
- **Ada:** pinned rndr architecture audit; component ownership, focus/input, accessibility and realistic implementation/test boundaries.
- **Host:** independent research and source inspection, product scope/migration, palette sanity checks and synthesis.

We completed a research round and a critique round. Both peers read the initial draft and challenged concrete details; the reconciled specification incorporates the amendments. This is a recommendation for your review, not a claim the library exists or is platform-certified.

## Our recommendation in plain words

Build `rndr_ui`, a reusable component library in rndr, above the existing core. Don't rebuild the renderer and don't bury the design system in Blooey.

Aim for **quiet, content-first desktop UI**: clear navigation, a compact toolbar, comfortable reading surfaces, thoughtful Light/Dark themes and consistent interaction states. Borrow good Apple/Fluent principles, not an indiscriminate mixture of glass effects. Native materials can come later; solids should already look excellent.

A component means more than its appearance: its keyboard behavior, pointer cancellation, accessible name/state, focus, scaling, overflow and update lifecycle are part of its contract.

## Initial scaffold


| Stage                       | What it produces                                                                                                                                               |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **A0 — visual foundations** | Semantic colors, typography, spacing, density, surfaces and passive gallery                                                                                    |
| **A1 — shared behavior**    | Necessary reusable focus/input/accessibility contracts, separately reproduced/tested, with legacy compatibility                                                |
| **A2 — usable components**  | Buttons/icon buttons, navigation row, Source/Preview-style selector, toolbar/reading layout, Card/Callout and compact source action, with an operating gallery |


A0 can be shown early, but a passive gallery is not completion of the usable scaffold. Exact A1 APIs are engineering decisions, not invented existing APIs.

Later: real fields/menus/dialogs/toggles, Compare/Plan/Timeline compositions, optional platform materials and Blooey adoption. Full Live editing is not part of this UI work.

## Design defaults worth reviewing

- Neutral Light/Dark palette with replaceable blue accent; candidate values and sample contrast calculations are in the spec.
- Comfortable desktop controls start at 32 points minimum, grow as needed; compact is opt-in.
- Reading starts at 16/24 type with adjustable \~65-character measure.
- Rounded corners are restrained; selected navigation and focus remain visually different.
- Source access stays visible and keyboard-reachable, not a giant repeated CTA or hover-only icon.
- Blooey's routine diagnostics can move out of the way, but active unsaved/recovery/data-loss warnings remain prominent.

These are our proposed rndr defaults, not universal Apple/Windows requirements or a final brand decision.

## Final technical clarifications from Taco's palette critique

These refine the full specification and belong in any coder brief:

1. **Focus treatment:** 2-point ring with an initial 2-point contrasting gap/offset, expandable as needed. Focus=accent is only a color token, not the complete indicator. Filled accent controls need a contrasting gap/two-color treatment so the ring is visible against both control and surrounding surface. Test clipping, obscuration and high-contrast/forced-color behavior where supported.
2. **Required boundaries vs decoration:** subtle separators are decorative. Required control boundaries must use a tested strong treatment against their actual adjacent surfaces. The sample light/dark strong-stroke checks cover navigation/overlay; they do not certify all surfaces. Strong stroke is not ordinary small-text color (light stroke on white is only \~4.37:1).
3. **Selection is not blue alone:** navigation selection includes a marker/shape or other explicit non-color cue, with focus independently visible. State-specific foreground/background pairs remain required.
4. **Manual tab model:** accessible group name; separate focused-tab/active-panel state; orientation-appropriate arrows plus Home/End move focus, Enter/Space activates. Real tab/panel relations and inactive-panel focus/hit/AT exclusion are prerequisites. Translate interaction guidance into native semantics—do not add browser ARIA attributes to a native ECS and call it implemented.
5. **Contrast evidence:** host and Taco independently computed selected opaque palette ratios. Neither computation proves hover/pressed/selected/focused/disabled/busy or transparency-composited states. Those full resolved-style tests are scaffold deliverables.

## What is protected

Existing apps remain unchanged unless opted in. Preserve explicit color overrides, editor/IME behavior, source integrity, Undo/Redo and inert document content. Keep parsers, recovery logic and application commands outside the library. No second renderer/event loop. No silent adoption of pending work.

## What you are deciding

**Do you like this direction, the first component set, and the staged scaffold scope?** If yes, the specification already contains the bounded coder brief and acceptance gates. If the visual direction needs adjustment, change it here before coding—not after every app has copied a different style.

Documents created; research complete. No source/build/dependency changes, publishing or deployment. Actual cross-platform/native assistive testing remains implementation work, clearly separated from today's source audit.
