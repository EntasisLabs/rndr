#!/usr/bin/env python3
"""Bounded synthetic-gallery-only native validation; never desktop automation.

Usage: python3 tools/validate_observation_native.py target/debug/examples/ui_gallery OUTDIR [gallery flags]
Starts only the explicitly supplied synthetic gallery executable. Requests two
observations through its opt-in socket, probes stale/auth rejection, waits for
its normal 15s timer exit, and checks descriptor cleanup. No OS grants requested.
"""
import hashlib
import json
from pathlib import Path
import re
import socket
import subprocess
import sys
import time
from observe_client import observe, save_frame, _read_exact
import struct


def main():
    binary = Path(sys.argv[1]).resolve()
    output = Path(sys.argv[2])
    output.mkdir(parents=True, exist_ok=True)
    command = [str(binary), "--observe", "--observe-exit-after-15s"] + sys.argv[3:]
    result = {"command": command, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
              "source": "native-synthetic-gallery-session", "presented_confirmation": False}
    descriptor = None
    with (output / "native-process.log").open("wb") as log:
        child = subprocess.Popen(command, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 6
            while time.monotonic() < deadline:
                text = (output / "native-process.log").read_text(errors="replace")
                match = re.search(r"observation descriptor: (.+)", text)
                if match:
                    descriptor = Path(match[1])
                    break
                if child.poll() is not None:
                    break
                time.sleep(.05)
            if descriptor is None:
                raise RuntimeError("native startup unavailable; see native-process.log")
            # A just-created window may have an unavailable/occluded first
            # surface. Retry fresh requests only; never reuse a cached frame.
            retries = []
            until = time.monotonic() + 5
            while True:
                try:
                    first, png = observe(descriptor)
                    break
                except RuntimeError as error:
                    retries.append(str(error))
                    if "surface_unavailable" not in str(error) or time.monotonic() >= until:
                        raise
                    time.sleep(.1)
            result["startup_denials"] = retries
            save_frame(output / "frame-1", first, png)
            second, png = observe(descriptor, int(first["frame_revision"]))
            save_frame(output / "frame-2", second, png)
            assert int(second["frame_revision"]) > int(first["frame_revision"])
            try:
                observe(descriptor, 2**64 - 1)
                raise AssertionError("future revision was not rejected")
            except RuntimeError as error:
                assert "stale_revision" in str(error), str(error)
            desc = json.loads(descriptor.read_bytes())
            with socket.socket(socket.AF_UNIX) as stream:
                stream.settimeout(3)
                stream.connect(desc["socket"])
                stream.sendall(("RNDR1 " + "0" * 64 + " 0\n").encode())
                until = time.monotonic() + 3
                length, = struct.unpack(">I", _read_exact(stream, 4, until))
                assert length < 4096
                denial = json.loads(_read_exact(stream, length, until))
                assert denial["error"] == "invalid_request" and denial["png_bytes"] == 0
            result.update(status="captured", revisions=[first["frame_revision"], second["frame_revision"]],
                          dimensions=[second["width"], second["height"]], nodes=len(second["nodes"]),
                          provenance=second["render_provenance"], stale_rejection=True, invalid_auth_rejection=True)
            result["exit_code"] = child.wait(timeout=18)
            assert result["exit_code"] == 0
            assert not descriptor.parent.exists(), "session directory not cleaned"
            result["normal_shutdown_cleanup"] = True
        except Exception as error:
            result.update(status="blocked-or-failed", error=str(error))
        finally:
            if child.poll() is None:
                try:
                    child.wait(timeout=18)
                except subprocess.TimeoutExpired:
                    child.terminate()
                    try:
                        child.wait(timeout=3)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=3)
                    result["forced_shutdown"] = True
            result["exit_code"] = child.returncode
            if descriptor is not None:
                result["normal_shutdown_cleanup"] = not descriptor.parent.exists()
    # No capability values are persisted; only synthetic frames and process log.
    (output / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))
    return 0 if result["status"] == "captured" and result.get("normal_shutdown_cleanup") else 1


if __name__ == "__main__":
    sys.exit(main())
