"""Capture editor states, validate layout and contrast, and compare Metal goldens."""
import argparse
import itertools
import json
import pathlib
import runpy
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
GOLD = ROOT / 'tests/goldens/editor'
AXES = [('', '--dark'), ('', '--hc'), ('', '--compact'), ('', '--200'), ('', '--narrow')]


def compare(actual, expected):
    if actual.read_bytes() == expected.read_bytes():
        return
    decode = runpy.run_path(str(ROOT / 'scripts/test-component-capture.py'))['png_rgba']
    w, h, pixels = decode(actual)
    rw, rh, reference = decode(expected)
    assert (w, h) == (rw, rh), expected.name
    bad = sum(any(abs(a-b) > 6 for a, b in zip(pixels[i:i+4], reference[i:i+4]))
              for i in range(0, len(pixels), 4))
    assert bad <= w*h*0.0001, (expected.name, bad)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--update', action='store_true')
    parser.add_argument('--binary', type=pathlib.Path, default=ROOT/'target/debug/examples/design_language')
    args = parser.parse_args()
    names = set()
    with tempfile.TemporaryDirectory() as temp:
        for flags in itertools.product(*AXES):
            name = '-'.join(f[2:] for f in flags if f) or 'default'
            names.add(name)
            prefix = pathlib.Path(temp)/name
            subprocess.run([args.binary, '--probe', '--capture-prefix', prefix, *filter(None, flags)], check=True)
            nodes = json.loads(prefix.with_suffix('.json').read_text())['nodes']
            assert len(nodes) > 30, name
            for n in nodes:
                bounds, clip = n['bounds_points'], n['clip_points']
                if not n['hidden'] and max(bounds[2:]) > 0:
                    assert min(bounds[2:]) >= 2 and all(abs(a-b) < 1 for a,b in zip(bounds, clip)), (name, n)
            if not args.update:
                compare(prefix.with_suffix('.png'), GOLD/(name+'.png'))
        # Don't partially replace the references if a later fixture is broken.
        if args.update:
            GOLD.mkdir(parents=True, exist_ok=True)
            for name in names:
                shutil.copyfile(pathlib.Path(temp)/(name+'.png'), GOLD/(name+'.png'))
    assert {p.stem for p in GOLD.glob('*.png')} == names, 'unexpected golden matrix'
    print(f'{len(names)} gallery captures passed contrast, geometry and ' + ('reference update' if args.update else 'pixel comparison'))


if __name__ == '__main__':
    main()
