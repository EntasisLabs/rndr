# Independent review — ListView + TreeView milestone

## Verdict

**Changes requested. Two concrete interaction defects remain. Do not accept this candidate as-is.** This is an independent review, not implementation-author self-verification, native owner approval, or integration.

- Reviewed candidate: **`2b2a42a44c97efdbbb86316ccb4bb6a8144f0193`**.
- Implementation: `865cbb3463eb2ab1c4460888ebfdfd3b37052857`.
- Baseline: `d0cbecf3524acd39fc9fc1c3cb35e33f04b62eeb`.
- Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-composable-listview-and-treeview-foundation`.
- Original approved direction checked against owner conversation `ses_0b76a3c56c494a8a920608be31128c63`, turns 260–263: public composable collection components, stable identity, independent focus/selection/activation, app-owned data, themed custom content, deliberate large-data strategy, interactive gallery, reuse rather than competing machinery. The present review request explicitly bounds this to retained 1024-node views, 128-record app paging, None/Single selection and passive content.
- No implementation changes were made. All reviewer additions are in **this distinct subfolder**. Original source, evidence and manifests are preserved. Any subsequent reviewer-evidence commit is not a corrected implementation HEAD.

## Severity-ranked findings

### R1 — High / P1: disclosure actions execute after later navigation, reversing chronological state and stealing focus

**Location:** `crates/rndr_ui/src/collections.rs:898–905` (`requests`), interacting with synchronous arrow handling at `:874–891`, the canonical key loop in `crates/rndr_core/src/interaction.rs:611–675`, and registration in `crates/rndr_ui/src/lib.rs:32`.

Canonical button activation queues a disclosure intent. Collection arrows act synchronously inside the key loop, but the disclosure is not toggled until the later `collections::requests` Input system. That later drain both toggles against the *new* state and unconditionally focuses its owner row. Input results therefore depend on frame batching, and an earlier disclosure action can override a later Tab.

Independently reproduced through real public components/core input:

1. **Keyboard:** focus a closed branch's named disclosure button; deliver `[Enter, ArrowRight]` in one rich-key batch. Expected: Enter expands, then Right focuses the now-visible child, branch remains open. Actual: Right first opens the branch, then deferred Enter closes it; focus ends on the branch row. The same inputs in separate frames pass the expectation.
2. **AT then keyboard:** `AccessCommand::Focus(toggle)`, accepted `AccessCommand::Activate(toggle)`, then Tab before the next frame. Expected: branch expands and later Tab leaves to the following host button. Actual: Tab is processed, then the deferred disclosure drain pulls focus back to the branch row.
3. **Pointer then keyboard:** pointer-press the disclosure; in the following frame deliver release plus Tab. Core processes release before keys. Expected: expand and leave to the following host button. Actual: the late drain pulls focus back to the branch row.

All three assertions fail deterministically in `adversarial-six.log`. This is not a physical-OS-only uncertainty and not the previously covered AT-**focus**→Tab case. Existing integration tests exercise disclosure activation and traversal in separate frames, so their passing result does not cover this gap.

**Required correction:** make collection-owned semantic state changes participate in the canonical chronological dispatch before later navigation observes them, including keyboard, AT and pointer paths. Preserve canonical eligibility/repeat/release/cancellation and queue-capacity policy; do not install another raw-key or action queue. Merely deleting the late `focus` call would address focus stealing but not the Enter→Right state reversal. Promote these three reproductions to normal passing regression tests when fixed, and refresh affected source/evidence binding.

### R2 — Medium / P2: changing an unrelated tree cancels another component's armed activation

**Location:** `crates/rndr_ui/src/collections.rs:742–744` (`ViewHandle::set_expanded`).

Every changed expansion calls global `cancel_activation(world)`, even when canonical focus and the armed input belong to an unrelated host control and no tree target becomes unavailable.

**Reproduction:** focus an ordinary public host Button outside the tree; press Space and render (assert the host is armed); programmatically expand the unrelated tree; assert focus still belongs to the host; release Space and render. Expected: the host's ordinary release intent is delivered. Actual: no host intent survives. The independent test `programmatic_expansion_does_not_cancel_unrelated_armed_host_button` fails. Expansion is a legitimate operation for an application receiving unrelated model updates, so this breaks component isolation rather than merely changing tree-local behavior.

**Required correction:** scope cancellation to the interaction targets actually invalidated by that view's visibility/state change. Reuse core's target-scoped lifecycle/cancellation machinery rather than clearing unrelated capture/Space state. Retain tests ensuring collapse still cancels an owned hidden descendant's pending activation, alongside this unrelated-host regression.

## Independent verification

### Commands and observed results

All builds ran **from the candidate worktree**, using the explicitly documented compatible Cargo output cache only:

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
python3 evidence/data-components/audit.py
bash scripts/verify-data-components.sh evidence/data-components/independent-review/verification
bash evidence/data-components/independent-review/run-adversarial.sh
python3 tools/validate_observation_native.py \
  "$CARGO_TARGET_DIR/debug/examples/data_gallery" \
  evidence/data-components/independent-review/native-dark -- --dark
```

The runner refuses an existing temporary test path, copies the preserved external reproduction fixture into an actual Cargo integration-test target, and removes only that temporary file on exit. This keeps intentionally failing reproductions out of the shipped default suite without hiding their result. It is expected to return 101 on the reviewed candidate.

| Check | Independent result / receipt |
|---|---|
| Original source/evidence integrity helper | PASS: 15 source entries, 231 evidence entries, nine final native states; `original-audit.json` |
| Formatting | PASS; `verification/formatting.log` |
| Full core/paint/UI tests | **558 passed**; `verification/core-paint-ui.log` |
| Focused public collection tests | **17 passed** (included in the 558); `verification/collection-metrics.log` |
| Native renderer/observe library | **73 passed**; `verification/native-observe.log` |
| Fallback renderer library | **25 passed**; `verification/fallback-renderer.log` |
| Facade startup, observe / no-default-features | **1 + 1 passed**; `verification/facade*.log` |
| Python tests | **20 passed**; `verification/python.log` |
| Gallery native build / fallback check | PASS; `verification/gallery-*.log` |
| Independent adversarial suite | **2 passed, 4 failed; exit 101**; `adversarial-six.log` (three R1 reproductions and one R2 reproduction) |
| Additional passing adversarial controls | Enter→Right in separate frames; focused disclosure becoming leaf normalizes to surviving row |
| Independent native dark recapture | PASS: native zero-exit capture, stale/auth rejection, idle identity, cleanup; `native-dark/result.json`, `capture-dark.log` |

Earlier `adversarial.log` and `adversarial-final.log` preserve the actual three-/five-test development runs; the **six-test** log is the final reproduction. No failing result is counted as passing verification. No repeated heavyweight build or cleanup was necessary. A separate attempt to import optional Pillow for possible contact-sheet preparation found it unavailable; no dependency was installed. This did not affect captures or direct image-tool inspection.

The independently reproduced gallery binary SHA-256 is **`a2851d1d83fa487066d6e8423893b39741a3c6ca3df03a570d813379fd1aff5e`**, matching the supplied binary receipt. The fresh dark PNG is byte-identical to `final-dark/frame-2.png` (SHA-256 `c9ac1f5219b6c50cd3c3138a45d63ac7a1601f02d07856c9f8d941f6e7c81c83`). Implementation and evidence-only follow-up have no source/doc/script differences.

### What was actually reviewed

I read the public collection implementation/setters/ownership validation, all 17 actual integration tests, shared synthetic gallery and launcher, core interaction/focus/accessibility diff and affected surrounding dispatch code, native AccessKit projection/tests, UI plugin/identity integration, API guide, implementation report, environment/verification scripts, integrity helper and manifests. This is not an inference from the worker's test counts.

- **Stable IDs / reconciliation:** entities and composed child subtrees follow IDs across reorder and rename; omissions destroy owned subtrees and drop state; reintroduced IDs get new entities. Selection clears on removal and item/ancestor disable; whole-view disable retains context. The supplied removal focus rule intentionally uses old supplied position clamped into available visible rows. No index-based selection resource was introduced.
- **Tree safety:** preorder/depth/duplicate validation is before mutation. Collapsed descendants retain data/content/selection but lose layout/input/AT presence. Safe-target traversal repairs descendant focus on ancestor collapse/reparenting. Branch→leaf expansion retirement and passive indentation behavior have real tests; the added auxiliary-disclosure-focus→leaf control also passes.
- **Interaction separation:** row click/Space selection, arrow focus and Enter activation are distinct in tested ordinary flows; None selection, repeat/synthetic input rejection, release cancellation, removal of armed targets and unrelated queued host intent preservation have actual coverage. **Chronological semantic effects and unrelated armed-state preservation fail as above.**
- **Disabled / empty / scroll / reveal:** tests use real components, frames and core scrolling; disabled ancestry, empty named focus viewport, clipping, scroll drag and separate reveal semantics are exercised. Whole-row reveal is a narrow extraction of existing core machinery, not a new scroll engine.
- **Accessibility:** List/ListItem and Tree/TreeItem projection, logical hierarchy despite flat layout, levels, expanded state, selected=false as well as true, actual focused target, named disclosure commands and unavailable actions are tested. Row AT Activate means selection, not host Enter activation, as documented. Native tests do not replace screen-reader trials.
- **Identity / lifetimes / composition:** component-family identity conflicts and generation-checked lifetimes are tested; content is deliberately collection-owned passive content, destroyed on omission/unmount. Sharing borrowed entities and nested editors are not advertised. Stateful application data must stay outside the ephemeral subtree. Explicit `UiStyle` is the caller's override seam, while managed structure must not be overwritten.
- **Regression scope:** legacy pooled List, filesystem Disclosure/Shell and application repositories were not changed. Core additive seams affect all GenericInteraction users, making R2 particularly important. `AccessRole::Tree/TreeItem` are source-breaking for downstream exhaustive matches; this is documented and needs downstream adoption work, not an unverified compatibility claim.

## Screenshots actually opened with image tools

I invoked `cognition_image_view` directly on every final `frame-2.png` below, and on the independently recaptured dark frame. These are actual Metal/Parley native surface pixels, not rendered Markdown, hashes-only inspection, or static mockups. Full filenames/PNG hashes are in `screenshot-inspection.json`.

| Supplied path under `evidence/data-components/` | Independent pixel assessment |
|---|---|
| `final-light/frame-2.png` | Readable primary/secondary hierarchy, quiet content surfaces, selected row fill distinct from separate focused-row outline. |
| `final-dark/frame-2.png` | Dark content/raised surfaces and text remain coherent; selected fill and focus outline remain distinct. Fresh independent capture matches exactly. |
| `final-narrow/frame-2.png` | 380×920 stacked panels and toolbar/filter arrangement are usable at this fixture size; partial bottom row is normal viewport clipping, not evidence of virtualization. |
| `final-tree-focus/frame-2.png` | Tree full-row focus frame includes the indent, while a different row is selected. **Report-label correction:** the fixture focuses tree ItemId(1), a leaf, and selects sibling ItemId(3), not a selected descendant of the focused row; it does not visually certify branch-focus behavior. |
| `final-disabled/frame-2.png` | Disabled content/disclosure ink and retained selection context are visible without a live collection focus outline. Host controls remain separately enabled. |
| `final-empty/frame-2.png` | Empty feedback is inset and legible; the focused list viewport has a clear boundary; no orphan rows are visible. |
| `final-filtered/frame-2.png` | Actual query is visible in the filter field; both no-results messages and empty viewport focus boundary are legible. |
| `final-compact-hc-200/frame-2.png` | Strong field/state boundaries and enlarged UI labels are visible; wrapping and lower feedback remain within this fixture. Normal viewport clipping may scroll the initial selected item away. This is UI-text scaling, not a physical 200% DPI/AT certification. |
| `final-large/frame-2.png` | Previous/Next page, page count, 10k record feedback and explicit retained/paging caption fit; actual mounted content is only the page. |

No additional material visual defect was established from these static states. They cannot expose R1/R2 or prove live mouse/keyboard/screen-reader behavior.

## Scoped performance and future DataTable assessment

The retained design is consistent with the scoped contract, **not a virtualization implementation**. Independent focused metrics: 1024 supplied rows retain **6147 layout entities**, first headless frame **279.57 ms** in this run. The gallery retains **1639 layout entities** for 128-record pages while its app model owns 10,000 records. Costs remain proportional to supplied rows across layout/style/accessibility; maximum-cap use is not a smooth-frame promise. No finding demands unapproved virtualization, multi/range selection or nested editors.

`ItemId`, app-owned metadata, persistent passive `RowSlot`, explicit view-tagged events, core TabStop/navigation/reveal and logical accessibility projection are useful foundations for later DataTable. Keep column/cell identities separate from row IDs; sorting belongs to app/data callbacks, and actual header/column sizing, cell navigation, resize/sort semantics and accessible table relationships still need design. The current scalar selected/active state and row-only KeyNavigation/CollectionAccess roles should not be advertised as a completed grid contract. Tree preorder/expansion must remain tree-specific. Fix the chronological interaction boundary before reusing it for grid cell/column interactions; no DataTable was implemented during this review.

## Preservation / remaining gates / handback

- Disk inspected at start and after verification: **6.7 GiB available**. Compatible cache reused in place; jobs=2, incremental=0, dev/test debug=0. No broad cleanup, duplicated heavy cache, Blooey edits, other-worktree source edits, owner approval, integration, push or deployment.
- Original evidence is intact; reviewer artifacts and their own manifest are isolated here. The independent manifest excludes itself to avoid circular hashing. Logs preserve raw stdout.
- Real physical OS keyboard/mouse/touch, screen readers, IME, Windows/Linux, native DPI changes and fallback-renderer native appearance remain unverified platform/manual gates. Only this Mac's Metal/Parley native rendering was independently reproduced.
- This review is complete with **R1/R2 unresolved**, not a claim of milestone completion. I have not made a superficial fix that bypasses canonical dispatch or weakened tests to obtain a green result. Sender retains final review and integration; arrange corrections, rerun the six adversarial cases plus the existing suite, refresh affected evidence/hashes, and review the resulting exact implementation HEAD before acceptance.
