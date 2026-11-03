# Exact execution commands and outcomes

Working directory: authorized selection-controls worktree. Shell setup used throughout:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
# CARGO_BUILD_JOBS=2; CARGO_INCREMENTAL=0;
# CARGO_PROFILE_DEV_DEBUG=0; CARGO_PROFILE_TEST_DEBUG=0
```

1. `rustfmt --edition 2021 crates/rndr_ui/src/data.rs crates/rndr_ui/tests/selection_corrections.rs` — exit 0.
2. `cargo test --offline --locked -p rndr_ui --test selection_independent_review --test selection_corrections` — initial exit 101: new fixture referenced nonexistent AccessNode disabled/readonly fields and Option<bool> instead of bool. Initial output was in probes.log, subsequently superseded by final gate output; tool transcript retains it. No passing count claimed.
3. Same Cargo command after API correction — exit 101; preserved `probes-final.log`: 3 new tests pass, 3 fail because the fixture did not retain property focus across frames / flush queued events before inspection. Cargo stops at the failed binary, so no claim that independent probes ran in this invocation.
4. Same Cargo command after focus/event fixture correction — exit 0; preserved `probes-focused-fixture.log`: 6 correction tests and 9 unchanged independent probes pass.
5. `bash evidence/selection-controls/corrections/run-bounded-gate.sh` — exit 0. Exact four Cargo commands, outputs and each EXIT recorded in final group logs. 668 executions pass. Script also records `git rev-parse HEAD; git status --short; df -h .` at completion.
6. `git diff --check -- crates/rndr_ui/src/data.rs docs/selection-controls.md` and `git diff --exit-code 97091b8 -- crates/rndr_ui/tests/selection_independent_review.rs evidence/selection-controls/independent-review` — exit 0. `git add crates/rndr_ui/src/data.rs crates/rndr_ui/tests/selection_corrections.rs docs/selection-controls.md; git commit -m "Fix inspector multichoice summary and set-valued transactions"` — exit 0, source commit 586c6e732f50aefc07ffae98b05a5b6423e41a81.
7. `bash evidence/selection-controls/corrections/capture-affected.sh evidence/selection-controls/corrections/native` — exit 0. Build command: `cargo build --offline --locked -p rndr --example selection_gallery --features observe`; per-preset exact validator commands are recorded in native/*.log and receipt.json. Six presets / twelve frames, no retries.
8. Direct image tool call once: path `evidence/selection-controls/corrections/native/inspector-multi/frame-2.png`. Metadata only; erroneous appearance commentary retracted. No raster/visual acceptance.
9. `PYTHONDONTWRITEBYTECODE=1 python3 evidence/selection-controls/corrections/audit.py` — exit 0; audit.json.

The checked-in scripts and logs are authoritative for final commands/counts. Development fixture runs overlap and must not be summed into the final gate. Historical evidence was not rewritten.

10. `rustfmt --check --edition 2021 crates/rndr_ui/src/data.rs crates/rndr_ui/tests/selection_corrections.rs` and `git diff --check 97091b8 -- crates/rndr_ui/src/data.rs crates/rndr_ui/tests/selection_corrections.rs docs/selection-controls.md` — exit 0, source-checks.log. `bash -n evidence/selection-controls/corrections/run-bounded-gate.sh evidence/selection-controls/corrections/capture-affected.sh` — exit 0.
11. `git diff --cached --check` — evidence-only logger whitespace/EOF warnings; source checks pass. `git add -f evidence/selection-controls/corrections/native` explicitly includes ignored native evidence. `evidence.sha256.json` binds every new evidence file except itself; changed-files.txt is the full 65-path inventory.
