# Unaccepted experiment — not a component delivery

These files preserve the worker's discarded in-turn prototype. `tracked.patch.gz` (losslessly gzip-compressed to retain exact patch context bytes)
was against baseline 57c30a3242efcea7d54a6083251575ac04f1c603. The two `.rs.txt`
files were new files, not included by that diff. Do not apply this as a candidate.

An earlier version of the optional core `FieldTransactions` seam passed the five
boundary probes; subsequent composition/lifecycle changes were only checked by
`cargo check --offline --locked -p rndr_ui -p rndr_render` before the final
unverified refinements. The files here are the last experiment, **not** the exact
source of a verified build. There is no screenshot, full test result or native
behavior claim for this experiment. Its last refinements were not compiled.

The experiment would need careful canonical lifecycle/blur/IME/read-only
contracts and full regression coverage, rather than merely making five isolated
probes green. It also contains incomplete table/inspector compositions, not
review-ready reusable APIs. Production source was restored to the admitted
baseline before committing the blocker report. No accepted historical evidence
was changed. This preservation is not an endorsement or shortcut around the
owner's independent review.
