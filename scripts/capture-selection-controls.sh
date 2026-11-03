#!/usr/bin/env bash
# Source/binary-bound native synthetic gallery capture; not physical input evidence.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${CARGO_TARGET_DIR:-}" ] || { echo 'Set retained compatible CARGO_TARGET_DIR first.' >&2; exit 2; }
source scripts/data-gallery-env.sh
out="${1:?Supply a NEW native evidence directory}"
[ ! -e "$out" ] || { echo "Refusing existing output: $out" >&2; exit 2; }
[ "$(df -Pk . | awk 'NR==2 {print $4}')" -ge 2097152 ] || { echo 'Less than 2 GiB free; stop without cleanup.' >&2; exit 2; }
mkdir -p "$out"
cargo build --offline --locked -p rndr --example selection_gallery --features observe > "$out/build.log" 2>&1
python3 - "$out" "$CARGO_TARGET_DIR/debug/examples/selection_gallery" <<'PY'
import hashlib,json,pathlib,subprocess,sys,struct,shutil
out=pathlib.Path(sys.argv[1]);binary=pathlib.Path(sys.argv[2]).resolve()
sha=lambda p:hashlib.sha256(pathlib.Path(p).read_bytes()).hexdigest()
paths=subprocess.check_output(['git','ls-files','crates','Cargo.toml','Cargo.lock','rust-toolchain.toml','docs/selection-controls.md','scripts','tools'],text=True).splitlines()
manifest={p:sha(p) for p in sorted(paths) if pathlib.Path(p).is_file()}
assert 'crates/rndr_core/src/popup.rs' in manifest and 'crates/rndr_ui/src/selection.rs' in manifest,'Commit/stage source first'
(out/'source.sha256.json').write_text(json.dumps(manifest,indent=2)+'\n')
receipt={'source_head':subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),'binary':str(binary),'binary_sha256':sha(binary),'source_manifest_sha256':sha(out/'source.sha256.json'),'origin':'Native macOS Metal GPU live-surface-copy-before-present, synthetic preset model/focus; not physical input, after-present/compositor or live AT validation.','runs':[]}
presets=[('light',[]),('dark-single',['--dark','--single-open']),('multi',['--multi-open']),('narrow-multi',['--narrow','--multi-open']),('compact-hc-200',['--compact','--hc','--200','--multi-open']),('invalid-inspector',['--invalid','--inspector-open']),('readonly',['--readonly']),('disabled',['--disabled']),('empty-popup',['--empty','--single-open']),('many-popup',['--many','--multi-open']),('inspector-multi',['--inspector-open']),('narrow-inspector',['--narrow','--inspector-open'])]
for name,args in presets:
    assert shutil.disk_usage('.').free>=2*1024**3,'Less than 2 GiB free; stop without cleanup'
    assert sha(binary)==receipt['binary_sha256'],'Binary changed during matrix'
    cmd=['python3','tools/validate_observation_native.py',str(binary),str(out/name)]+args
    with (out/(name+'.log')).open('w') as log:
        log.write('$ '+repr(cmd)+'\n');log.flush();r=subprocess.run(cmd,stdout=log,stderr=subprocess.STDOUT);log.write(f'\nEXIT={r.returncode}\n')
    entry={'name':name,'command':cmd,'exit':r.returncode};receipt['runs'].append(entry)
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    if r.returncode:raise SystemExit(r.returncode)
    result=json.loads((out/name/'result.json').read_text())
    assert result['binary_sha256']==receipt['binary_sha256'] and result['status']=='captured' and result['normal_shutdown_cleanup'],result
    entry['frames']=[]
    for index in [1,2]:
        metadata=out/name/f'frame-{index}.json';png=out/name/f'frame-{index}.png';m=json.loads(metadata.read_text());data=png.read_bytes()
        assert m['source']=='live-surface-copy-before-present' and not m['presented_confirmation']
        assert m['png_bytes']==len(data) and list(struct.unpack('>II',data[16:24]))==[m['width'],m['height']]
        assert m['render_provenance']['backend']=='Metal' and m['render_provenance']['text_engine']=='parley'
        # Check requested synthetic popup really exists in the captured frame.
        if any(a.endswith('-open') for a in args):assert any(n['role']=='ListBox' for n in m['nodes']),(name,index,'popup absent')
        entry['frames'].append({'revision':m['frame_revision'],'png_sha256':sha(png),'metadata_sha256':sha(metadata),'source':m['source']})
    assert int(entry['frames'][1]['revision'])>int(entry['frames'][0]['revision'])
    (out/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print('PASS native '+name,flush=True)
assert all(sha(p)==v for p,v in manifest.items()),'Source changed during capture matrix'
print('PASS source/binary/frame integrity (appearance requires actual pixel inspection)',flush=True)
PY
