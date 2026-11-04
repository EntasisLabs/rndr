# Native document editing — E2.1 input candidate

Status: experimental keyboard/composition increment after E1 commit `aab3860`.
This is **not E2 completion or native platform certification**. The epic's E2
checkbox remains open: semantic text ranges, interactive scrollbars, captured
selection/autoscroll and the native platform ledger are separate work.

## Implemented contract

- Unmodified Page Up/Down moves by the editor's current logical viewport height,
  minus text insets and one shaped row of overlap, with at least one row of
  motion in a tiny viewport. It uses full-document shaped rows, not newline
  counts or byte columns. Resize changes the following page distance.
- Page motion shares Up/Down's preferred-x and `CaretRow` wrap endpoint hint.
  Shift retains the directed anchor; reversing pages can collapse and reverse
  selection. End rows clamp without discarding preferred x. Navigation does not
  create content revisions, change events or undo steps.
- Both managed source sessions and legacy Buffer editors use this movement.
  Release and synthetic edges do not edit/move; held page keys may repeat.
  Ctrl/Alt/Super-modified Page keys are left unclaimed. Home/End and Page
  movements request caret reveal just like existing arrow navigation.
- Outside composition, plain Tab and Shift-Tab traverse canonical focus when
  `GenericInteraction` is installed, including Tab returned from popup
  dismissal to an editor anchor. Editors do **not** implicitly insert indentation.
  Composite `KeyNavigation` hooks still run first and may explicitly own keys.
  Synthetic/released/repeated Tab edges do not advance focus.
- While managed source composition owns native input, candidate keys including
  Tab/Escape/Home/End/Page and duplicate key text cannot reach menus, focus
  traversal, editing or a different widget. Native IME commit/cancel is the
  boundary; Escape is not simulated as native cancellation by the surface.

## Composition lifetime

A private owner record contains the source document and generation-qualified
view entity. It is independent of the live `SourceBinding` enumeration.

Chrome captures the native lifecycle before generic focus processing and
readonly consumer filtering. The editor consumes that prepared state instead
of reinterpreting scrubbed text as a native cancellation. A guard runs again
before fields and after host Update, covering pointer focus changes and view
teardown inside Update. Standalone `apply_editor` still prepares its own input.

- The original document/selection remains the only commit target.
- Moving focus away keeps a live owner's document locked while native preedit
  persists, but quarantines its transient text. It does not transfer preedit
  or a late commit to the newly focused source or field.
- Destroying the owner clears its document's composition lock while retaining
  an owner tombstone until native commit/cancel. Reattaching the same document
  to a replacement entity does not grant that entity the old native commit.
- A native empty preedit/no-commit frame cancels; window blur cancels explicitly.
  A late orphan commit and duplicate key text are discarded, never retargeted.
- Commit into the original live/focused view is one source transaction/history
  entry; document-host edits/reload continue to refuse while composing.

This increment fixes source-traced review findings: destroyed-view orphan
retargeting, readonly sanitization releasing another document's lock, and
popup-dismissal Tab swallowed by editor key capture. New regression tests
exercise those paths through Chrome plus GenericInteraction.

## Movement/affinity compatibility boundary

Existing Left/Right and word commands retain **logical source/grapheme order**;
this is not a new visual bidi cursor model. Existing platform `standard_action`
mappings remain authoritative: macOS Alt-word/Cmd-line/Cmd-document and
non-macOS Ctrl-word/Home-End/Ctrl-Home-End. Line-start/end commands still mean
logical newline-delimited source line edges, not physical screen-left/right
or soft-wrap visual row edges. Up/Down/Page use shaped row geometry and the
existing endpoint row hint, not a new public universal position type.

Mixed bidi/wrap endpoint behavior needs the named native/shaped-font ledger
before ratifying a broader affinity API. No structured positions, multi-caret,
rectangular selection, indentation provider or app-specific command policy was
introduced here.

## Verification and limitations

New tests:

- `crates/rndr_core/tests/editor_page.rs`: viewport distance, preferred x,
  forward/reverse directed selection, no revision/history effects, soft wraps,
  resize, caret reveal, readonly Unicode/CRLF and composition admission,
  repeat/release/synthetic/modifier ownership; managed and legacy variants.
- `crates/rndr_core/tests/source_lifecycle.rs`: dead owner/replacement view,
  readonly-field focus and late duplicate text, native blur, destruction during
  host Update, IME candidate-key ownership, single commit/undo, ordinary and
  popup-returned Tab/Shift-Tab traversal.
- `crates/rndr_core/tests/ime.rs`: resolver and legacy widget suppression for
  Home/End/Page while native preedit is active.

Regression commands (run from the isolated worktree):

```sh
cargo test -p rndr_core -p rndr_ui -p rndr_render -p source-session
cargo check --workspace --all-targets
cargo fmt --all -- --check
git diff --check
```

Result: **786 passed, zero failed, one intentionally ignored cost probe** across
these packages and doctests. Workspace all-target compilation, formatting,
whitespace and local documentation link/fence checks passed. The follow-up
read-only correctness review found no concrete blocker in the inspected input
changes, while recommending blur, host-Update teardown and post-orphan typing
coverage; those cases are included and pass in the final lifecycle suite.

Private receipts: `target/e2-input-tests.log`, `target/e2-input-check.log`.
These are automated/headless checks, **not real OS input evidence**. No native
Japanese/Chinese/Korean/dead-key IME, clipboard/primary, VoiceOver/NVDA/Linux AT,
out-of-window capture, platform candidate rectangle or new performance pass
was performed in this increment. Existing E1 large-document costs remain.

Try the same fake-host demo with `cargo run -p source-session`; Page Up/Down
(and Shift variants) and Tab traversal now use the contracts above. On compact
Mac keyboards, verify the platform's Fn-arrow Page key translation rather than
assuming that headless NamedKey events certify the hardware path.
