#!/usr/bin/env python3
"""Read-only validation. No build, launch, mutation or external access beyond recorded local files."""
import argparse
import csv
import hashlib
import pathlib

parser = argparse.ArgumentParser()
parser.add_argument('--external', action='store_true', help='also hash original local /tmp payloads')
args = parser.parse_args()
base = pathlib.Path(__file__).resolve().parent
manifest = base / 'independent/MANIFEST.sha256'
assert hashlib.sha256(manifest.read_bytes()).hexdigest() == '5732aaa2c54ee57d4d10fd6d66554d5593393c99c1bc43b9ec678a1a552fe063'
original = dict(line.split('  ', 1)[::-1] for line in manifest.read_text().splitlines())
rows = list(csv.DictReader((base / 'ARCHIVE-INVENTORY.tsv').open(), delimiter='\t'))
assert len(rows) == len(original) == 267
retained = external = 0
for row in rows:
    path = pathlib.Path(row['original_absolute_path'])
    rel = str(path.relative_to('/tmp/rndr-a2-rereview-OgFoQ8'))
    assert original[rel] == row['sha256']
    if row['storage'] == 'retained':
        copy = base / row['repository_relative_copy']
        data = copy.read_bytes()
        assert len(data) == int(row['bytes'])
        assert hashlib.sha256(data).hexdigest() == row['sha256'], copy
        retained += 1
    else:
        assert row['storage'] == 'external-binary' and not row['repository_relative_copy']
        assert not (base / 'independent' / rel).exists()
        external += 1
    if args.external:
        data = path.read_bytes()
        assert len(data) == int(row['bytes'])
        assert hashlib.sha256(data).hexdigest() == row['sha256'], path
assert (retained, external) == (238, 29)
print(f'PASS: original267 identities; retained{retained}; external-binary identities{external}; original payloads hashed={args.external}')
