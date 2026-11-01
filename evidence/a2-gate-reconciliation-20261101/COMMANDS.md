# Reconciliation commands and reproducibility boundary

This is the reconciler's record, distinct from the complete unmodified independent
[COMMANDS.md](independent/COMMANDS.md). No Cargo/rustc/build/native command was executed here.
All shell operations ran in the admitted undertaking worktree; original independent files were read only.

## Inspection actually performed

```sh
pwd; git status --short; git rev-parse HEAD; df -h .
find . -maxdepth 3 -type f \( -iname '*spec*' -o -iname '*review*' -o -iname '*a2*' -o -iname '*api*' -o -iname '*checkpoint*' -o -iname '*tracking*' \) | sort
ls -la /tmp/rndr-a2-rereview-OgFoQ8
# Complete bounded reads (also read via code.read):
sed -n '1,115p' 'docs/design/UI Library Council — Specification.md'
sed -n '116,235p' 'docs/design/UI Library Council — Specification.md'
sed -n '236,339p' 'docs/design/UI Library Council — Specification.md'
cat 'docs/design/UI Library Council — Review Guide.md' docs/design/A2-components.md docs/design/implementation-tracking.md docs/design/A1-api-decisions.md
sed -n '1,78p' docs/design/A0-A1-checkpoint.md
sed -n '79,147p' docs/design/A0-A1-checkpoint.md
cat /tmp/rndr-a2-rereview-OgFoQ8/REVIEW.md /tmp/rndr-a2-rereview-OgFoQ8/COMMANDS.md
find evidence -iname 'REVIEW.md' -o -iname '*review*.md'
cat evidence/a2-r1-20261101/independent-review/REVIEW.md
cat evidence/a2-r1-content-fit-20261101/independent-review/REVIEW.md
cat /tmp/rndr-a2-rereview-OgFoQ8/audit.json /tmp/rndr-a2-rereview-OgFoQ8/all-build-artifacts.sha256
ls crates/rndr_ui/tests
cat crates/rndr_ui/tests/complete_gallery.rs crates/rndr_ui/README.md
git log -8 --oneline
cat /tmp/rndr-a2-rereview-OgFoQ8/strict-aggregate-clippy.log
du -sh target; df -k .
```

Additional bounded searches inspected current test names/state/geometry/localization anchors and specification headings/numbered gates.
Saved Coder memory was recalled for authorized original scope, accepted04642 and previous independent dispositions;
its stale historical nodes were reconciled against current Git and reports, not treated as new authority.

## Preservation actually performed

An inline Python3 script first asserted exact clean f427, absent destination, and original manifest digest.
For every standard two-space manifest row it read original bytes and asserted SHA256 (267 rows).
Binary classification used Mach-O/ELF magic or archive `!<arch>` header, not filename/size guesses.
It created `evidence/a2-gate-reconciliation-20261101/independent`, copied238 nonbinary payloads with `shutil.copy2`,
copied original MANIFEST.sha256 unmodified, and wrote ARCHIVE-INVENTORY.tsv/PRESERVATION.json.
29 binary payloads were **not** copied, removed or modified. All relative paths and full original locations are in the inventory.
The resulting238+29 split and every exact byte/hash can be independently checked using the retained validator below.
No external/network operation occurred; “external” here means local /tmp files outside the repository.

New report/matrix/current commands/read-only validator were authored, then only new current-status sections appended to
A0-A1-checkpoint, implementation-tracking and A2-components. Git diff is the exact record of those doc edits;
canonical Specification/Guide/API, every implementation/test/dependency/golden and historical evidence stay unchanged.

## Integrity and final seal commands

From repository root (local reads only):

```sh
python3 evidence/a2-gate-reconciliation-20261101/verify-preservation.py --external
(cd /tmp/rndr-a2-rereview-OgFoQ8 && shasum -a 256 -c MANIFEST.sha256)
# Final validation script also compares all parent-tracked paths with Git blobs,
# permits changes only to the three explicit tracking docs and new evidence/matrix,
# checks canonical digests, new links, binary exclusion and changed-path inventory.
python3 evidence/a2-gate-reconciliation-20261101/verify-doc-only.py
# Root evidence manifest creation: sorted regular files relative to evidence directory,
# SHA256 + two spaces + path, excluding only MANIFEST.sha256 itself.
(cd evidence/a2-gate-reconciliation-20261101 && shasum -a 256 -c MANIFEST.sha256)
# Stage ONLY the named docs and new directory; commit after staged boundary check:
git add docs/design/A0-A1-checkpoint.md docs/design/A2-components.md docs/design/implementation-tracking.md docs/design/A2-remaining-gates.md evidence/a2-gate-reconciliation-20261101
git diff --cached --name-only
# Inspect evidence text diff separately: preserved logs may contain historic whitespace.
git diff --cached --check -- docs/design
# After successful verification:
git commit -m 'docs(ui): preserve independent R1 evidence and reconcile remaining A2 gates'
git log -1 --format='%H %P'
git status --short
git diff HEAD^ HEAD -- crates Cargo.toml Cargo.lock
```

The new evidence root manifest is generated last after verification receipts; post-commit validation is read-only and
its stdout/commit identity is delivered to sender, not amended into a self-referential manifest/commit.
Original independent runners are preserved as historical source, not rerun, and not turnkey relocatable commands.
For native instructions see the matrix; **instructions only, no native execution in this task**.
