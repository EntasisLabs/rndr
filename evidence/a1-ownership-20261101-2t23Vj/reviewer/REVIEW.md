# Independent A0/A1 re-review: REVISE

Reviewed exact HEAD 01da8acdc57240cd536234d062933e232b033eec against baseline 291ab05493a59732f891dd762bf90c64e79a48d0, including correction delta from f7aa9703b7258e5ed546fc8436b3676f2705d9b0. Git status --porcelain empty before and after. Review only, no tracked source/docs changes, approval, A2, integration or push.

## P2: unrelated modified hotkey steals ordinary Space release

crates/rndr_core/src/hotkey/registry.rs:538-544 and 565-568.

With a live Alt+Space registration (backend succeeds, guard has not fired), focus a Button + ReleaseActivation and submit unmodified Space down/up. Expected one intent, actual zero. Reproduces with both edges in one Input frame and in consecutive frames. The identical no-registration controls pass. `control_edge` rejects releases based merely on the base key's membership in `live`, not whether this physical gesture matched/was owned by the registered chord. The plain down survives stroke matching and arms; plain up is converted into a synthetic cancellation marker, so GenericInteraction resolves the arm without activating. Hotkey priority must follow matching chord/active guard ownership, not a registered base key alone. Preserve genuine hotkey/synthetic cancellation and stale-arm protections when repairing. Add permanent same-frame and split-frame positive nonmatching-hotkey regressions alongside existing hotkey-cancellation tests.

External asserting repro: adversarial.rs. Build/run from candidate worktree:

    rustc --edition=2021 --test /tmp/rndr-rereview-01da.3eseqx/adversarial.rs -L dependency=target/debug/deps --extern rndr_core=target/debug/deps/librndr_core-99fc9f1d1448f44b.rlib -C debuginfo=0 -o /tmp/rndr-rereview-01da.3eseqx/adversarial-expanded
    /tmp/rndr-rereview-01da.3eseqx/adversarial-expanded --nocapture

Result exit 101, 2 controls passed / 2 regressions failed. See adversarial-expanded.log. This blocks A1 readiness and A2 clearance; no other blocking finding identified in this review.

## Independent verification, not cached evidence acceptance

All Cargo commands offline --locked; CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0; existing worktree target only. Preserved just target/debug/.fingerprint/rndr_core-99fc9f1d1448f44b as core-library-fingerprint-preserved here before cargo test -v. core.log records actual rustc compilation of candidate crates/rndr_core/src/lib.rs and dependent tests, eliminating source snapshot fingerprint ambiguity. No broad cleanup.

Commands, all exit 0:
- cargo test --offline --locked -p rndr_core -v: 402 passed (includes 14 correction tests).
- cargo test --offline --locked -p rndr_paint: 62 passed.
- cargo test --offline --locked -p rndr_ui: 14 passed.
- cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests: 1 passed, 22 filtered.
- cargo check --offline --locked -p rndr --all-targets --no-default-features.
- cargo tree --offline --locked -p rndr_ui --edges normal: GPU-free UI -> core, existing pulldown-cmark/unicode-segmentation only.
- cargo run --offline --locked -p rndr_ui --example passive_gallery -- /tmp/rndr-rereview-01da.3eseqx/gallery; diff -rq against evidence/ui-gallery: six byte-identical files.
- shasum -a 256 -c correction evidence files-sha256.txt, source-docs-sha256.txt, recovery/logs-sha256.txt, ui-gallery-sha256.txt: pass.
- targeted rustfmt --check on registry.rs, interaction.rs, a1_interaction.rs, a1_review_corrections.rs: pass.

Standalone probe.rs and mixed.rs independently compiled with rustc against rebuilt candidate explicit rlib. probe: Tab+Space yields one intent, subsequent unmatched release none. mixed: same-frame Tab+Enter legacy and separate-frame Enter yield Fired("legacy").

Identical final permanent 14-test fixture independently compiled/tested twice via rustc with explicit --extern rlibs. Original source was git archive of exact f7aa9703 core/src into original-source here, compiled directly to original-library/librndr_core.rlib using existing dependency rlibs, without Cargo workspace fingerprint selection or a second target tree. Original: exit 101, 11 failed / 3 guardrails passed; corrected: exit 0, 14 passed. This independently confirms the worker's counterfactual claim. See original-library-build.log, fixture-original.log, fixture-corrected.log. Current source/build left unchanged by original compile.

## Contracts and review scope

Read entire unchanged approved Specification and Review Guide, A1-api-decisions, A0-A1-checkpoint, implementation-tracking, evidence README. Hashes match requested approved docs and evidence manifest 88cd3a42971648bde34f0590effc6b8cca729d3a5b53a2a09553513f358caf3e. Reviewed full baseline source inventory/prerequisites (accessibility, readonly/editor guards, Chrome scheduling, relations, wrapped-Hug layout), passive UI token/style/owned lifecycle and actual paint/gallery assertions, plus correction delta/tests/call sites. No A2 implementation or adoption implied.

FIFO64 request policy is appropriate: each accepted request preserved, same-target multiplicity, no inferred entity/source/frame dedup, no modern->Fired fanout; explicit overflow/no eviction; drain checks generation/visibility/enabled/readonly/action name. Removed AlreadyPending/coalesced are new unaccepted A1 API only; no remaining production matches/callers; pre-A1 public structs remain unchanged. Canonical producer per physical activation and host permission checks remain necessary. Legacy Fired remains single-slot; raw pointer is processed before ordered key events; no cross-device temporal ordering promise. AT modern requests persist until host Update drain; later producers stay pending; bounded queue may overflow explicitly.

Inspected historical baseline/recovery/correction evidence and raw before-expanded/current-artifact-rebuilt/force-current-library/fresh-verified-run provenance. Worker accurately labels snapshot-cache-reuse-invalid, final-run and verified-run as invalid artifacts, not acceptance evidence. Independent rebuild and explicit-rlib counterfactual avoid those collisions. Raw trailing-blank-line whitespace quirk retained, not normalized. Existing evidence/logs unchanged.

Native windows/screenshots, native capture/blur integration, OS AT, preferences, native fonts/shaping and physical DPI remain UNVERIFIED. Headless AccessKit and paint do not certify them. End disk about 3.2 GiB available (began 3.3 GiB). All new outputs confined here and small independent source/library copies; no another large Cargo target, network, secrets or custody workaround.

Disposition: REVISE for P2 above; A1 not ready for owner acceptance/A2 yet. Owner retains acceptance and continuation.
