#!/bin/sh
# Scoped F3 correction reruns only; reuses target/, never cleans or broad-rebuilds.
set -u
export CARGO_INCREMENTAL=0
out=docs/reports/f3-refusal-evidence
mkdir -p "$out"
: > "$out/gates.tsv"
failed=0
run() {
    name=$1
    shift
    printf 'RUN %s: %s\n' "$name" "$*"
    "$@" > "$out/$name.log" 2>&1
    result=$?
    # Only trailing empty lines are normalized for checked-in text evidence.
    python3 -c 'import pathlib,sys; p=pathlib.Path(sys.argv[1]); b=p.read_bytes(); p.write_bytes(b.rstrip(b"\n")+b"\n" if b else b)' "$out/$name.log"
    if [ "$?" != 0 ]; then failed=1; fi
    printf '%s\t%s\t%s\n' "$name" "$result" "$*" >> "$out/gates.tsv"
    printf 'RESULT %s: %s\n' "$name" "$result"
    if [ "$result" != 0 ]; then failed=1; fi
}
run environment sh -c 'date -u; git rev-parse HEAD; git rev-parse main; df -h .; rustc --version; cargo --version; printf "CARGO_INCREMENTAL=%s\n" "$CARGO_INCREMENTAL"; shasum -a 256 crates/rndr_flow/src/*.rs crates/rndr_flow/tests/*.rs'
run flow-test cargo test --locked -p rndr-flow --no-default-features
run budget-proof cargo test --locked -p rndr-flow --no-default-features budget_tests -- --nocapture
run reviewer-probes cargo test --locked -p rndr-flow --no-default-features --test reviewer_probes -- --nocapture
run cycles cargo test --locked -p rndr-flow --no-default-features one_hundred_lifecycle_cycles -- --nocapture
run headless cargo build --locked -p rndr-flow --no-default-features
run dependency-tree cargo tree --locked -p rndr-flow --no-default-features -e normal,build
run dependency-inverse cargo tree --locked -i rndr-flow
run fmt cargo fmt --all --check
run flow-clippy cargo clippy --locked -p rndr-flow --no-default-features --all-targets -- -D warnings
run feature-off cargo check --locked -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
run editing-off cargo test --locked -p rndr --no-default-features --test editing_compatibility
run editing-ui cargo test --locked -p rndr --no-default-features --features ui --test editing_compatibility
run editing-liquid cargo test --locked -p rndr --no-default-features --features liquid --test editing_compatibility
run flow-docs env RUSTDOCFLAGS=-Dwarnings cargo doc --locked -p rndr-flow --no-default-features --no-deps
run cycle-audit python3 -c 'import pathlib,re; text=pathlib.Path("docs/reports/f3-refusal-evidence/cycles.log").read_text(); rows=re.findall(r"F3 cycle=(\d+) scopes=1 cleanups=0 leases=0 active_leases=0 pending_scopes=0 pending_cleanups=0 queued_commands=0 callbacks=(\d+)", text); assert rows==[(str(n),str(3*n)) for n in range(1,101)], rows; print("100/100 exact baseline vectors; callbacks=300; cycle test also asserts final root reclamation to Counts::default()")'
run graph-audit python3 -c 'import pathlib; p=pathlib.Path("docs/reports/f3-refusal-evidence"); tree=(p/"dependency-tree.log").read_text(); inverse=(p/"dependency-inverse.log").read_text(); assert all(x not in tree for x in ["rndr_ui ","rndr_liquid", "rndr_render ", "tokio "]); assert len(inverse.strip().splitlines())==1; print("Headless normal/build graph: feature-off core, no UI/render/Tokio, no reverse adopter")'
run original-evidence git diff --exit-code fa5a03b76975068e015cc5f373baadaff1b703fe -- docs/reports/f3-evidence
run diff-check git diff --check
cat "$out/gates.tsv"
exit "$failed"
