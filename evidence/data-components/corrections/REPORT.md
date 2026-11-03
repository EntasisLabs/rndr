# ListView / TreeView — independent-review corrections

## Disposition and lineage

**R1 P1 and R2 P2 corrected and locally verified. Ready for sender review and independent re-review; not approved, integrated, pushed or deployed.**

- Historical baseline: `d0cbecf3524acd39fc9fc1c3cb35e33f04b62eeb`.
- Reviewed implementation: `865cbb3`; candidate `2b2a42a44c97efdbbb86316ccb4bb6a8144f0193`.
- Preserved independent-review / starting HEAD: `b4eddfc9b22439209ab274f237fb3f1bce3ecedb`.
- **Final correction source commit: `17d16f1f7143a2cb4b23176702fe5ed7ae5e1d06`.** The delivery HEAD is its evidence-only descendant, returned separately to the sender. The audit checks source-commit ancestry and exact current source bytes.
- Same isolated `rndr-composable-listview-and-treeview-foundation` worktree. No Blooey or other-worktree source changes, integration, approval, push, deployment, or broad cleanup.

Read the preserved reviewer `REPORT.md`, `adversarial.rs`, `run-adversarial.sh`, and `adversarial-six.log` before edits. All 264 pre-correction files under `evidence/data-components`, including the implementation evidence and independent-review evidence, remain byte-for-byte unchanged. Original reports are historical, not current-source certification. `audit.py` validates their manifests against the starting Git revision rather than falsely comparing historical hashes to corrected source.

## R1 — semantic effects at canonical chronological dispatch

Core now supplies the opt-in `ActivationHandler` component at `try_activate`'s accepted `ReleaseActivation` boundary. Collection row selection and disclosure use it. Core still decides eligibility, pointer release/cancellation, keyboard repeat/release/synthetic policy, AT admission, and FIFO64 capacity. On accepted activation, only that handler's newly admitted intent is consumed and its semantic effect runs synchronously; unrelated host intents remain in order. Overflow/rejected requests do not mutate collection state. There is no parallel raw-input queue or second dispatcher.

The late collection `requests` pass now only reconciles view/focus reporting; it no longer applies deferred selection/disclosure intent. Thus Enter→Right produces the same expanded branch/focused child in one batch as in separate frames, and AT Activate→Tab / pointer release→Tab cannot later pull focus back. This is not merely removal of the late focus call: expansion itself is committed before subsequent navigation. Row Enter's separate host-activation meaning is unchanged.

## R2 — invalidate only actual hidden ownership

`set_expanded` no longer calls global `cancel_activation`. Visibility reconciliation uses core `forget_interaction_targets` only on newly hidden owned row subtrees. Collapse still cancels hidden descendants' armed/queued interaction, clears invalid focus through core machinery, then restores safe ancestor focus. Selection remains independent. Unrelated host buttons retain an armed Space release during expansion **and collapse**.

The follow-through audit also found that unconditional normalization from a surviving disclosure to its owner row would cancel an otherwise valid armed disclosure in another branch of the same tree. The final setter preserves surviving canonical focus and repairs only invalidated focus. An additional regression covers both expansion and collapse of an unrelated branch while the surviving disclosure is armed. Actual disclosure activation still intentionally normalizes to its row before later navigation.

## Maintained regressions

`crates/rndr_ui/tests/collection_chronology.rs` is a default Cargo integration test, not an evidence-only fixture or opt-in feature:

1. Enter→Right in one batch.
2. AT disclosure Activate→Tab.
3. Disclosure pointer release→Tab.
4. Separate-frame Enter→Right control.
5. Focused disclosure becoming a leaf control.
6. Programmatic expansion preserving unrelated armed host Button.

These are the reviewer's six reproductions/controls, with **all assertions unchanged**. The audit compares the entire six-case prefix to the preserved source after rustfmt normalization via stdin. An initial comparison attempt included rustfmt's filename header and failed; the corrected comparison removes no source or assertions. See `reviewer-case-preservation.txt`.

Six additional maintained policy regressions cover FIFO64 overflow and unrelated intent order, repeat/release/synthetic/readonly/disabled/hidden rejection, hidden descendant Space cancellation even after reopening, unrelated host Space release during collapse, disclosure Space-release chronology and Escape cancellation, and surviving armed disclosure isolation during unrelated same-tree expansion/collapse. The maintained file has **12 tests**; the pre-existing collection suite retains **17 tests**.

## Final verification

All commands below passed on the final correction source with the documented compatible cache, `CARGO_BUILD_JOBS=2`, incremental disabled and dev/test debug info disabled. No new heavyweight target tree was created. The second full run and second native matrix were required after the surviving-disclosure correction; the first run is explicitly superseded under `intermediate/`, with its earlier source/binary bindings preserved.

| Gate | Final result / receipt |
|---|---|
| Core / paint / UI (including default regressions and doctests) | **570 passed**, zero failures: `verification/core-paint-ui.log` |
| Existing collection metrics | **17 passed**: `verification/collection-metrics.log` (already included above) |
| Native renderer + observation | **73 passed**: `verification/native-observe.log` |
| Fallback renderer | **25 passed**: `verification/fallback-renderer.log` |
| Facade startup, native / fallback | **1 passed each**: `verification/facade.log`, `verification/facade-fallback.log` |
| Python tools | **20 passed**: `verification/python.log` |
| Formatting | Passed, including maintained chronology tests: `verification/formatting.log` |
| Native gallery build | Passed: `verification/gallery-build.log` |
| Fallback gallery compile | Passed: `verification/gallery-fallback-check.log` |
| Preserved reviewer adversarial runner | **6 passed**, zero failures: `adversarial-six.log` |
| Current + historical integrity | `audit-result.json`; rerun command below |

Reproduction from the worktree root:

```sh
source scripts/data-gallery-env.sh
out=/path/to/fresh-correction-evidence
bash scripts/verify-data-components.sh "$out/verification"
bash evidence/data-components/independent-review/run-adversarial.sh
# After the documented native build, use the same fresh evidence root:
bash evidence/data-components/corrections/capture-native.sh "$out"
# Audit this committed correction packet (including its saved inspection record):
python3 evidence/data-components/corrections/audit.py
```

The adversarial runner is preserved unchanged and removes only its temporary integration-test copy. It does not replace the maintained tests.

## Final native evidence and actual image review

**Final binary SHA-256:** `88924c9429fedb4efcad3ee221a7cfb7ebd2a54974c0684031d8d8573b97ef1e`.

`binary.json` binds the documented build log and source commit; `source.sha256` binds the 16 implementation/test/doc/script files from that commit. Each `native-*/result.json` independently records the final binary hash, command arguments, Metal/Parley provenance, dimensions, capture rejection checks and shutdown result. Each native state has two byte-identical idle frames. Final capture folders are **not** the superseded `intermediate/native-*` folders. Final narrow and compact/high-contrast captures use the original dark flags; earlier intermediate equivalents used light appearance.

I opened and inspected each final `frame-2.png` directly with `cognition_image_view`, not just metadata, hashes or capture exit codes. Exact inspected PNG hashes and observations are in `screenshot-inspection.json`:

| Final capture | Actual fixture / image assessment |
|---|---|
| `native-light` | Light content surfaces, readable row hierarchy; full-row focus frame is distinct from selected fill. |
| `native-dark` | Dark surfaces and secondary text remain legible; focus frame and selection remain separate. |
| `native-narrow` | Dark 380×920 stacked layout; toolbar wraps, viewport clipping is contained rather than labels escaping horizontally. |
| `native-tree-focus` | Focus frame is on **Getting started, a leaf**, while **Keyboard and focus, its sibling**, is selected. |
| `native-disabled` | Both data views show disabled muted rows, no enabled-looking focus frame; host toolbar remains available. |
| `native-empty` | Both inset empty messages are visible and document status states no data; no ghost selected row. |
| `native-filtered` | Filter text is `no-matching-record`; both views give no-results feedback. |
| `native-compact-hc-200` | Dark enlarged high-contrast text, readable wrapped toolbar, explicit focus/selection separation; lower rows are viewport-clipped and remain scrollable, not a claim that all data fits simultaneously. |
| `native-large` | Shows 128/10,000 application-owned records, with the explicit paging controls and distinct row focus/selection. This is paging, not virtualization. |

### Erratum to the preserved original tree-focus description

The original implementation report's description of selected content as a **descendant** of the focused row is incorrect. The fixture focuses `ItemId(1)` (a leaf) and selects `ItemId(3)` (its sibling), under the same branch. The same correction is in the live consumer guide (`docs/data-components.md`). The old report is preserved without rewriting its history; this report and the guide supersede that specific claim. No visual claim of focused ancestor / selected descendant is made.

## Scope, residual risk, and handback

No remaining R1/R2 reproducer fails in the exercised tests. This is implementation-worker verification, **not independent acceptance**. Sender retains review, independent re-review and any eventual native integration.

Nonvirtualized retained limit remains **1,024 supplied nodes**, maximum tree depth 32; gallery uses explicit **128-record pages** over app-owned data. Single/no-selection and passive composed row content remain the only supported scope. No DataTable, multiple/range selection, nested editors, lazy loading or feature expansion.

Physical OS mouse/keyboard use, live assistive technology, real IME, touch/physical DPI and native Windows/Linux validation were **not performed**. Captures observe actual native Mac Metal/Parley surfaces, but do not automate physical interaction or prove screen-reader/platform certification. Input behavior is exercised through canonical programmatic core input/AT dispatch in the maintained integration tests.

Final disk observation: approximately **6.7 GiB free**. Compatible shared build outputs were reused; no source outside this isolated worktree, broad cleanup, cache deletion, approval, integration, push or deployment. The final delivery commit and clean worktree state are reported separately after the evidence-only commit.
