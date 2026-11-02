#!/usr/bin/env python3
"""Qualify the actual native enhancer with an owned synthetic PNG only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import signal
import struct
import subprocess
import tempfile
import time
import zlib

parser = argparse.ArgumentParser()
for name in ("executable", "model", "runtime", "artifacts"):
    parser.add_argument("--" + name, type=Path, required=True)
args = parser.parse_args()
root = Path(tempfile.mkdtemp(prefix="enhance-cli-", dir=args.artifacts.resolve()))
root.chmod(0o700)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))


width, height = 257, 9
rows = bytearray()
for y in range(height):
    rows.append(0)
    for x in range(width):
        rows.extend(((x * 13 + y * 7) % 256, (x * 9 + y * 17) % 256, 80, 255))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
png += chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
source = root / "synthetic.png"
source.write_bytes(png)
source.chmod(0o600)
output = root / "enhanced.png"
request = root / "request.json"
payload = dict(schemaVersion=1, model=str(args.model.resolve()), runtimeLibrary=str(args.runtime.resolve()),
               input=str(source), output=str(output), sourceSha256=hashlib.sha256(png).hexdigest(), factor=2)


def write_request(value):
    request.write_text(json.dumps(value))
    request.chmod(0o600)


command = [str(args.executable.resolve()), "--request", str(request)]
write_request(payload)
process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env={}, start_new_session=True)
try:
    deadline = time.monotonic() + 60
    with selectors.DefaultSelector() as selector:
        selector.register(process.stderr, selectors.EVENT_READ)
        while True:
            assert time.monotonic() < deadline, "first tile deadline exceeded"
            if not selector.select(1):
                assert process.poll() is None, "process ended before first tile"
                continue
            if process.poll() is not None:
                raise AssertionError("process ended before cancellation")
            line = process.stderr.readline()
            try:
                progress = json.loads(line)
            except (ValueError, UnicodeError):
                continue
            if progress.get("event") == "progress":
                assert progress["completed"] == 1 and progress["total"] == 2
                os.killpg(process.pid, signal.SIGTERM)
                process.communicate(timeout=10)
                assert process.returncode != 0
                break
finally:
    if process.poll() is None:
        os.killpg(process.pid, signal.SIGKILL)
        process.communicate(timeout=10)
assert not output.exists(), "cancelled enhancement published a partial image"

completed = subprocess.run(command, capture_output=True, env={}, timeout=60)
(root / "generation.log").write_bytes(completed.stderr)
assert completed.returncode == 0, completed.stderr.decode(errors="replace")[-1000:]
metadata = json.loads(completed.stdout)
encoded = output.read_bytes()
assert encoded[:8] == b"\x89PNG\r\n\x1a\n"
assert struct.unpack(">II", encoded[16:24]) == (width * 2, height * 2)
assert metadata["width"] == width * 2 and metadata["height"] == height * 2
assert metadata["sourceHash"] == payload["sourceSha256"]
assert metadata["outputHash"] == hashlib.sha256(encoded).hexdigest()
assert metadata["modelHash"] == hashlib.sha256(args.model.read_bytes()).hexdigest()
assert metadata["runtimeHash"] == hashlib.sha256(args.runtime.read_bytes()).hexdigest()
assert output.stat().st_mode & 0o777 == 0o600
repeat = subprocess.run(command, capture_output=True, env={}, timeout=10)
assert repeat.returncode != 0 and output.read_bytes() == encoded
for update in ({"sourceSha256": "0" * 64, "output": str(root / "wrong-source.png")},
               {"url": "https://invalid", "output": str(root / "unknown-field.png")}):
    write_request({**payload, **update})
    refused = subprocess.run(command, capture_output=True, env={}, timeout=10)
    assert refused.returncode != 0 and not Path(update["output"]).exists()
assert not list(root.glob(".enhance-*"))
report = {"root": str(root), "cancelledAfterTiles": 1, "metadata": metadata,
          "privatePng": True, "noOverwrite": True, "invalidSourceRefused": True, "unknownFieldRefused": True}
(root / "report.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report))
