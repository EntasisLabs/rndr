#!/usr/bin/env bash
# Real native readback of synthetic presets; no claim of physical input or AT.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/data-gallery-env.sh
out="${1:?Supply a NEW native output directory}"
[ ! -e "$out" ] || { echo "Refusing existing output: $out" >&2; exit 2; }
[ "$(df -Pk . | awk 'NR==2 {print $4}')" -ge 2097152 ] || { echo 'Less than 2 GiB free; stop without cleanup.' >&2; exit 2; }
mkdir -p "$out"
cargo build --offline --locked -p rndr --example data_editors_gallery --features observe > "$out/build.log" 2>&1
binary="$CARGO_TARGET_DIR/debug/examples/data_editors_gallery"
python3 - "$out" "$binary" <<'PY'
import hashlib,json,pathlib,subprocess,sys,struct
out=pathlib.Path(sys.argv[1]);binary=pathlib.Path(sys.argv[2]).resolve()
paths=subprocess.check_output(['git','ls-files','crates/rndr_core/src','crates/rndr_ui/src','crates/rndr_paint/src','crates/rndr_render/src','crates/rndr/examples','crates/rndr_ui/examples','Cargo.toml','Cargo.lock','rust-toolchain.toml','crates/rndr/Cargo.toml','crates/rndr_ui/Cargo.toml','crates/rndr_core/Cargo.toml','crates/rndr_render/Cargo.toml'],text=True).splitlines()
paths+=['crates/rndr_ui/src/data.rs','crates/rndr_ui/examples/support/data_editors_gallery.rs','crates/rndr/examples/data_editors_gallery.rs','docs/data-editors.md','scripts/data-gallery-env.sh','scripts/capture-data-editors.sh','tools/observe_client.py','tools/validate_observation_native.py']
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
manifest={p:sha(p) for p in sorted(set(paths))}
(out/'source.sha256.json').write_text(json.dumps(manifest,indent=2)+'\n')
receipt={'head_before_candidate_commit':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary':str(binary),'binary_sha256':sha(binary),'source_manifest_sha256':sha(out/'source.sha256.json'),'origin':'Native macOS GPU pre-present surface copy; synthetic preset model/focus/scroll, not human input or after-present/compositor confirmation.','runs':[]}
presets=[('light',[]),('dark',['--dark']),('narrow',['--narrow']),('narrow-inspector',['--narrow','--inspector-scroll']),('compact-hc-200',['--compact','--hc','--200']),('compact-hc-200-inspector',['--compact','--hc','--200','--inspector-scroll']),('compact-hc-200-bottom',['--compact','--hc','--200','--inspector-scroll','--inspector-bottom']),('invalid',['--invalid']),('readonly',['--readonly']),('disabled',['--disabled']),('empty',['--empty']),('workflow-bottom',['--inspector-bottom'])]
for name,args in presets:
    assert sha(binary)==receipt['binary_sha256'],'Binary changed during matrix'
    command=['python3','tools/validate_observation_native.py',str(binary),str(out/name)]+args
    with (out/(name+'.log')).open('w') as log:
        log.write('$ '+repr(command)+'\n');log.flush();r=subprocess.run(command,stdout=log,stderr=subprocess.STDOUT);log.write(f'\nEXIT={r.returncode}\n')
    entry={'name':name,'command':command,'exit':r.returncode};receipt['runs'].append(entry)
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if r.returncode:raise SystemExit(r.returncode)
    session=json.loads((out/name/'result.json').read_text())
    assert session['binary_sha256']==receipt['binary_sha256'] and session['status']=='captured' and session['normal_shutdown_cleanup'],session
    # This repository's native observation contract is explicitly BEFORE
    # presentation. Do not upgrade that to compositor/after-present proof.
    entry['presented_confirmation']=session['presented_confirmation']
    entry['frames']=[]
    for index in [1,2]:
        metadata=out/name/f'frame-{index}.json';png=out/name/f'frame-{index}.png'
        m=json.loads(metadata.read_text());data=png.read_bytes()
        assert m['source']=='live-surface-copy-before-present' and not m['presented_confirmation'],m
        assert m['png_bytes']==len(data) and list(struct.unpack('>II',data[16:24]))==[m['width'],m['height']]
        assert m['render_provenance']['backend']=='Metal' and m['render_provenance']['text_engine']=='parley'
        entry['frames'].append({'revision':m['frame_revision'],'png_sha256':sha(png),'metadata_sha256':sha(metadata),'source':m['source']})
    assert int(entry['frames'][1]['revision'])>int(entry['frames'][0]['revision'])
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print('PASS native '+name,flush=True)
assert all(sha(p)==v for p,v in manifest.items()),'Source changed during matrix'
PY
