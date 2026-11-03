#!/usr/bin/env python3
"""Inspect decoded synthetic PNG pixels; make explicitly zoomed comparison crops.
No dependencies, network, native input, or capture-policy changes.
Usage: python3 scripts/check-ui-polish-pixels.py evidence/hands-on-polish
"""
import importlib.util
import pathlib
import struct
import sys
import zlib

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("capture_tests", pathlib.Path(__file__).with_name("test-component-capture.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xffffffff)

def write(path, width, height, rgba):
    rows = b"".join(b"\0" + rgba[y*width*4:(y+1)*width*4] for y in range(height))
    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))

def crop(image, rect, zoom=1):
    width, height, rgba = image
    x, y, w, h = rect
    assert x >= 0 and y >= 0 and x+w <= width and y+h <= height
    out = bytearray()
    for row in range(y, y+h):
        pixels = b"".join(bytes(rgba[(row*width+col)*4:(row*width+col+1)*4])*zoom for col in range(x, x+w))
        out.extend(pixels*zoom)
    return w*zoom, h*zoom, out

def compare(root, name, scene, rect, zoom=1):
    pair = [crop(module.png_rgba(root / "rendered" / f"{state}-{scene}.png"), rect, zoom) for state in ["before", "after"]]
    width, height = pair[0][:2]
    assert pair[1][:2] == (width, height)
    data = bytearray()
    for y in range(height):
        data.extend(pair[0][2][y*width*4:(y+1)*width*4])
        data.extend(b"\x80\x80\x80\xff"*8)
        data.extend(pair[1][2][y*width*4:(y+1)*width*4])
    write(root / "crops" / f"{name}.png", width*2+8, height, data)

root = pathlib.Path(sys.argv[1])
(root / "crops").mkdir(exist_ok=True)
for state in ["before", "after"]:
    image = module.png_rgba(root / "rendered" / f"{state}-selection-light-normal-1x.png")
    _, _, ink = crop(image, (38,272,18,18))
    mass = xmass = ymass = 0.
    for y in range(18):
        for x in range(18):
            p = (y*18+x)*4
            value = max(0., min(ink[p:p+3])-180.)
            mass += value; xmass += x*value; ymass += y*value
    assert mass > 0
    center = (xmass/mass,ymass/mass)
    print(f"{state} checkbox decoded white-ink centroid: {center}; face center=(8.5,8.5)")
    if state == "after":
        assert abs(center[0]-8.5)<0.15 and abs(center[1]-8.5)<0.15
compare(root, "checkbox-8x-before-left-after-right", "selection-light-normal-1x", (32,266,36,36), 8)
compare(root, "focus-2x-before-left-after-right", "focus-light-2x", (412,258,420,100))
compare(root, "tree-dark-2x-before-left-after-right", "data-dark-normal-2x", (1024,620,930,1080))
compare(root, "table-light-1x-before-left-after-right", "editors-light-normal-1x", (20,240,475,410))
print("PASS decoded actual-PNG centering and comparison crops (before left / after right). Native input remains unverified.")
