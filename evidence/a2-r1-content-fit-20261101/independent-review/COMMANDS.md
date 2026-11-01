# Fresh independent review commands and provenance

All repository-relative commands ran in:
`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.

```sh
O=/tmp/rndr-a2-fresh-review-4TfleE
B=/tmp/rndr-a2-independent-SQwoYo/exact
```

No source edits, approvals, merges, pushes, network, launches or cleanup. New files were confined to `$O` and the small `/tmp/rndr-a2-fresh-review-location` pointer.

## Inspection and preflight

```sh
pwd; git status --short; git rev-parse HEAD; df -h .
find docs -type f | sort
find evidence/a2-r1-20261101 -maxdepth 2 -type f | sort
wc -l docs/design/* evidence/a2-r1-20261101/{REPORT.md,COMMANDS.md,verify-ui.sh,verify-rest.sh}
git diff 31006f0dfb834ea90e8b20ebac3b41d10dcdf88d..HEAD -- crates docs
```

Read full Specification in bounded ranges 1–80/81–165/166–250/251–339; Review Guide; A1-api-decisions; A2-components; A0-A1-checkpoint (1–46/47–100/101–147); implementation-tracking; complete prior REVIEW; correction REPORT/COMMANDS; both runner scripts. Inspected controls.rs, all new allocation-test assertions and relevant core allocation, snapping, field-text-width and text-layout paths. Relevant original expected baseline and initial correction failed logs were inspected, not treated as final results.

```sh
shasum -a 256 evidence/a2-r1-20261101/{REPORT.md,MANIFEST.sha256,independent-review/REVIEW.md}
shasum -a 256 -c evidence/a2-r1-20261101/MANIFEST.sha256 > "$O/worker-manifest-validation.log"
```

## Fresh build and actual tests

Copied only inspected scripts, not worker UI artifacts:

```sh
cp evidence/a2-r1-20261101/verify-{ui,rest}.sh "$O/"
sh "$O/verify-ui.sh" "$B" "$O/exact-ui"    # exit 0, 37 UI tests, gallery twice
```

Validated every receipt row from original exact source-before inventory except changed UI, and all binary/current-rlib/dependency-rlib inventories with Python SHA256. For each of the 147 unchanged source paths also compared bytes to `git show HEAD:path`. Counts in `reuse-full-validation.json`: 147 / 79 / 3 / 5. No source or dependency mismatch. Then:

```sh
sh "$O/verify-rest.sh" "$B" "$O/exact-ui" "$O/checks"   # exit 0
```

The saved scripts are the exact executed command definitions, with complete rustc inputs, Cargo environment/flags, per-command transcripts and logs. They include jobs=2, dev/test debug=0, incremental=0 and offline --locked existing-target Cargo tests/checks/clippy/fmt. The rest script rehashes reused artifacts and recompiles unchanged independent adversarial/geometry probes against the new UI. Counts: Cargo520+24, repeated direct411+62+37+2, separate conformance13, adversarial5, geometry1(64configs), final-regressions5. These overlap exactly as described in REVIEW.md.

Independent baseline run (not the worker's retained log):

```sh
rustc --edition=2021 -C debuginfo=0 --test crates/rndr_ui/tests/a2_toolbar_allocation.rs \
 -L "dependency=$B/current" -L dependency=target/debug/deps \
 --extern "rndr_core=$B/current/librndr_core.rlib" \
 --extern "rndr_ui=$B/current/librndr_ui.rlib" \
 -o "$O/baseline-regressions" > "$O/baseline-compile.log" 2>&1
"$O/baseline-regressions" --nocapture > "$O/baseline-regressions.log" 2>&1
# compile exit 0; run exit 101, 0 passed/5 failed (expected negative control)
```

## New independent probes

Authored `fresh-probes.rs` and diagnostic `fractional-survey.rs` outside source. Final command:

```sh
rustc --edition=2021 -C debuginfo=0 --test "$O/fresh-probes.rs" \
 -L "dependency=$B/current" -L "dependency=$O/exact-ui/current" -L dependency=target/debug/deps \
 --extern "rndr_core=$B/current/librndr_core.rlib" \
 --extern "rndr_ui=$O/exact-ui/current/librndr_ui.rlib" \
 -o "$O/fresh-probes-verified" > "$O/fresh-probes-verified-compile.log" 2>&1
"$O/fresh-probes-verified" --nocapture > "$O/fresh-probes-verified.log" 2>&1
# compile 0, run 101: 2 passed, 2 failed; integer text inset and fractional snapping
"$O/fresh-probes-verified" unwrapped_fit_must_include_text_engine_inner_inset --nocapture
```

The final integer probe reproduces core's public-source inner-width formula `(rect.w - 2*12).max(engine.unit_advance(style))` and reads real `TextLayout.lines`. This is not an assertion of merely cosmetic difference. Diagnostic survey compiles identically with source/binary paths replaced by `fractional-survey.rs`/`fractional-survey`; 175 configurations, mismatch printing intentionally non-fatal. Its four harness tests are not new acceptance regressions.

Earlier initial three-test own run, failed public/private API attempt and their logs/source snapshot remain preserved. Final verified probe log is authoritative; no failed log was deleted. The survey isolates repeated updates at widths1696/1697/1700/2000 and logical scales1/1.75.

## Source/artifact confirmation

Full tracked crates+Cargo SHA256 inventory before/after confirmation:

```sh
git ls-files -z crates Cargo.toml Cargo.lock | xargs -0 shasum -a 256
shasum -a 256 -c "$B/dependency-rlibs-sha256.txt"
```

Repeated whole final-UI rlib build after full input verification:

```sh
CARGO_PKG_VERSION=0.3.1 CARGO_PKG_NAME=rndr_ui CARGO_MANIFEST_DIR="$PWD/crates/rndr_ui" \
rustc --edition=2021 -C debuginfo=0 -C metadata=a2_handoff_ui_exact \
 -L "dependency=$O/exact-ui/current" -L "dependency=$B/current" -L "dependency=$PWD/target/debug/deps" \
 --extern "rndr_core=$B/current/librndr_core.rlib" \
 --extern "rndr_paint=$B/current/librndr_paint.rlib" \
 --extern "insta=$PWD/target/debug/deps/libinsta-ceeb3b5188bdb88b.rlib" \
 --crate-name rndr_ui --crate-type rlib crates/rndr_ui/src/lib.rs \
 -o "$O/confirmation/librndr_ui.rlib"
cmp "$O/confirmation/librndr_ui.rlib" "$O/exact-ui/current/librndr_ui.rlib" # 0
```

An earlier confirmation with output basename `librndr_ui-confirmed.rlib` compiled successfully but binary comparison differed because ar members include that basename. Retained both binaries and `ar -t` inventories. Same-basename rerun is byte-identical. UI rlib hash67c2e5b4fe93182d92ff494753955541fc7ed1ae16235899e0db4b9b42e86494. Verified all final UI .rs and Cargo manifest/lock bytes against exact candidate Git blobs; full-source/dependency checks pass at end. UI binary hashes are in `exact-ui/binary-identities-sha256.txt`; reused binary/input receipts copied into `confirmation-before/`.

## Evidence, geometry and negative harness audit

First original manifest check used wrong CWD and failed to open relative paths; preserved `original-manifest-validation.log`. Correct command:

```sh
(cd /tmp/rndr-a2-independent-SQwoYo && shasum -a 256 -c MANIFEST.sha256) \
 > "$O/original-manifest-validation-correct-cwd.log" 2>&1 # 404 valid, exit0
```

Python audit compared every504 `git ls-tree -r --name-only 31006f0d -- evidence` path's original Git bytes with current bytes; original UI-test files byte-identical and contain32 test functions; seal457 additions only. `audit-summary.json` records assertions. All new gallery-one files compared to gallery-two; all passive files compared to evidence/ui-gallery; historical comparison changes exactly6 files/3 states. For the three changed states compared every access-node line after replacing only `bounds: Rect {...}`; semantic identity/order unchanged (`three-gallery-audit.json`). Inspected full display-list/semantics gallery diff, not merely repeat hashes.

```sh
git rev-parse HEAD; git status --porcelain
shasum -a 256 -c "$O/all-source-before-confirmation.sha256"
shasum -a 256 -c "$B/dependency-rlibs-sha256.txt"
rustc -Vv; cargo -V; df -h .; du -sh target "$O"
```

## Manifest

`MANIFEST.sha256` uses standard SHA256 + two spaces + review-root-relative path, all review files except itself and its validation log. Verify from review directory:

```sh
cd "$O"
shasum -a 256 -c MANIFEST.sha256
```

No unique-coverage inflation: repeated direct library and gallery runs are explicitly labeled repeated; diagnostic survey printing mismatches is not declared a correctness pass. No native/platform certification.
