#!/usr/bin/env python3
"""Stage native shell icon metadata for an already-built desktop executable."""
import argparse
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile


APP_ID = r"[A-Za-z_][A-Za-z0-9_-]*(?:\.[A-Za-z_][A-Za-z0-9_-]*)+"
VERSION = r"[0-9]+(?:\.[0-9]+){0,2}"


def png_size(path):
    with path.open("rb") as f:
        header = f.read(24)
    if len(header) != 24 or header[:16] != b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR":
        raise ValueError("--icon must be a PNG")
    width, height = struct.unpack(">II", header[16:24])
    if width != height or width not in (16, 24, 32, 48, 64, 128, 256, 512, 1024):
        raise ValueError("--icon must be square at a standard icon size (16–1024 pixels)")
    return width


def put_binary(source, destination):
    destination.parent.mkdir(parents=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755)


def macos(args, stage, output):
    if sys.platform != "darwin":
        raise ValueError("macOS packaging requires macOS")
    if "_" in args.id:
        raise ValueError("macOS bundle IDs cannot contain underscores")
    if not output.name.endswith(".app"):
        raise ValueError("--output must end with .app")
    if png_size(args.icon) != 1024:
        raise ValueError("macOS needs a 1024×1024 PNG for the full iconset")
    if not all(re.fullmatch(VERSION, v) for v in (args.version, args.build_number)):
        raise ValueError("--version and --build-number must be numeric (one to three components)")
    executable = args.executable_name or args.binary.name
    if not re.fullmatch(r"[A-Za-z0-9_][A-Za-z0-9_.-]*", executable):
        raise ValueError("invalid executable name; use --executable-name")
    contents = stage / "Contents"
    put_binary(args.binary, contents / "MacOS" / executable)
    resources = contents / "Resources"
    resources.mkdir()
    # iconutil requires both logical sizes and Retina variants even when some
    # share the same physical dimensions. Keep the input unmodified.
    iconset = stage.parent / "AppIcon.iconset"
    iconset.mkdir()
    for logical in (16, 32, 128, 256, 512):
        for scale in (1, 2):
            pixels = str(logical * scale)
            suffix = "@2x" if scale == 2 else ""
            dest = iconset / f"icon_{logical}x{logical}{suffix}.png"
            if pixels == "1024":
                shutil.copyfile(args.icon, dest)
            else:
                subprocess.run(["sips", "-z", pixels, pixels, str(args.icon.resolve()),
                                "--out", str(dest)], check=True, stdout=subprocess.DEVNULL)
    subprocess.run(["iconutil", "-c", "icns", str(iconset),
                    "-o", str(resources / "AppIcon.icns")], check=True)
    plist = contents / "Info.plist"
    with plist.open("wb") as f:
        plistlib.dump({
            "CFBundleDevelopmentRegion": "en",
            "CFBundleExecutable": executable,
            "CFBundleIconFile": "AppIcon.icns",
            "CFBundleIdentifier": args.id,
            "CFBundleName": args.name,
            "CFBundleDisplayName": args.name,
            "CFBundlePackageType": "APPL",
            "CFBundleShortVersionString": args.version,
            "CFBundleVersion": args.build_number,
            "NSHighResolutionCapable": True,
        }, f)
    subprocess.run(["plutil", "-lint", str(plist)], check=True)


def linux(args, stage, _output):
    # Only accept paths expressible as a simple quoted Desktop Entry Exec
    # argument. No shell is involved; exclude special escapes/field codes.
    if not re.fullmatch(r"/[A-Za-z0-9_ ./-]*", args.prefix):
        raise ValueError("--prefix must be an absolute path without desktop entry metacharacters")
    size = png_size(args.icon)
    binary = Path("bin") / args.id
    put_binary(args.binary, stage / binary)
    icon = stage / "share/icons/hicolor" / f"{size}x{size}" / "apps" / f"{args.id}.png"
    icon.parent.mkdir(parents=True)
    shutil.copyfile(args.icon, icon)
    launcher = stage / "share/applications" / f"{args.id}.desktop"
    launcher.parent.mkdir(parents=True)
    name = args.name.replace("\\", "\\\\").replace(" ", r"\s")
    exec_path = Path(args.prefix) / binary
    launcher.write_text(f"[Desktop Entry]\nType=Application\nName={name}\n"
                        f'Exec="{exec_path}"\nIcon={args.id}\n'
                        f"StartupWMClass={args.id}\nTerminal=false\n", encoding="utf-8")


def options():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="platform", required=True)
    mac = subparsers.add_parser("macos", help="create a local .app on macOS")
    xdg = subparsers.add_parser("linux", help="stage bin/ and share/ for an XDG launcher")
    for sub in (mac, xdg):
        sub.add_argument("--id", required=True, help="stable reverse-DNS shell identity")
        sub.add_argument("--name", required=True, help="display name")
        sub.add_argument("--binary", required=True, type=Path, help="already-built application")
        sub.add_argument("--icon", required=True, type=Path, help="square PNG artwork")
        sub.add_argument("--output", required=True, type=Path, help="new staging output path")
    mac.add_argument("--version", required=True)
    mac.add_argument("--build-number", required=True)
    mac.add_argument("--executable-name")
    xdg.add_argument("--prefix", default="/usr", help="final install prefix, not the staging path")
    return parser, parser.parse_args()


def main():
    parser, args = options()
    try:
        if not re.fullmatch(APP_ID, args.id) or len(args.id) > 128:
            raise ValueError("--id must be a reverse-DNS identifier (max 128 characters)")
        if not args.name.strip() or not args.name.isprintable():
            raise ValueError("--name must be nonempty printable text")
        if not args.binary.is_file():
            raise ValueError("--binary must point to a file")
        output = args.output.absolute()
        if output.exists() or output.is_symlink():
            raise ValueError(f"refusing to replace existing output: {output}")
        output.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix=".rndr-icon-", dir=output.parent) as work:
            stage = Path(work) / "output"
            {"macos": macos, "linux": linux}[args.platform](args, stage, output)
            stage.rename(output)
        print(output)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{parser.prog}: {error}\n")


if __name__ == "__main__":
    main()
