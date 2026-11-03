# Corrected complete combined candidate — owner re-review handoff

## Identity and authority

Verified starting HEAD: `39b049feb32b50a7000bcb60ead62e16ee4fe8e2`.
Initial tracked worktree was clean; only `independent-combined-review/`,
`owner-editor-gap-check/` and `owner-editor-gap-check-retry/` were untracked under
`evidence/data-components-next/`. All three remain intentionally untracked and
unchanged. The entire combined candidate is retained; this is not a standalone
prerequisite or DataTable approval.

Focused source correction commit: **`d9d9c3c998e67c8da332167cc7f179e4d373508b`**.
The evidence-only commit carrying this report is the final handoff candidate;
its exact OID is returned to the owner in the terminal response. No source
changes occurred after the source correction commit. Native source and binary
bindings refer to that exact source commit. The gate ran those same worktree
bytes immediately before their source commit, so its environment records the
starting HEAD rather than the subsequently created source commit.

**Return for one combined independent re-review and owner native lifecycle.**
No candidate/project acceptance, merge, approval, apply, push, integration,
deployment or Blooey operation was performed. Acceptance of the prior review
deliverable was not interpreted as candidate acceptance.

## Findings corrected

* **F1 / P1:** outstanding commit requests now survive later draft edits.
  Value-only host reconciliation matching a pending request advances the baseline
  and retires that request and older ones, without replacing the later draft,
  caret, composition or typing history. `1, Enter, 2` in the real gallery commits
  through `1` and retains `2` as uncommitted draft text. Multiple outstanding
  acknowledgements and later invalid drafts are covered. Unrelated external
  value replacements, object/contract changes and explicit cancellation still
  reset drafts/history; host models are never mutated by the inspector.
* **F2 / P2 and F3 / P2:** an additive opt-in canonical `FocusBoundary` notifies
  a property root synchronously at actual subtree departure, before the next
  queued input. Same-property descendant navigation does not notify. Window
  blur notifies even when the Boolean editor, a Commit button or a custom
  descendant is focused. Choice and custom boundary paths have added controls.
  Leave/return in one input batch cancels at departure, so the returning Commit
  cannot submit the stale draft. Post-update containment remains a fallback,
  not the chronology mechanism. This reuses the existing canonical transition
  sites and input loop, not a second dispatcher.
* **F4 / P2:** `transaction_key` copies `Input.ime_enabled` alongside preedit and
  commit into the canonical per-key `text_frame` filter. The unchanged-intent
  public Input reproduction now inserts `日`, not `n日`. This establishes the
  public snapshot contract only; **no real OS IME reproduction is claimed**.

All five reviewer tests were promoted to the existing three maintained suites,
with the same assertions/intent (only rustfmt formatting). Their original
fixtures and failing logs remain byte-for-byte in the untracked review directory.
Four additional maintained tests cover caret/undo plus external replacement,
multiple delayed acknowledgements plus validation/contract cancellation, choice
blur/departure, and custom-descendant within-property retention/blur/departure.
Controls for within-property Commit navigation, object changes, validation,
readonly/disabled policy and host ownership continue passing.

`FieldTransactions` remains opt-in; legacy unopted fields and Blooey are not
migrated. The five original legacy failure probes were not rerun in this pass;
the prior unchanged review logs remain historical evidence, not new passing proof.

## Exact corrected files

* `crates/rndr_core/src/field.rs`
* `crates/rndr_core/src/lib.rs`
* `crates/rndr_ui/src/data.rs`
* `crates/rndr_ui/tests/data_editors.rs`
* `crates/rndr_ui/tests/data_editors_gallery.rs`
* `crates/rndr_ui/tests/field_transactions.rs`
* `docs/data-editors.md`

All other additions are evidence in this directory. No other source family or
application changed. `changed-source-files.txt` and `audit-receipt.json` contain
exact names and SHA256 bindings.

## Commands, exits and counts

Run from this isolated RNDR checkout with Cargo on PATH and the existing
compatible cache:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
bash evidence/data-components-next/independent-combined-review/reproduce.sh evidence/data-components-next/review-corrections/focused-final
bash scripts/verify-data-editors.sh evidence/data-components-next/review-corrections/verification
bash scripts/capture-data-editors.sh evidence/data-components-next/review-corrections/native
python3 evidence/data-components-next/review-corrections/audit.py
```

These recorded output directories already exist: use fresh names to rerun the
first three commands. Each top-level command exited **0**. The audit is read-only
and may be rerun without an output argument (prints JSON). Builds were offline,
locked, two jobs, incremental and dev/test debug disabled, reusing the authorized
compatible target cache. No cache cleanup or dependency refresh occurred. Free
disk was approximately **4.5 GiB** before and after this pass.

Original focused runner (runs every suite despite failures):
**16 data + 10 field + 6 gallery = 32 passing executions**, exits 0/0/0, including
the five previously failing assertions and all 27 original controls. Exact
commands and outputs: `focused-final/{data,field,gallery}.log`.
`focused-1/` retains the earlier exploratory passing rerun before eliminating
duplicate inspector text-only blur handling; it is not the authoritative gate.

Maintained focused run:
`cargo test --offline --locked -p rndr_ui --test data_editors --test data_editors_gallery --test field_transactions`
exited **0**, **20 + 6 + 10 = 36** passing executions (`maintained-1.log`). The
same maintained suites are included in the final full gate, not extra unique
tests to add to its count.

Full bounded gate: **10 command groups, 717 passing executions**, no failures.
The increase from the previous 708 is exactly five promoted tests plus four
additional controls. These are execution counts, not unique tests; overlapping
focused, feature and doc-test runs are not summed as unique coverage. Original
six collection reproductions, two activation-policy tests and the maintained
12 chronology regressions pass unchanged.

| Gate log | Exact recorded command | Exit | Passing executions |
|---|---|---:|---:|
| `core-paint-ui.log` | `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui` | 0 | 608 |
| `facade-fallback.log` | `cargo test --offline --locked -p rndr --lib --no-default-features` | 0 | 1 |
| `facade.log` | `cargo test --offline --locked -p rndr --lib --features observe` | 0 | 1 |
| `fallback-renderer.log` | `cargo test --offline --locked -p rndr_render --lib --no-default-features` | 0 | 25 |
| `formatting.log` | `rustfmt --edition 2021 --config skip_children=true --check crates/rndr_core/src/access.rs crates/rndr_core/src/field.rs crates/rndr_core/src/interaction.rs crates/rndr_core/src/lib.rs crates/rndr_render/src/a11y.rs crates/rndr_ui/src/collections.rs crates/rndr_ui/src/controls.rs crates/rndr_ui/src/data.rs crates/rndr_ui/src/lib.rs crates/rndr_ui/tests/data_editors.rs crates/rndr_ui/tests/data_editors_gallery.rs crates/rndr_ui/tests/field_transactions.rs crates/rndr_ui/examples/support/data_editors_gallery.rs crates/rndr/examples/data_editors_gallery.rs` | 0 | 0 |
| `gallery-build.log` | `cargo build --offline --locked -p rndr --example data_editors_gallery --features observe` | 0 | 0 |
| `gallery-fallback.log` | `cargo check --offline --locked -p rndr --example data_editors_gallery --no-default-features` | 0 | 0 |
| `historical-six-and-activation.log` | `cargo test --offline --locked -p rndr_ui --test combined_original_six --test combined_activation_policy` | 0 | 8 |
| `native-observe.log` | `cargo test --offline --locked -p rndr_render --lib --features observe` | 0 | 74 |
| `whitespace.log` | `git diff --check` | 0 | 0 |

Environment/disk records are in `verification/`; commands, log digests and test
binary paths/SHA256s are sealed in `audit-receipt.json`. Formatting and
baseline-to-source whitespace validation also pass. The later whole-evidence
diff check reports exit 2 for verbatim command-log trailing spaces and one
terminal blank line; these raw outputs are intentionally preserved, not scrubbed.
This does not indicate source whitespace failures or a failed test command.

## Refreshed native bindings — no pixel acceptance

`native/` contains all **12 states / 24 frames**: light, dark, narrow,
narrow-inspector, compact-hc-200, compact-hc-200-inspector,
compact-hc-200-bottom, invalid, readonly, disabled, empty and workflow-bottom.
Every native run exits 0, with normal shutdown and exact binary/source/PNG/
metadata bindings. The 131-file source manifest matches the corrected source;
all 24 PNG/metadata pairs revalidate. These are native macOS Metal/Parley
pre-present GPU surface-copy captures, **not compositor/presentation proof**;
`presented_confirmation=false` throughout.

Native binary SHA256: `7957800360fefe9d0b37d2b142ddb1b96cde6146b50004712817a23fc5546aee`.

Native receipt SHA256: `b63b5c9f4df557ebf2e7c602819be667e0c425c772366d225da8d01a97449049`.

**Pixel inspection NOT performed in this correction pass. Independent visual
sign-off remains OUTSTANDING.** A direct `cognition_image_view` attempt on
`native/light/frame-2.png` returned metadata with `ok=true` and
`vision_available=true`, but no inspectable pixel attachment. Neither that
receipt nor provenance/geometry tests establish clipping, contrast, legibility
or visual correctness. `visual-status.json` records this explicitly. The prior
review's lack of independent pixels is not retroactively resolved by refreshed
captures. Follow-on review must receive and inspect actual pixels.

## Preservation audit

All **3,396 existing evidence files / 66,869,705 bytes** match the initial SHA256 snapshot.
This includes **3,354 tracked files / 66,628,094 bytes**, also checked against their baseline Git blobs, and **42 untracked review/owner files**.

No previous report, fixture, retry log, owner reproduction or PNG was deleted or
rewritten. `preserved-evidence.json` binds the entire preexisting set;
`audit.py` revalidates it and all corrected source/native bindings.
`bundle.sha256.json` binds new files in this correction directory except itself.
The only untracked worktree entries after evidence commit should be the same
three preserved review/owner directories, with no tracked diff.

## Limits and owner next step

No new feature scope was added: table remains bounded to 256 rows x 32 columns,
not virtualized, passive cells, single/no selection and declarative widths (no
pointer-drag resize/autosize/spreadsheet editing). Inspector remains explicitly
described/custom-composed, with host-owned models, no reflection/persistence/app
undo and no accessible SetValue support. Pending requests require host handling;
value reconciliation acknowledges matching outstanding values, not an opaque
transaction-ID protocol.

Physical input, live assistive technology, real OS IME, compositor presentation,
physical DPI and Windows/Linux remain unvalidated. No benchmark or platform
certification is claimed. **Owner must arrange combined independent re-review,
including actual pixel inspection, then decide candidate acceptance and the
native lifecycle.** This worker has neither approved nor integrated anything.
