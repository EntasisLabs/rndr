# A2 evidence preservation and remaining-gate reconciliation

## Outcome and authority

**COMPLETE for this bounded documentation/evidence deliverable; NOT terminal A2/project success.**
The sender accepted independent scoped HEADLESS R1 review of clean parent
`f427b7474dad99a983115edfad14c5084c9cb253`. Previous P2 is resolved.
Full A2 acceptance, native checks and integration remain open to sender/owner.
This record is authored by the reconciler, not another independent reviewer; no identical R1 review or expensive build was repeated.

Read complete canonical Specification(339 lines), Review Guide(70), A2-components(112), A1-api-decisions,
A0-A1-checkpoint(147), implementation tracking(80), latest independent REVIEW(99)/COMMANDS(113),
both earlier independent A2 reports, crate font/ownership documentation and saved authorized-goal/A1/A2 review memory.
Bounded follow-up reads recovered overlarge reads. Canonical proposals are unchanged:
Specification SHA256 `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b`;
Guide `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`.

The [requirement-by-requirement matrix](../../docs/design/A2-remaining-gates.md) covers Specification §4 A2.1–7,
§8 evidence1–9, cross-cutting §§2–7/Review Guide, previous reviews, saved goals, remaining defects/risks,
manual platform cells, unavailable capabilities/authorization and executable next actions.

## Independent provenance and preservation

Original independent directory `/tmp/rndr-a2-rereview-OgFoQ8` remains untouched.
Reviewer session `ses_peer_d1737b092b7f20e04afee355b12757431811473f7d9e447a840b8a1589205231`,
assignment `2a057eb53b9ddae6099e66536ef6d7dc06775d630e940c0d35f5b3a9f4e667cf`, attempt `att-18e3ea0698b05e2041db6219c3e26564`.
Tested implementation `91c008bdc1054f42f67dcd5d45729290ddb4ffa4`; evidence-only reviewed child f427 as above.
Complete unedited [independent REVIEW](independent/REVIEW.md) and [COMMANDS](independent/COMMANDS.md) are retained.
SHA256 REVIEW `86e24e37274b9ef50ef51bf70375f29411d4de92282eed614bfb6d03a9203eee`;
COMMANDS `dce7f59c6dac1615b339044be6bddf6f714c3e88dcb312bd350a763bf43b4a9e`.
Original standard267-row manifest SHA256 `5732aaa2c54ee57d4d10fd6d66554d5593393c99c1bc43b9ec678a1a552fe063`
is copied byte-for-byte, not rewritten to disguise omitted binaries.

All267 payloads were rehashed before copying. **238 payload files /4,291,811 bytes** retained exactly,
including scripts, successful/failing logs, negative controls, original/final fixtures, source/blob/toolchain identities,
diffs, gallery outputs, reuse/dependency identities, build transcripts and prior review inputs.
**29 compiled Mach-O/archive binaries /152,586,704 bytes are external only**, not committed.
`ARCHIVE-INVENTORY.tsv` records every payload SHA256, byte size, original absolute path and retained copy or external classification.
`PRESERVATION.json` summarizes the split. Binary hashes/identities and external dependency/rlib paths in original receipts remain intact.
Original267-row manifest cannot be blindly checked from the retained subtree:29 binaries intentionally are not present there.
Use `verify-preservation.py` for retained validation and `--external` to additionally hash originals, or original shasum from /tmp while available.
New root `MANIFEST.sha256` is standard two-space syntax and covers every new evidence file except itself.

External /tmp binaries are ephemeral; durable hashes are not durable executable bytes. If removed later, retained scripts/source identities
support reproduction subject to recorded toolchain/dependencies and future authorized resource availability, not a guarantee of turnkey binary recovery.
Archived runner scripts contain original absolute paths and some historical runners overwrite outputs; do NOT run them in the archive.
Any future reproduction must be inspected/adapted to a new nonexistent output directory and separately authorized.

## What was verified versus carried forward

This reconciler verified manifest integrity/copy equality, scoped documentation status, exact Git parent and unchanged implementation/history,
relative links, standard new manifest and doc-only delta. Native windows/builds/tests were NOT rerun here.
The following are independent review evidence carried forward with explicit provenance, not fresh reconciler executions:

- Fresh all-source UI41/direct build and headless examples; identity-verified reuse core411/paint62/adapter2 and conformance13.
- Prior adversarial5 and geometry1/64 configurations; unchanged fresh probes baseline2/4→final4/4;
  identical new permanent fixture baseline0/4→final4/4; diagnostic175 configurations13→0 printed mismatches (not175 tests).
- New independent adversarial5 and cost1 pass; Cargo548 is overlapping coverage, not additional unique tests.
- Original32 tests unchanged; corrected later5 boundary stimuli justified by actual text geometry and preserved obsolete failure logs.
- Operating17 files equal starting/worker/two fresh runs; passive6 unchanged; historical961 blobs and234 source blobs identity checked by reviewer.

Known caveats preserved: whole-world layout/text passes100 unrelated-label calls→200/300; native responsiveness unverified.
Strict UI clippy and format passed; strict aggregate core all-target lint fails two pre-existing test warnings at a1_wrapped_hug.rs79/125.
No new implementation/review defect established. This narrow conclusion is not exhaustive full-A2 acceptance.

## Remaining gate / blocker

No additional uncovered headless requirement requiring a fresh build was identified from the canonical reconciliation.
All remaining native cells require capability/authorization absent from this no-launch task: interactive OS window/input,
VoiceOver/Narrator/real bridge, preferences, native fonts/shaping/IME, physical DPI and separate Windows/X11/Wayland targets.
The concrete next step is owner-admitted macOS synthetic-gallery keyboard/pointer/resize/responsiveness,
then AT/IME/font/DPI checks, using the exact command and checklist in the matrix. No native launch/grant was requested or performed.
Owner can assess bounded A2 scaffold scope with explicit native gaps per Specification outcome labels;
this task does not make that decision or silently equate native parity with scoped R1 acceptance.

## Resource and mutation boundary

Preflight2,459,952 KiB available (~2.35 GiB); existing target1.3 GiB. Only ~4.1MiB textual evidence plus small docs/receipts added;
no build artifacts, binary copies, second Cargo target, cleanup, network, source/test/dependency/golden edits,
native launch, approval, push, merge, deployment or integration.
Exact parent is f427 above. Exact new commit is recorded by the post-commit receipt delivered to sender
(commit cannot truthfully include its own hash); use `git log -1 --format='%H %P'` and compare tree identities.
Final seal verification and changed-path inventory are retained beside this report; sender retains review/lifecycle decisions.
