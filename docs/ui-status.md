# Semantic UI status and integration boundary

## Current baseline and this increment

Baseline **`fe9e679`** includes the owner-integrated data-editor and selection-control work. `rndr_ui` is a real GPU-free component library above core, not only an A0/A1 proposal or an unaccepted A2 stub. Earlier A0/A1/A2 checkpoint files describe their exact historical review scopes; they do not override this status. Historical proposals, review evidence and captured results are retained, not retroactively changed into new approvals.

Implemented today:

- Semantic Light/Dark/high-contrast themes, typography, density, styling, cards, callouts, text and separators.
- Button/IconButton, navigation/source actions, mode selection, responsive toolbar and WorkspaceLayout.
- Stable-identity retained ListView/TreeView with separate focus, selection, activation and passive row slots.
- DataTable with application-owned rows, sort requests and declarative column widths; PropertyInspector with typed drafts, validation and explicit commit/cancel.
- Checkbox, SingleSelect and MultiSelect, standalone and in the inspector, with core anchored-popup/focus/dismissal behavior. Selection changes inspector drafts, not application records.
- Explicitly opted-in live observation of a running window, not component-targeted/headless capture.

This **first bounded consolidation increment** makes UiPlugin own scrolling, adds a collision-safe facade namespace, corrects documentation/metadata and closes CI coverage gaps. It is a candidate for owner review, **not executor approval or integration**. It does not start the unified-list/palette increment, overlays, markdown slots, capture architecture, native IME or startup work.

## Integration contracts

`UiPlugin` reuses core Chrome/GenericInteraction focus and activation. It installs `apply_scroll` at the front of Input so wheel/drag movement precedes canonical focus reveal/activation. Core consumes wheel input and advances drag's last-applied position: old Input or Shell/Update registrations can remain without multiplying movement. Reinstalling UiPlugin is a no-op. No Shell filesystem/global-selection behavior is added. New hosts need only UiPlugin; the galleries exercise that path, and regressions retain legacy-host combinations.

The facade's optional, default-off **`ui` feature** exposes `rndr::ui` as the same API as `rndr_ui`. Do not glob core and UI APIs together. Existing root `rndr::Button`, tray `Item`, `Theme` and virtual `List` keep their core meanings; use `rndr::ui::controls::Button`, `rndr::ui::collections::{Item, ListView}` and `rndr::ui::UiTheme` explicitly. No compatibility aliases rename or replace core types.

On ListView/TreeView rows, screen-reader **Activate** follows canonical click/Space selection; keyboard **Enter** emits `CollectionEventKind::Activated(id)` **without selecting**. These are intentionally different contracts. SelectionMode::None does not select. A host consumes Activated for its explicit open/run action; this cleanup does not alter behavior.

ListView/TreeView retain bounded rows (1024 supplied items); DataTable is bounded to 256 rows × 32 columns, and select popups to 256 options. None is a viewport-virtualized semantic list. Embedded interactive row controls remain unsupported. Core's virtual List and core Theme still exist separately; this increment does not claim to bridge them to UiTheme or migrate the palette.

## Observation and native limits

Feature `observe` is off by default and additionally requires an explicit app session/content policy. It returns the **whole live window surface** after ordinary draw commands and a matching **flattened accessibility tree** from the paint boundary. Privacy/policy and supported state-divergence checks fail closed for both channels. Public-content attestation is the host's responsibility, not automatic secret detection.

The Unix-socket/Python path supports macOS/Linux; non-Unix live transport is unsupported. It exposes neither a full layout tree nor UiIds/component targeting, is not headless rendering, and does not prove compositor presentation (`presented_confirmation=false`), physical user input or native screen-reader/IME behavior. See [native observation](native-observation.md) for exact supported checks and custom-engine mutation caveats. Successful capture metadata/hashes alone are not pixel inspection. Owner gallery checks supported prior integration but do not constitute a macOS/Linux/Windows certification matrix.

## Package and CI policy

`rndr_ui` stays **`publish = false`**, now with accurate description, existing README, keywords and category. The facade's optional local UI dependency has no registry version/policy; neither metadata nor passing builds makes it publication-ready. Do not publish this candidate or use the legacy release script for it. CI explicitly excludes internal rndr_ui and the facade from tarball packaging until dependency policy is deliberately resolved, while retaining their feature builds/docs and all other applicable package gates. No release enablement is part of this work.

UI now matches core/facade's **`deny(missing_docs)`** policy: the current library checked clean before tightening, without blanket suppressions. CI builds all four native semantic galleries with observe, runs the existing Python observe tests on Unix runners, and includes rndr_ui in no-default-features clippy. Facade unit/doc configurations exercise ui both with and without desktop defaults, while legacy no-ui configurations remain checked.

## Sustainable evidence handling

Do not delete archived `evidence/`, rewrite history or broadly clean other worktrees/caches to shrink a new diff. This increment adds no tracked captures or raw build logs. For future checks:

1. Reuse an explicitly chosen compatible shared Cargo target; use bounded jobs, no incremental/debug bloat where compatible, and inspect available space before each large gate. Stop rather than erase unknown caches when space is insufficient.
2. Put regenerated captures/logs in a **fresh private external directory** (or CI artifacts with a defined retention period), not a new bulky tracked evidence subtree. Existing capture/verify scripts accept output paths; pass an external path explicitly.
3. Track only compact review summaries/manifests as appropriate: exact source/command/configuration, exit results, hashes and an honest account of pixel/platform gaps. Never track observation capabilities, descriptors or private app contents. Archive retention/history changes require a separate owner decision.

See [data collections](data-components.md), [data editors](data-editors.md), [selection controls](selection-controls.md), and the [UI README](../crates/rndr_ui/README.md) for current APIs and gallery commands.
