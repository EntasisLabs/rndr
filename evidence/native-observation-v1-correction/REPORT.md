# Native observation v1 — targeted correction report

## Disposition

All three review findings are reproduced and corrected in implementation commit **`57458a7a8b1b164ac4959ce1226b7a19c01c2e0b`**, directly on review-only commit `28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7`. This is a correction candidate for **fresh independent re-review**, not approval, acceptance or integration. No styling, actions endpoint, Blooey/Medousa adoption, push/deploy/spend or new grants.

Implementation diff: **9 files, 601 insertions, 33 deletions**. Five Rust implementation/test files, native-observation documentation, the client, its existing fixture-mode correction, and a new permanent client regression file. Prior implementation remains in history; original native and independent-review evidence directories are byte-for-byte preserved. UI/design/unrelated paths are unchanged. The subsequent evidence-only seal changes only this new directory; its exact commit is returned in the handback.

## Reproduction BEFORE editing

Read the complete pinned `REVIEW.md` and ran its supplied executable reproductions on clean `28a0ae30`:

- `baseline-client.log`: FIFO descriptor with `timeout=.05` hangs until the reviewer watchdog kills it at approximately .809s; preexisting JSON collision leaves a newly created PNG beside untouched old JSON while raising an exception.
- `baseline-rust.log`: exact observer source plus fresh paint compilation, 18 original tests and three reviewer probes pass. The deliberately bug-asserting probe confirms builtin shaping happened, then a Render-stage custom-engine replacement is accepted and labelled custom. Passing that reproduction means the defect exists, not that behavior is acceptable.

**No Parley corruption was reproduced.** That distinction applies to both this baseline probe and the new synthetic tests below.

## Corrections

### F1 — paint-time engine/resource coherence

`Capture` retains the actual paint-boundary engine Arc, runtime-kind provenance and epoch tuple. Keeping the Arc also prevents pointer-address recycling from disguising replacement. Validation checks identity, not just kind/name, and checks UV/configuration epoch. Removal, same-kind replacement, custom replacement and versioned mutable updates deny both channels with `post_paint_text_engine_mutation`.

Builtin font registration, fallback changes and default-face changes now bump `cache_epoch` and invalidate affected cached lines; unchanged face synchronization and unchanged fallback lists do not bump it. Parley registration and physical-scale changes are versioned alongside existing font-byte/fallback/UV invalidations. A final resource audit also identified that Render could consume pending Parley uploads without moving UVs: upload consumption now has a **separate generation**, checked in the observation epoch tuple. Normal runner uploads do not force all label layouts to be invalidated each frame.

The runner validates a pending capture **before atlas upload and submission**. An invalidated requested frame is not uploaded/submitted and returns failure, retaining input edges for the next frame. Successful provenance comes from `Capture`, not the post-Render World. No second paint/update/export reconstruction is added. No-request/default-off scheduling and authorization remain unchanged.

Permanent tests reproduce the review's actual-paint builtin→custom replacement, same-kind builtin replacement, mutable custom epoch, builtin fallback/font/TextFace changes, unchanged builtin/custom paths, and (with Parley enabled) unchanged Parley plus fallback/registration/font-byte/scale/upload-consumption changes. All are synthetic core/protocol tests, not live atlas-corruption claims.

### F2 — bounded, checked descriptor fd

One absolute deadline starts before descriptor processing and supplies the remaining connect/send/receive budget. The client pins an opened directory fd, checks current ownership and mode 0700, and opens the final descriptor with `O_NOFOLLOW | O_NONBLOCK`. It verifies the **opened fd** is a current-user-owned regular file mode 0600 before reading. The read is capped at 4097 bytes (rejecting more than 4096), with initial/final size and timestamps checked for detectable changes. FIFO/nonregular files, invalid JSON/object/types, symlink substitutions and oversized/growing descriptors fail locally. No additional transport or permission is admitted.

Python cannot cancel an arbitrarily stalled regular-file filesystem/kernel syscall: O_NONBLOCK prevents FIFO-open waits, not pathological regular-filesystem stalls. Budget is checked before/after descriptor processing and before socket work; an expired descriptor result is never admitted. The descriptor/protocol is still not a sandbox against a hostile same-UID/root process. Directory ancestors/socket connection are ordinary local path operations under that same account boundary.

### F3 — no-clobber outputs with failure rollback

Serialize metadata and completely stage both private files in a newly created private temporary directory before publishing either final name. Hard-link publication refuses to overwrite existing PNG/JSON or symlinks, including raced collisions. On failure, rollback considers only names published by this invocation and unlinks only an observed matching inode. Open fds hold those inodes through rollback, preventing inode reuse from creating an ABA identity match. Preexisting files and observed replacements are preserved.

**Not a crash-atomic two-file transaction.** A reader can observe the first final name before the second; SIGKILL/crash can leave a partial pair. Rollback is best effort if filesystem cleanup itself fails. POSIX does not provide atomic compare-inode-and-unlink: arbitrary external path mutation between the identity check and unlink is not isolated. Use a private output directory; do not treat this as a sandbox against hostile same-UID mutation. Cooperating concurrent callers cannot clobber each other. Consume output pairs only after successful return, never assume globbed files from a failed call are coherent. These limits are in the client docstring and public documentation, not hidden behind an atomicity claim.

Tests cover preexisting PNG and JSON, actual second-write failure, second-publication failure, raced JSON collision, observed PNG replacement during rollback, existing output symlinks and concurrent same-prefix publishers.

## Final verification (overlapping suites, NOT summed as unique tests)

| Command/result | Final evidence |
|---|---|
| Affected core/paint/UI/renderer minimal-observe suite: **576 passed**, 80 result groups, no failures/ignored | `suites-final.log` |
| Renderer default features + observe, including Parley: **69 passed** | `parley-default-final.log` |
| Renderer observe-disabled unit suite: **22 passed** | `disabled-final.log` |
| Python original + correction regressions: **20 passed** (8 original + 12 new) | `python-sealed.log` |
| Default gallery check without/with observe: both exit 0 | `default-disabled-final.log`, `default-observe-final.log` |
| Bounded native minimal-observe build: exit 0, changed renderer and gallery rustc/link commands retained | `native-build-final.log` |
| Direct exact observer + freshly compiled paint: **22 passed** (18 original + 4 non-Parley corrections) | `run_source_regressions.py`, `source-regressions-sealed.log` |
| Three new permanent Rust regressions against original observer: each fails as expected with exit 101 | same source replay log |
| Two final fresh native processes, four new PNG/tree pairs, auth/stale rejection and normal cleanup | `native-final-{light,narrow}/`, matching `.log` files |
| Original 111 payload hashes + reviewer 30 payload hashes preserved; final PNG CRC/decompression/unfilter/tree, pixel comparison and source hashes pass | `check_correction.py`, `integrity-final.log`, `source-inputs.sha256` |

The direct replay reuses pinned dependency rlibs, including Cargo-rebuilt core; it is not a fresh rebuild of every dependency. For the baseline red replay only, the original observer source is read using `git show`; current core is deliberately retained, and an unavailable paint-kind accessor in the **unexecuted unchanged-path test** is replaced with a compilation-only string. Each of the three executed baseline red tests fails at validation accepting the invalidated capture. Production source is not modified by this harness. Final replay rewrites copied test references to execute the freshly compiled paint module. Actual copied-observer hashes and compiler commands are logged. Cargo independently rebuilt changed production sources; this is not reliance solely on a reused test executable.

Intermediate logs/captures are retained, not substituted for final results. `focused-rust.log` records initial syntax/fixture compilation failures corrected before passing tests; `focused-rust-2.log` is the earlier passing focused run. Earlier source replay logs preceded the final fresh-paint linkage correction and/or upload-generation addition; only `source-regressions-sealed.log` is the final replay. Earlier `native-light/narrow` captures used intermediate binary `3b2e6d...`; only `native-final-*` uses the sealed final binary below.

## Final native identity and limits

- Original/reviewer native executable SHA256: `b96be3081726d73afb683466fa10e02555370c3ebb159311e8fa900c025c9263`. Its old captures remain old evidence, unchanged.
- **Fresh final executable SHA256:** `ce544034520d1a8cc6d9af538ffa0f3502d0438303f84240e55e8813f3e44e09`.
- Final build: `cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery`; changed production sources were compiled and the example linked. Rust/Cargo identities and source hashes are saved. Binary is not committed as a large artifact.
- Both final processes: Metal / Apple M4 / Bgra8UnormSrgb / AutoVsync / alpha Auto with opaque clear / quads / builtin_atlas / parley feature false / physical scale 1. Light 1000×700, narrow/dark/200%-text/long 320×700, 28 semantic nodes, two revisions per process. Each records startup surface-unavailable denial, then fresh success, bad-auth and future-revision denial, normal exit and exact-session cleanup.
- Four final images fully decode and match corresponding old native **raw pixels**: light `7921fa8dc92de1336b06eca2e8c79773fd48a99efd5113c289a83121991427d3`, narrow `c704670129dac30925b25e51d15ed038b4e44696aa9b13d3762d0799ecd3b209`. This is structural/pixel equality, not visual legibility or polish acceptance.

The provenance remains `source: live-surface-copy-before-present`, **`presented_confirmation: false`**. No compositor/OS-presented confirmation is inferred. Live scene/Parley/custom rendering, actual atlas transitions/corruption, arbitrary unversioned custom font/engine interiors, concurrent engine mutation through submission, shaping/IME, physical DPI/multi-monitor/resize, native interaction, transparent composition, broader lost/occluded transitions, alternate formats/backends/platforms and OS accessibility remain unverified. All engines must avoid concurrent mutation through paint/upload/submission; custom invalidations must be versioned. These tests cannot certify arbitrary app-supplied Public assertions or unmarked secrets.

## Commands, resource discipline and integrity

Exact principal command sequence and log mapping: `COMMANDS.md`. All Cargo runs were offline and locked with `CARGO_BUILD_JOBS=2`, dev/test debug=0 and incremental=0. Direct rustc work was serial with debuginfo=0. Disk was checked first: about **1.3 GiB** available (not assumed from the prior review); final checks around **0.7 GiB**. No broad artifact deletion, dependency fetch, desktop input/other-app capture, new grants, push/deploy or spend occurred. Only authorized synthetic gallery processes were launched.

Original native manifest SHA256 remains `9034ec3ec28db8aa007ccd6faef65ba5139368e9fb0a549e5a4cbe7d2360b30a` (111 payloads); review manifest remains `12e9fda99c12d431fca79acc745595800d647556ca80401d8dfc46337de89110` (30 payloads). This directory's `MANIFEST.sha256` covers every payload except itself. Raw compiler/test logs retain original whitespace, including blank lines; scoped source diff whitespace checks pass. Implementation source hashes are exact and independently rechecked after the implementation commit.

**Next step belongs to sender:** arrange fresh independent re-review of the exact correction/evidence commits before any acceptance or integration. This worker has not approved or integrated anything.
