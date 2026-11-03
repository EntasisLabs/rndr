#!/usr/bin/env bash
# Baseline failure characterization, NOT a passing component verification gate.
# Fresh output only; never overwrites a caller test or historical evidence.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target}"
source scripts/data-gallery-env.sh
out="${1:?Supply a NEW output directory}"
if [ -e "$out" ]; then echo "Refusing existing output: $out" >&2; exit 2; fi
free_k=$(df -Pk . | awk 'NR==2 {print $4}')
if [ "$free_k" -lt 2097152 ]; then echo 'Less than 2 GiB free; stop without cleanup.' >&2; exit 2; fi
fixture=crates/rndr_ui/tests/property_editor_prerequisites.rs
if [ -e "$fixture" ]; then echo "Refusing existing fixture: $fixture" >&2; exit 2; fi
mkdir -p "$out"
cp evidence/data-components-next/blockers/baseline-probes.rs "$fixture"
trap 'rm -f "$fixture"' EXIT
python3 - "$out" <<'PY'
import hashlib, json, os, pathlib, resource, subprocess, sys, time
out = pathlib.Path(sys.argv[1]); root = pathlib.Path.cwd()
fixture = 'crates/rndr_ui/tests/property_editor_prerequisites.rs'
def run(command, log):
    started=time.monotonic()
    with (out/log).open('w') as f:
        f.write('$ '+repr(command)+'\n'); f.flush()
        r=subprocess.run(command, stdout=f, stderr=subprocess.STDOUT)
        f.write(f'\nEXIT={r.returncode}\nELAPSED_SECONDS={time.monotonic()-started:.6f}\n')
    return r.returncode
sources=subprocess.check_output(['git','ls-files','crates/rndr_core/src','crates/rndr_ui/src','crates/rndr_paint/src','Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/rndr_core/Cargo.toml','crates/rndr_ui/Cargo.toml','crates/rndr_paint/Cargo.toml'],text=True).splitlines()+[fixture]
manifest={p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest() for p in sources}
(out/'source.sha256.json').write_text(json.dumps(manifest,indent=2)+'\n')
command=['cargo','test','--offline','--locked','-p','rndr_ui','--test','property_editor_prerequisites','--no-run','--message-format=json']
started=time.monotonic()
with (out/'build.jsonl').open('w') as stdout, (out/'build.log').open('w') as stderr:
    stderr.write('$ '+repr(command)+'\n'); stderr.flush()
    r=subprocess.run(command,stdout=stdout,stderr=stderr)
    stderr.write(f'\nEXIT={r.returncode}\nELAPSED_SECONDS={time.monotonic()-started:.6f}\n')
if r.returncode: raise SystemExit(r.returncode)
artifacts=[json.loads(line) for line in (out/'build.jsonl').read_text().splitlines() if line.startswith('{')]
artifacts=[v for v in artifacts if v.get('reason')=='compiler-artifact' and v.get('target',{}).get('name')=='property_editor_prerequisites' and v.get('executable')]
assert len(artifacts)==1, artifacts
binary=pathlib.Path(artifacts[0]['executable'])
run_command=[str(binary),'--ignored','--nocapture','--test-threads=1']
exit_code=run(run_command,'probes.log')
text=(out/'probes.log').read_text()
expected=['enter_snapshot_precedes_later_typing_in_same_native_batch','two_enter_boundaries_keep_distinct_snapshots','repeated_enter_is_not_a_second_commit_request','text_on_each_side_of_tab_goes_only_to_its_chronological_target','capture_field_can_reach_canonical_escape_hook']
reproduced=exit_code==101 and '0 passed; 5 failed;' in text and all('    '+name+'\n' in text for name in expected)
receipt={'head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'product_baseline':'57c30a3242efcea7d54a6083251575ac04f1c603','cwd':str(root),'binary':str(binary),'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'binary_bytes':binary.stat().st_size,'source_manifest':'source.sha256.json','source_manifest_sha256':hashlib.sha256((out/'source.sha256.json').read_bytes()).hexdigest(),'command':run_command,'acceptance_exit':exit_code,'known_five_failures_reproduced':reproduced,'max_child_rss_bytes_macos':resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss,'environment':{k:v for k,v in os.environ.items() if k in ['CARGO_TARGET_DIR','CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG']}}
(out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps({'known_failures_reproduced':reproduced,'acceptance_exit':exit_code}))
if not reproduced: raise SystemExit(1)
PY
