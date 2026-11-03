# Independent adversarial markdown host-slot / theme-bridge review

## Boundary and disposition

Reviewed candidate `69f1452a9d9fbbaa4c2184e6d501d9e0b6274cc1` against main `e46a3205bdff534278f5281d63b48d5039f3a9dc`. New reviewer, not the original implementer. This correction commit is on the same undertaking branch. **Deliverable only: not acceptance, approval, integration, merge, push or publication.** The exact final commit is the commit carrying this report (obtain with `git rev-parse HEAD`). Owner retains sealed review/integration responsibility.

Actual implementation, public API/docs, reconciliation, focus/input/scroll callers, Pane/default-style consumers, gallery, privacy policy and tests were inspected, not just the implementer's report. Parsing/stream assembly remain in core. No markdown interpreter, callbacks from markdown, or new execution syntax was added.

## Findings

1. **Medium / P2 — control focus survives an ordinary markdown text press. Fixed.** Generic focus intentionally retains focus on passive background clicks. The new host-focus guard then prevents markdown copy indefinitely, while Enter/Space can still act on the old embedded control. Reproduced with a real Button, fallback text press and copy. Markdown text presses now release focus through a small internal core path: synchronous normal focus-boundary/transaction notification, activation cancellation and state cleanup without duplicate next-frame notification. Regression coverage includes copy handoff across two views, exactly-once blur and armed Space cancellation. Presses inside hosts still yield to their real controls.

2. **Medium / P2 — embedded Editor selection drag also drags ancestor markdown scroll. Fixed.** The existing generic scroll capture was unaware of Editor selection ownership, newly exposed by embedding Editors in markdown. The deterministic probe moved the markdown offset from 0 to -20 while selecting editor text. Core scroll admission now yields left presses in Editor subtrees to editor selection/internal autoscroll. Wheel scrolling remains unchanged; the regression checks both selection and subsequent wheel movement. Splitter, ordinary scroll drag and editor-navigation regressions pass. This also corrects the same inherited conflict for Editors in other scrollports, without changing wheel or non-left-button policy.

3. **Documentation / limitation — positional identity is not source relocation. Clarified, not redesigned.** Content comparison deliberately ignores theme-derived colors, and binds semantic content at one index. An indistinguishable identical block inserted at that index preserves the old host; an insertion that changes that index retires it. A deterministic test captures both outcomes. Public API and guide now say this explicitly instead of making an unqualified insertion-retirement promise. No fuzzy matching, movement tracking or source-location identity is promised.

4. **Documentation / lifetime — subtree transfer. Clarified.** Root reparenting remains unsupported. A descendant explicitly reparented out before detach is no longer in the owned subtree and survives under normal core hierarchy semantics; a test proves this. Still-attached descendants cascade on removal.

No additional in-scope correctness/privacy defect was established. No unresolved high/critical finding is claimed.

## Audit conclusions and contract limits

- Mount validates synchronized view/index/content, finite positive height, per-view never-reused IDs and duplicate occupied indexes before mutation. Invalid fabricated public handle combinations (wrong view/root/ID) cannot resize or detach another binding. Handles are structural lifecycle identifiers, **not a security capability against trusted code that knows all public fields**. Valid copies deliberately refer to the same slot. Pending source conservatively disables liveness/resize but exact detach remains allowed. External root deletion/replacement, removal, full reparse, streaming append, changed preceding blocks and view destruction have regression coverage.
- Host roots and attached descendants are owned, not borrowed. Height is host-supplied and root layout/parent are core-owned. Width follows layout; scroll clipping is inherited. Hosts must update height for growing content. Arbitrary root/Children mutation and removal of MarkdownView without normal cascade are outside contract.
- Controls use canonical core focus/activation/selection/copy. Selection of markdown copies fallback source text, not embedded values. Tests exercise real Button, TextField and Editor plus multiple views/controls. This is deterministic input simulation, not native desktop certification.
- Theme conversion is explicit at UI -> core, world-local, and replaces four documented resources. UiPlugin alone does not enable it; changing UiTheme alone is not a watcher. Existing entity TextStyle and authored colors override defaults. Generated Pane fill refresh is equality/ownership-based and does not rebuild edit state; a different authored color relinquishes generated ownership. An identical-value write is inherently indistinguishable from no write. No new global or reverse dependency was introduced. Default TextStyle falls back to the old TextFace when no resource exists; core/paint/editor/Pane/theme tests pass. Static authored root fills still require explicit host update.
- Capture policy remains whole-frame opt-in, denies any sensitive marker including hidden descendants and unrelated targets, checks before identity/metadata, and never yields partial sensitive output. Renderer metadata and real GPU capture probes pass; embedded labels are not added to structural metadata. Real controls require host classification; markdown trust alone never attests them public.

## Independent verification

Environment: existing shared `CARGO_TARGET_DIR`, `source scripts/data-gallery-env.sh`, `CARGO_BUILD_JOBS=1`, offline locked serial bounded targets. No new target directory, cache deletion, worktree/history deletion or parallel heavy build. Initial free capacity approximately 1.2 GiB; a later inspection fell to approximately 934 MiB, then approximately 1.1 GiB was available at manifest sealing. A bounded read-batch intent encountered one `No space left on device` runtime error; subsequent shell inspection succeeded. No further heavy builds were attempted after capacity dropped. All requested gates below had already completed. New evidence is only in this directory; original `evidence/markdown-slots/` is immutable. Only new text logs have surplus trailing blank lines normalized for Git whitespace checks; no assertions/results were removed.

Authoritative reproduction: `candidate-reproduction.log`, against exact original candidate markdown/layout sources with corrected explicit synthetic pointer edges: **2 failures, 1 pass**, exit 101. The failures are the focus-copy and editor-scroll assertions above. Corrections were then restored. Early `pre-fix.log` / `second-probe.log` used an uncleared synthetic pressed edge and are exploratory, not authoritative proofs; `third-probe.log` fixes those edges, and `candidate-reproduction.log` is the definitive candidate comparison. They are retained rather than hidden. `fixed-probe.log` and `expanded-probes.log` record the progression; final evidence supersedes intermediate runs.

Final primary command (exit **0**):

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1 RNDR_CAPTURE=1
bash scripts/verify-markdown-slots.sh evidence/markdown-slots-review/final
```

`final/commands.log` records exact commands/results. Counts from final logs (not inherited or exploratory executions):

| Gate | Passed executions |
|---|---:|
| UI tests, including 7 newly authored adversarial tests | 200 |
| Core markdown/layout/text/editor/navigation/drag/interaction/Pane | 89 |
| Core parser unit filter | 8 |
| Markdown with highlight | 12 |
| Paint tests | 62 |
| Renderer capture tests | 20 |
| **Total Rust test executions** | **391** |

Additionally: gallery build with observe; facade gallery no-default-features check; UI docs; two actual GPU gallery renders and **18 real capture privacy probes** (9 per variant) passed. These are executions, not 391 unique tests. The implementer's reported 384 + 18 were inherited claims at review start; **391 + 18 above are this reviewer's fresh runs**.

Additional commands:

- `cargo clippy --offline --locked -p rndr_core -p rndr_ui --lib -- -D warnings`: exit 0 (`final/clippy-libs.log`).
- `cargo clippy --offline --locked -p rndr_ui --test markdown_adversarial -- -D warnings`: exit 0 (`final/clippy-adversarial.log`).
- Broader `cargo clippy --offline --locked -p rndr_core -p rndr_ui --tests -- -D warnings`: exit 101, two inherited needless-borrow warnings in unchanged `crates/rndr_core/tests/a1_wrapped_hug.rs` lines 79/125 (`final/clippy.log`). Not silently repaired outside scope; do not claim all-tests clippy passes.
- `cargo doc --offline --locked -p rndr_core --no-deps`: exit 0 with four inherited unresolved `Fired` links in unchanged `button.rs` (`final/core-docs.log`). Public host identity/lifetime comment clarification followed the main gate; library clippy/docs subsequently checked that exact source. Runtime code was unchanged after the gate.
- `git diff --check`, manifest verifier and final clean-status inspection are required when sealing this commit.

## Visual evidence and remaining gaps

Opened saved candidate and independently generated `final/light.png` (760x660) and `final/dark-narrow.png` (340x440) using direct image-view calls, not just capture metadata. The calls returned image artifacts with vision available. I do not treat those receipts as appearance acceptance and make no perceptual-quality claim here. These are bitmap/headless gallery snapshots, **not native visual, event-loop, pointer timing, IME, assistive-technology, font-quality or platform acceptance**. Own PNG digests match the candidate PNGs, as expected for interaction-only fixes. Same-frame structural metadata is in adjacent JSON files. No screenshot can establish focus/copy correctness; deterministic input assertions do that. Hands-on/appearance acceptance stays with the owner.

Native/IME/AT/MSRV/Windows/Linux certification remains out of scope and unverified. Also unverified: long-duration/high-volume mounting memory budgets and arbitrary unsupported hierarchy mutation. Headless gates do not satisfy those gaps. Capacity is still constrained: stop before additional heavy verification until the owner resolves storage safely.

## Integrity

`source.sha256` covers the candidate source set plus corrected interaction/layout and new test. `evidence.sha256` covers this report, verifier, source manifest, exploratory/final logs, PNGs and JSON, excluding itself. Run `python3 evidence/markdown-slots-review/verify.py` from the repository root. It validates current source/evidence, original candidate source blobs against the inherited source manifest, unchanged inherited evidence, and the pinned main OID. This report does not self-embed a final commit hash or manifest hash; return those from the sealing receipt to avoid self-referential hashes.
