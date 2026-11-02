#!/usr/bin/env python3
"""Build the original FFmpeg EBU meter, with allocator/thread shims only.

Usage: build.py /path/to/ebur128.c /path/to/ebur128.h /output/meter
Upstream: https://github.com/FFmpeg/FFmpeg/tree/master/libavfilter
The exact audited source bytes are required; no network or production linkage.
"""
import hashlib
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sources = [Path(p).resolve() for p in sys.argv[1:3]]
if len(sys.argv) != 4:
    raise SystemExit(__doc__)
expected = [
    'bce02f6c8f0518d538596c7c46c6bbce68e5f50042f68a17cc04e6c533d1a459',
    '891e45637d56cd6292633d865564dda03ae5059fe471d54eceeebb01eccb583f',
]
for source, digest in zip(sources, expected):
    if hashlib.sha256(source.read_bytes()).hexdigest() != digest:
        raise SystemExit(f'Original source hash mismatch: {source}')
output = Path(sys.argv[3]).resolve()
output.parent.mkdir(parents=True, exist_ok=True)
here = Path(__file__).resolve().parent
with tempfile.TemporaryDirectory(prefix='video-kadr-ebu-oracle-') as staging:
    root = Path(staging)
    (root / 'libavutil').mkdir()
    for source, name in zip(sources, ['ebur128.c', 'ebur128.h']):
        shutil.copyfile(source, root / name)
    shutil.copyfile(here / 'main.c', root / 'main.c')
    shutil.copyfile(here / 'shim.h', root / 'libavutil/shim.h')
    for name in ['error', 'macros', 'mem', 'mem_internal', 'thread']:
        (root / f'libavutil/{name}.h').write_text('#include "shim.h"\n')
    subprocess.run(['cc', '-O2', '-ffp-contract=off', f'-I{root}',
                    str(root / 'main.c'), str(root / 'ebur128.c'),
                    '-lm', '-o', str(output)], check=True)
print(output)
