# Independent re-review — ListView / TreeView milestone

## Disposition

**R1 (P1/high) and R2 (P2/medium) are resolved on the reviewed correction delivery. No remaining implementation blocker was established. The complete bounded milestone is technically acceptable for sender review/acceptance, subject to the explicit scope and outstanding manual/platform gates below.** This independent recommendation is not owner approval, project acceptance, integration, push or deployment.

- Reviewed delivery: `1957e1ece7cd4a3777e069e4899873d14073bee3`.
- Correction source: `17d16f1f7143a2cb4b23176702fe5ed7ae5e1d06`.
- Prior independent review: `b4eddfc9b22439209ab274f237fb3f1bce3ecedb`.
- Historical baseline: `d0cbecf3524acd39fc9fc1c3cb35e33f04b62eeb`.
- Same isolated `rndr-composable-listview-and-treeview-foundation` worktree; continuing the original independent review, not substituting worker self-verification.
- Read both preserved reports, the entire correction diff and maintained chronology tests, affected surrounding canonical dispatch/queue/cancellation/focus code, collection visibility/setter/activation code, fixture/launcher, verification/capture scripts, integrity helpers and manifests. The prior review's unchanged implementation/API/accessibility/lifetime assessment carries forward after checking the exact correction diff and rerunning its coverage.
- No implementation, maintained-test, guide or script edits. All new reviewer files are isolated in this folder. Temporary Cargo test copies were removed by their runners. The eventual evidence-only commit does not change the reviewed implementation.

This report supersedes the earlier independent review's **unresolved R1/R2 disposition**, not its historical observations or provenance. All historical files remain unchanged.

## Findings and correction acceptability

### R1 — resolved: accepted semantic effects precede later navigation

`crates/rndr_core/src/interaction.rs:try_activate` now performs eligibility and FIFO64 admission first. Only an opted-in `ReleaseActivation` Button with `ActivationHandler` consumes its just-admitted request and immediately runs its semantic effect. The new intent is popped **before** invoking the callback, with no queue borrow held across it. It cannot pop an earlier unrelated host request, and overflow cannot run the handler. This is inside the existing pointer/keyboard/AT activation entry point, not a competing raw-input/action queue.

Collection rows and real disclosure commands opt in through `sync`. Their effect runs before the next chronological key observes expansion/selection/focus. The late `requests` system only reconciles/reports focus; it no longer drains deferred selection/disclosure actions. Leaf reconciliation removes the obsolete handler with the obsolete Button/ReleaseActivation/RowOwner. Row Enter retains its separate host-activation meaning.

All three original failure cases now pass **unchanged**: disclosure Enter→Right in one batch, AT Activate→Tab, and pointer release→Tab. Separate-frame Enter→Right and disclosure Space-release→Tab also pass. The tests establish expansion itself is committed before subsequent navigation, not merely that a late focus call was removed.

**R1 correction acceptable.**

### R2 — resolved: cancellation/focus repair follows invalidated ownership

`collections.rs:sync` calls `forget_interaction_targets` only for newly hidden owned row subtrees. `set_expanded` no longer globally cancels another component's Space/pointer state. On collapse, hidden descendants lose armed/queued references through core machinery; invalidated focus moves to the safe surviving ancestor. Selection remains independent.

The setter also checks the actual surviving canonical focus target. It does not normalize an otherwise valid auxiliary disclosure to its row merely because another branch changes. The maintained regression explicitly arms one disclosure, changes an **unrelated branch of the same tree** in both directions, and verifies focus/arm survive and release still expands the original branch. This important follow-through is present in the final source, not only the intermediate correction.

Original unrelated-host expansion case, additional unrelated-host collapse case, hidden descendant cancellation/reopen case and surviving armed disclosure case all pass.

**R2 correction acceptable.**

### Shared core hook / framework regression assessment

- FIFO64 remains a bound on **pending** requests, not a new per-frame activation budget. Full host queues reject collection effects without eviction, state mutation or order loss. Accepted local effects free their own admission slot synchronously. Ordinary host Buttons without a handler keep their existing queued intent/revalidation semantics; legacy non-release Buttons retain Fired behavior.
- Eligibility still goes through `access_details`: hidden/stale/disabled/readonly rejection is not bypassed. Existing input policy still owns pointer hit/release/capture cancellation, Escape/blur, Space arm/release, repeat and synthetic rejection. The hook adds no raw input handling.
- Two additional evidence-only reviewer tests exercise **nested different handlers**, self-hiding during callback, unrelated host FIFO order, the last available capacity slot and nested overflow. Both pass. A callback can re-enter after its own intent has been consumed; no stale queue borrow or post-callback pop corrupts nested requests.
- This is not a recursion/panic sandbox: application-supplied callbacks must terminate and uphold their own invariants, as with existing synchronous hooks. No unbounded recursion is used by the collection handlers. No new guard/feature was added by the reviewer.
- The complete existing core/paint/UI, native/fallback renderer, facade and Python suites were independently rerun. No ordinary-host/framework regression was established. Physical OS/AT behavior is not inferred from these tests.

### E1 — Low / P3, nonblocking: correction evidence prose has wrong fixture labels

`corrections/REPORT.md` and `corrections/screenshot-inspection.json` describe the tree-focus leaf as **Getting started** and selected sibling as **Keyboard and focus**. The actual source and pixels show **Brand refresh (ItemId 1)** focused and **Release checklist (ItemId 3)** selected. Their leaf/sibling relationship and the correction to the old ancestor/descendant claim are correct.

Those files also describe the filter as `no-matching-record`; the actual launcher and pixels contain **`no matching project`**. No-results behavior is real. This is an evidence-description error, not a source/binary/capture binding failure or an R1/R2 defect. This paragraph and this review's `screenshot-inspection.json` supply the exact erratum; original evidence was intentionally not rewritten. Sender should use these actual labels when describing the screenshots. No implementation correction is requested for E1.

## Independent verification

The authoritative repeated run used the documented compatible shared cache explicitly, `PATH="$HOME/.cargo/bin:$PATH"`, jobs=2, incremental=0 and dev/test debug=0. See the environment receipt under `verification/`.

| Gate | Observed independent result |
|---|---|
| Correction audit, including historical preservation | Pass: 264 original files unchanged; saved `correction-audit.json` |
| Full core / paint / UI | **570 passed**, zero failures; includes 12 maintained chronology tests and 17 collection tests |
| Maintained chronology/policy regressions | **12 passed**, including same-tree surviving armed disclosure; in `verification/core-paint-ui.log` |
| Existing collection tests / metrics | **17 passed**, separately reproduced; already included in 570 |
| Preserved original external adversarial runner | **6 passed**, assertions/fixture unchanged; `adversarial-six.log` |
| Additional reviewer reentrancy/FIFO tests | **2 passed**; `activation-policy.rs`, `activation-policy.log` |
| Native renderer / observation | **73 passed** |
| Fallback renderer | **25 passed** |
| Facade startup, native / fallback | **1 passed each** |
| Python tools | **20 passed** |
| Formatting | Passed, including maintained chronology tests |
| Native gallery build / fallback gallery compile | Both passed |
| Fresh native capture matrix | **All nine captured**, exit 0, idle identity, stale/auth rejection and clean shutdown checks pass |
| Independent integrity audit | `audit-result.json`: all 431 pre-existing delivery evidence files unchanged; all 16 source bindings match correction source; original six-case normalized prefix preserved |

These counts are not added together to double-count the focused suites or duplicate original cases.

### Reproducible commands

From this worktree root, using a **new** output path:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
export CARGO_BUILD_JOBS=2
source scripts/data-gallery-env.sh
python3 evidence/data-components/corrections/audit.py
bash scripts/verify-data-components.sh /path/to/new-output/verification
bash evidence/data-components/independent-review/run-adversarial.sh
bash evidence/data-components/independent-rereview/run-activation-policy.sh
bash evidence/data-components/corrections/capture-native.sh /path/to/new-output
# Audit this saved independent review packet, not the new output folder:
python3 evidence/data-components/independent-rereview/audit.py
```

`reproduce.sh /path/to/new-output` bundles the verification, original adversarial runner, extra reviewer tests and captures with explicit cache/PATH settings and refuses an existing output directory. It does not replace actual pixel review. Both temporary-test runners refuse an existing test target and remove only their own temporary source on exit. The original runner's historical comment that failures are expected applies to the old reviewed candidate, not this correction.

## Native pixels and source/binary/capture bindings

**Independently rebuilt binary SHA-256:** `88924c9429fedb4efcad3ee221a7cfb7ebd2a54974c0684031d8d8573b97ef1e`, identical to the correction delivery. Fresh `binary.json` records delivery HEAD; the correction packet records its source commit. Their source bytes agree, verified through the 16-entry source manifest and the unchanged tracked tree.

I opened all nine final correction `frame-2.png` files directly with `cognition_image_view` and inspected actual native pixels, including a two-column nine-state contact sheet to review the matrix together. The sheet is a nearest-neighbor derivative of those native PNGs, not a mockup. Per-file original PNG hashes and actual assessments are in `screenshot-inspection.json`. Image-tool artifact hashes can describe re-encoded display renditions; source PNG integrity uses disk-byte hashes, not rendition receipts.

The independent recapture produced **all nine frame-2 PNGs byte-identical to the corresponding correction-delivery PNGs**, and each state's two idle PNGs are byte-identical. The audit checks per-state final binary hash, PNG dimensions, Metal/Parley provenance, metadata, authorization rejection and clean shutdown. This is stronger than merely trusting worker capture success, but still not physical interaction certification.

| State | Independent pixel assessment |
|---|---|
| Light | Quiet content surfaces and readable primary/secondary hierarchy; Brand refresh focus outline distinct from Release checklist selection. |
| Dark | Content surfaces and secondary ink coherent; selected fill and focused outline remain separate. |
| Narrow | Dark 380×920 stacked composition and wrapped toolbar; viewport-contained text clipping rather than horizontal escape. Lower content scrolls. |
| Tree focus | Brand refresh leaf has full-row outline including indent; Release checklist sibling is selected. Does not demonstrate focused ancestor / selected descendant. |
| Disabled | Data row/disclosure ink muted; retained selection context without enabled-looking data focus; host toolbar remains separately enabled. |
| Empty | Both inset empty messages and no-data feedback visible; no orphan row/selection. |
| Filtered | Actual query `no matching project`; both views show no-results messages and no data rows. |
| Compact / high contrast / 200% UI text | Enlarged text, strong field/state boundaries, wrapping toolbar; ordinary lower viewport clipping, not a claim all data fits. Not physical 200% DPI. |
| Large / paged | Explicit Previous/Next, page 1/79 and 128/10000 feedback; mounted page only, no virtualization claim. |

No additional material visual defect was established in this static matrix. It cannot expose all live interaction or certify Apple/Fluent platform behavior.

## Complete bounded milestone assessment

The original review's substantive coverage of stable IDs/reconciliation, owned passive composition/lifetimes, separate focus/selection/activation, filtering/reorder/removal, tree validation/collapse/branch-to-leaf, disabled/empty behavior, core scroll/reveal and logical accessibility projection remains supported by the passing actual component tests. Corrections are coherent with that architecture; they remove the two observed interaction gaps without expanding the feature scope.

The retained limit remains **1024 supplied nodes**, maximum depth 32. This run reproduced **6147 layout entities** for 1024 rows and about **278.59 ms** for the first headless frame; not a smooth-frame or virtualized performance promise. Gallery application paging retains **1639 layout entities** for a 128-record page while the app owns 10,000 records. Costs remain proportional to supplied nodes. Single/no-selection and passive row composition are the scope. No multiple/range selection, nested editors, lazy loading, DataTable or virtualization was implemented or required by this review.

The prior future-DataTable assessment still applies: stable ItemId/RowSlot/app-owned model and canonical hooks are reusable foundations; cells/columns, sorting/resize/header semantics, grid navigation and accessible table relationships require later design. The scalar selection/row navigation contract is not a completed grid. New Tree/TreeItem enum roles still require downstream exhaustive-match adoption; no unverified downstream compatibility claim is made.

## Operational record, preservation and remaining risks

- Initial disk: **6.7 GiB free**. My first invocation omitted the explicit shared-cache path, so the environment script defaulted to a newly created worktree-local `target`. The complete initial verification logs are preserved under `initial-local-cache-verification/`; its original adversarial log is also preserved. A stop signal arrived after that initial command chain had completed. It is not the authoritative cache-compatible run.
- I repeated the full suite and capture matrix with the exact shared-cache path. The initial mistake temporarily reduced free space to about **4.7 GiB**. I then verified the filesystem **birth time of every local target entry including its root** was within this review's initial-run interval, with no symlinks, tracked files or pre-existing entries. I removed only those exact enumerated turn-created artifacts: **3909 files, 2,211,131,703 bytes**. `local-cache-reversal.json` records the full path/size/birth-time proof. This was reversal of my own generated scratch output, not broad/stale-cache cleanup. The existing shared cache was not deleted or cleaned.
- Final observed disk: **6.9 GiB free** (`final-disk.txt`). No duplicated heavy local target remains. No Blooey/other-worktree source edits, broad cleanup, integration/approval, push or deployment.
- All **264** original implementation/review evidence files and all **431** files present in the correction delivery are byte-for-byte preserved. The independent audit checks against exact Git revisions rather than treating historical source hashes as current-source certification. New evidence has its own manifest; raw stdout logs, including trailing spaces, were not rewritten to satisfy whitespace checking.
- Physical OS keyboard/mouse/touch, live assistive technology/screen readers, real IME, physical/native DPI, Windows/Linux and fallback native appearance remain **unperformed**. This Mac's native Metal/Parley rendered surfaces and canonical programmatic input/AT tests are the verified evidence, not a substitute for those gates.
- Sender retains final review and any native integration. This evidence-only handback recommends accepting the bounded R1/R2 correction, not declaring the whole framework finished or waiving manual/platform validation. Exact clean evidence HEAD is returned separately after commit.
