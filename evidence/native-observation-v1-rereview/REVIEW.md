# Fresh independent re-review — native observation v1 correction

## Verdict: PASS within the documented v1 boundary; ready for sender review

**No remaining acceptance-blocking finding, or new reproduced product defect, was found in this scoped re-review. F1, F2 and F3 are resolved within the expressly documented contracts below.** This is a review verdict only, not approval, acceptance, integration or full native/A2 certification. Sender retains lifecycle responsibility. No fixes, product edits, styling, actions endpoint, Blooey/Medousa changes, push/deploy/spend or new grants were performed.

Exact reviewed coordinates:

- Prior review/base: `28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7`.
- Implementation: `57458a7a8b1b164ac4959ce1226b7a19c01c2e0b`, direct child of that base.
- Evidence-only candidate HEAD: `dc470507c5e65977ce34bbe6a6a9e4f1376f7983`, direct child of implementation; clean at entry.
- Complete scoped implementation diff inspected: nine files, +601/-33. Checked surrounding paint/update/Render, label/editor epoch consumers, builtin face synchronization, Parley invalidation/upload, runner upload/submission/provenance, client transport/output and privacy/default-off contracts. Read prior REVIEW, correction REPORT/COMMANDS, original and corrected observation contract, permanent regression additions and replay/checker implementations.
- Entire sealed `implementation.diff` equals the exact base-to-implementation Git diff byte-for-byte. All 63 seal-commit files are additions under the correction evidence directory only. Prior evidence, UI/design and unrelated source remain unchanged.
- Independently hashed **all 111 original + 30 prior-review + 62 correction payloads**, checked manifest coverage (no missing/extra payloads), nine source-input hashes, and ancestry. Correction manifest SHA256 **`9e067e9daad5262a2f3bc59837a55994edba90b75e992e261b269077c08f094e`** is confirmed. Original/prior-review manifest identities remain `9034ec3ec28db8aa007ccd6faef65ba5139368e9fb0a549e5a4cbe7d2360b30a` and `12e9fda99c12d431fca79acc745595800d647556ca80401d8dfc46337de89110`.

## Finding dispositions and adversarial review

### F1 (previous P2 / medium): resolved for sequential versioned engine lifecycle

`crates/rndr_render/src/observe.rs:304-317,414-432,494-502` retains the paint-time Arc (preventing address reuse), runtime kind and epoch tuple, checks Arc identity rather than kind alone, synchronizes builtin TextFace before version comparison, and denies detectable invalidations. `lib.rs:664-680` validates **before** `upload_atlases`/submission; failure returns without submitting the invalidated requested frame. Provenance at `lib.rs:703-710` uses Capture's paint-time kind, not a new World lookup. No-request and feature-disabled paths retain ordinary scheduling; no extra reconstruction/update is introduced.

Builtin registration/replacement, fallback and default-family changes clear cached lines and bump epoch (`rndr_core/src/text.rs:367-377,622-667`). Stable face/fallback identity does not bump epoch. Parley registration/scale invalidations and existing byte/fallback invalidations are versioned. `rndr_render/src/text/mod.rs:278-294` increments a separate pending-upload-consumption generation; the observation tuple checks it before runner uploads. Ordinary upload consumption does not change `cache_epoch`, so it does not itself invalidate every normal label layout each frame.

Fresh permanent tests pass for builtin→custom, builtin→builtin, mutable custom epoch, builtin font/fallback/TextFace, unchanged builtin/custom and Parley unchanged/configuration/upload consumption. New independent actual-paint probes additionally pass for **same custom runtime-type replacement**, replacement of an already-registered font, and three stable builtin frames with identical fallback configuration and stable epoch. Exact-source red replay still rejects the old observer by three expected failures; green replay passes corrected source.

**Limits are material, explicit and sufficient for this v1 contract, not synchronization guarantees.** Arc identity/epoch checks are not locks through GPU submission. All engines must avoid concurrent mutation throughout paint/upload/submission; custom engines/fonts must version invalidating interiors. Arbitrary unversioned custom/font interior mutation, adversarial version reuse, or transient replace-and-restore with hidden side effects cannot be certified by this check. These limits are disclosed in `docs/native-observation.md:71` and TextEngine API documentation. The framework must not be advertised as safe for arbitrary concurrently mutable app engines. Parley test coverage here is synthetic configuration/generation checking; its permanent fixture has no label/glyph population. It is not a live test that drained dirty glyph uploads corrupt a frame. No Parley corruption is claimed reproduced or prevented under arbitrary unsupported concurrency. Live Parley/atlas-transition certification remains open, not silently closed by 69 green tests.

### F2 (previous P3 / low): resolved for ordinary local regular-file descriptors

`tools/observe_client.py:44-76,87-104` starts a shared absolute deadline before preprocessing, pins a directory fd, opens the final directory/descriptor without following symlinks, uses nonblocking open and verifies opened regular-file type, UID and private modes. One read is capped at 4097 bytes; oversized, short, growing or detectably rewritten files fail closed via size/mtime/ctime comparisons. Remaining connect/send/read budgets share that deadline. FIFO, directory, symlink-open substitution and malformed/private-mode tests pass.

Independent probes confirm final-directory symlink denial, same-size rewrite detection, expired descriptor read starts no socket, and path replacement after opening cannot redirect the read to the replacement inode. On this filesystem unlink changes the old inode ctime, so that replacement is conservatively denied. The initial reviewer probe incorrectly required success on the pinned old inode; its failure is retained in `extra-client-initial.log`. The probe assertion was corrected to accept conservative denial or old-inode data, never replacement data. **No product code was changed to make that test pass.**

Not a hard real-time cancellation API: `O_NONBLOCK` does not cancel pathological regular-file/kernel stalls. Final directory components are no-follow; arbitrary ancestors and the later socket path are ordinary local path resolution, not a full openat2-style sandbox. These restrictions and same-UID/root threat boundary are documented; they do not reintroduce the reproduced FIFO timeout bug. The bounded descriptor content/read and rejection of late results are verified, not an absolute wall-clock bound against broken filesystems.

### F3 (previous P3 / low): resolved for normal failures and cooperating publishers

`tools/observe_client.py:128-172` serializes/stages both complete private files before no-clobber hard-link publication. Rollback records only successfully published names, checks device/inode without following links, and holds the created inodes open to avoid ABA reuse. Existing PNG/JSON and symlinks survive collisions; second write/publication failures leave no invocation-owned partial pair; observed replacements survive rollback. Permanent injected-error/race tests pass. Independent stress probes pass **20 rounds of four simultaneous same-prefix publishers**: exactly one coherent PNG/JSON winner per round, no staging leftovers.

This is deliberately **not** a crash-atomic/durable paired-file transaction. Publication has an observable interval between names; crash or rollback-cleanup failure can leave a partial pair. POSIX check-then-unlink cannot isolate arbitrary hostile same-UID replacement in that interval; observed replacements are protected, not every adversarial interleaving. Use a private output directory and consume only after successful return, never infer success by globbing. These are explicit public/docstring limits, adequate for the advertised no-clobber example client rather than a database transaction.

## Fresh verification (counts overlap; do not sum as unique tests)

All rows below are **this review's fresh executions**, not merely inherited reports. Commands/log mapping is in `COMMANDS.md`.

| Check | Result | Evidence |
|---|---|---|
| Complete affected core/paint/UI/renderer minimal-observe suite | **576 passed**, 80 result groups, no failures/ignored | `suites.log` |
| Default-feature renderer + observe (Parley included) | **69 passed** | `parley.log` |
| Observe-disabled renderer unit suite | **22 passed** | `disabled.log` |
| Original + correction Python suites | **20 passed** | `python.log` |
| Correction exact observer/fresh paint source replay | **22 passed**, plus three expected baseline child exits 101; driver exit 0 | `source-replay.log` |
| New independent exact observer/fresh paint harness | **25 passed** = 22 correction/original + three new reviewer tests | `extra-rust.log`, `run_extra_rust.py` |
| Independent client probes | Five groups pass, including 20 four-way publication races | `extra-client.log`, `extra_client.py` |
| Default gallery checks without/with observe | Both exit 0 | `default-disabled.log`, `default-observe.log` |
| Minimal-observe native build validation | Exit 0; **Cargo cache hit**, not reviewer fresh native recompilation | `native-build.log` |
| New actual native processes | Two processes, four captures, bad auth/future revision denial, normal exit/cleanup | `native-{light,narrow}/`, `.log` files |
| Independent seals/diff/source/result parsing | Every payload/hash/count passes | `check_seals.py`, `integrity.log` |
| Independent PNG CRC, decompression, filters, frame/tree and raw-pixel checking | Eight final-author/fresh-review PNGs pass | `check_pixels.py`, `pixels.log` |
| Author's final checker, audited and rerun as supporting check | Exit 0 | `inherited-checker.log` |

Cargo suites/build/checks reused existing local dependencies and compiled artifacts. Direct Rust harnesses **newly compile exact observation source and original paint source**, redirecting only copied test-module paths and test references to the fresh paint module; dependencies (including corrected core) are existing pinned rlibs. They are not independent whole-dependency rebuilds. The baseline red replay keeps current core and substitutes an unexecuted missing accessor solely for compilation; its three actually executed invalidation tests fail on old observer acceptance, as intended. Inspected actual compiler commands/source hashes and final test groups, including Parley consumption test; did not treat summary prose as verification. Author's final native build log contains actual renderer rustc and example compile/link invocations; that compilation is inherited evidence, not this review's fresh compile.

## Native identity, preserved security and remaining gates

Confirmed on disk and in both new process results: binary SHA256 **`ce544034520d1a8cc6d9af538ffa0f3502d0438303f84240e55e8813f3e44e09`**. Fresh processes use that author's final binary; this reviewer did not independently relink/rebuild it. Runtime repetition and exact-source tests strengthen verification but do not attest a fresh independent dependency toolchain.

Both fresh sessions: Metal / Apple M4 / Bgra8UnormSrgb / AutoVsync / alpha Auto / opaque clear / quads / builtin_atlas / parley feature false / device scale 1. Light 1000×700; narrow/dark/200%-text/long 320×700; 28 nodes; revisions 3→4. Each initially denies an unavailable surface then succeeds on new requests, rejects bad auth/future revision, exits 0 and removes its exact session directory. Independent PNG implementation validates chunk CRCs, decompression and all filter types encountered, frame sizes/lengths/revisions/tree references and raw pixels. Light raw SHA256 `7921fa8dc92de1336b06eca2e8c79773fd48a99efd5113c289a83121991427d3`; narrow `c704670129dac30925b25e51d15ed038b4e44696aa9b13d3762d0799ecd3b209`; both equal sealed final captures.

Default-off additive feature, explicit session, Deny by default, exact authentication/private Unix transport, whole-frame sensitive denial (including hidden/unsemantic), revocation and foreign-surface denial remain intact. No interaction/action endpoint was added. New engine checks occur only for a pending capture. GPU code still copies the acquired live surface after ordinary draws in the same encoder/submission in both paths (`gpu.rs:707-712,772-777`), without Exporter fallback. Success remains **`source: live-surface-copy-before-present`, `presented_confirmation: false`**. No OS/compositor visibility, other-app capture or actual displayed-frame proof is inferred.

Open certification gates remain: live scene/Parley/custom rendering and atlas transitions, concurrent/unversioned engine interiors, shaping/IME/font correctness, visual legibility/polish, physical DPI/multimonitor/resize, actual native interaction, transparent desktop composition, broad lost/occluded paths, other formats/backends/platforms and OS accessibility. Application Public assertions cannot detect unmarked secrets. These are disclosed limits, not newly discovered blockers to the scoped correction.

Disk checked **first: 716 MiB**, after tests about **708 MiB**. Every Cargo command offline/locked, jobs=2, dev/test debug=0, incremental=0. Direct rustc serial, debuginfo=0. No broad deletion, dependency download, desktop automation, unrelated app capture, permission expansion or lifecycle action. Only this new review evidence directory is committed. **Ready for sender's scoped consideration; not integrated or approved by this reviewer.**
