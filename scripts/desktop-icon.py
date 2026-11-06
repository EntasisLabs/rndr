#!/usr/bin/env python3
"""Generate the example's own geometric icon; stdlib only, no external artwork."""
import binascii
import pathlib
import struct
import sys
import zlib


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", binascii.crc32(kind + data) & 0xFFFFFFFF)


def png(size):
    rows = bytearray()
    for y in range(size):
        rows.append(0)
        for x in range(size):
            u, v = x / size, y / size
            color = (23, 30, 43, 255)
            if 0.10 <= u <= 0.90 and 0.10 <= v <= 0.90:
                color = (58, 104, 174, 255)
            # A crisp lowercase r, with breathing room at every icon size.
            if (0.30 <= u <= 0.43 and 0.30 <= v <= 0.74) or (0.43 < u <= 0.69 and 0.30 <= v <= 0.43) or (0.57 <= u <= 0.69 and 0.43 < v <= 0.52):
                color = (241, 245, 251, 255)
            rows.extend(color)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(rows))) + chunk(b"IEND", b"")


if __name__ == "__main__":
    directory = pathlib.Path(sys.argv[1])
    directory.mkdir(parents=True, exist_ok=False)
    for logical in [16, 32, 128, 256, 512]:
        for scale in [1, 2]:
            suffix = "@2x" if scale == 2 else ""
            (directory / f"icon_{logical}x{logical}{suffix}.png").write_bytes(png(logical * scale))
