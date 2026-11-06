# Desktop foundation recovery — 2026-11-06 UTC

## Provenance and reconciliation

The stranded desktop-app readiness foundation was adapted onto `main` at
`f1b91e235d8b77c4342cbf5d0a8510bbfe32cee5` from:

- undertaking `work-e34eea8aac8625b96cec9bf7cfd991c0`;
- branch `worktree/rndr-desktop-app-readiness-foundation`;
- commits `53cfe295ba21bb4b7d0190e5398b53df368b1481` and
  `f58c91b1f33cd4d7529177fa2435bae1f035e6de`;
- merge base `6e611f59eaa3768cb946c757339abe63384af6c7`.

The original undertaking, branch and worktree were left intact. No branch
switch, merge, rebase or cherry-pick was used. Main's refactors, semantic UI,
engineering guidance, README and CI checks were retained.

| Candidate work | Recovery decision |
| --- | --- |
| Unpublished `rndr_app` helpers and bounded JSON preferences | Recovered as optional low-level ECS helpers; no facade dependency or replacement of `rndr_ui` |
| Offline desktop example, draft settings, cancellation and stale-result tests | Recovered and adapted to current core |
| Old `FormNavigation` input queue splitting | Replaced by idempotent `GenericInteraction`; fields use `FieldTransactions`, buttons use canonical release activation and `ActionIntents` |
| Old `apply_field_keys` and `reset_text_field` core additions | Not restored; current transaction dispatch and `reset_field_draft` own those behaviors |
| New compatibility findings | Canonical fields now apply aggregate legacy text before Tab/blur, honoring readonly; Tab stays on a composing/committing field. Four direct core regression tests cover these boundaries, including preservation of existing undo/redo history |
| Field accessible captions | Persistent `Semantics` names replace the old assumption that a placeholder names an input |
| Local macOS bundle/icon/window-smoke scripts | Recovered; packaging explicitly selects the fresh native target artifact, overriding configured Cargo build target/output paths. Two portable mocked regression tests cover stale artifacts, build failure and existing-bundle refusal. Still a local demonstration, not distribution approval |
| Docs/CI/lockfile/workspace | Integrated into current versions; historical readiness evidence is explicitly labeled |

The example consumes canonical intents in Update and revalidates targets after
prior actions. It uses generic app interaction/scrolling, not Shell's file-tree
keyboard policy. Native clipboard/IME and assistive technology remain core and
platform concerns, not an alternate app runtime.

## Fresh verification

Verified locally on macOS. Generated logs and captures live under ignored
`target/desktop-recovery-20261106/`; original undertaking artifacts were not copied.

- `cargo test -p rndr_app -p desktop --locked --offline`: **17 tests passed**,
  including queued Unicode + Tab + save, cancel/discard undo isolation,
  persistence, invalid drafts, async cancellation/retry/stale completion,
  structural scrolling/accessibility and static-text paint.
- `cargo test -p rndr_core --test field_form_chronology --locked --offline`:
  **4 tests passed**, covering text/publication before blur, existing-history
  undo/redo, readonly discard, and legacy/rich IME preedit/commit with Tab.

Additional checks:

- Strict lint passed without allowances for core/UI libraries, all app-kit and
  desktop targets, and the new core regression target:
  `cargo clippy -p rndr_core -p rndr_ui --lib --locked --offline -- -D warnings`,
  `cargo clippy -p rndr_app -p desktop --all-targets --locked --offline -- -D warnings`,
  and `cargo clippy -p rndr_core --test field_form_chronology --locked --offline -- -D warnings`.
- `python3 scripts/check-desktop-packaging.py`: **2 tests passed**, using mocked
  build/platform tools. This tests artifact selection/control flow, not native
  packaging or release acceptance. CI runs this portable check on Linux.
- `sh -n scripts/package-desktop-macos.sh`: passed.
- Fresh GPU export completed with `cargo run -p desktop --locked --offline --
  --memory --export target/desktop-recovery-20261106/render`: workspace at
  880×760 and 1760×1520, and narrow settings at 420×640. All passed nonuniform
  readback checks. This does not establish native-window/aesthetic acceptance.

Broad strict lint remains **not green** in untouched baseline code. The workspace
all-target run reported unused `prefix` in `crates/rndr/examples/ui_polish.rs:119`;
expanded affected all-target runs also reported a needless update in
`crates/rndr_ui/tests/structured.rs:80`, `filter_next` in core structured
`operation_tests.rs:208`, and unnecessary mutable arguments in
`structured_native_tests.rs:474,495,499,569,619`. No blanket allowances or
unrelated lint cleanup were added. These broader failures remain visible rather
than being described as successful strict workspace lint.

Final combined workspace rerun after both review fixes:
`cargo test --workspace --locked --offline` — **1,212 passed, 0 failed, 5 ignored**
across 177 test binaries/doc suites. Ignored tests remain ignored, not signed off.

- `RUSTDOCFLAGS='-D warnings' cargo doc -p rndr_app -p desktop --no-deps
  --locked --offline`: passed.
- `scripts/package-desktop-macos.sh dev`: passed on this Mac, including icon
  generation and `plutil` validation. Produced the ignored local bundle
  `target/bundle/Desktop Workshop.app`. Its embedded executable's `--help`
  completed successfully. This was a dev-profile local build, **not** a GUI
  launch test, release-profile build, codesign/notarization or distribution test.
- `cargo fmt --all --check` and `git diff --check`: passed.

Independent read-only integration review found the pre-Tab history and configured
Cargo-target packaging issues above; both were fixed with regression coverage.
The review did not mutate files or certify native acceptance. Generated exports,
logs and the local bundle were preserved under ignored `target/`, not committed.

## Remaining acceptance gates

Tests above are not native IME, clipboard, VoiceOver/NVDA, Linux/Windows,
multimonitor/DPI or signed distribution acceptance. Structural access-tree and
paint-command assertions are not pixel or assistive-technology signoff.

[desktop-readiness.md](desktop-readiness.md) preserves the original historical
ledger and open gates. [desktop-apps.md](desktop-apps.md) describes current usage.
No release-readiness or cross-platform quality claim follows from this recovery.
