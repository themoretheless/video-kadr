#!/usr/bin/env python3
"""Generate automatic keyed Vec2 fixtures with post-interpolation fallback from pinned Filmcraft."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
SHA = '5231852443363f001c3f6b396dd9b1e6461ae2be'
p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--reference', type=Path, required=True)
p.add_argument('--toolchain', default='1.98.1')
p.add_argument('--check', action='store_true')
a = p.parse_args()
reference = a.reference.resolve()
head = subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip()
if head != SHA:
    raise SystemExit('reference commit mismatch')
if subprocess.check_output(['git', '-C', str(reference), 'status', '--porcelain', '--untracked-files=no'], text=True).strip():
    raise SystemExit('reference has tracked changes')
with tempfile.TemporaryDirectory(prefix='video-kadr-auto-key-point-oracle-') as directory:
    manifest = Path(directory) / 'Cargo.toml'
    quote = lambda path: json.dumps(str(path))
    manifest.write_text('[package]\nname="video-kadr-auto-key-point-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="auto-key-point-oracle"\npath=' + quote(ROOT/'scripts/filmcraft-auto-key-point-oracle.rs') + '\n[dependencies]\nserde_json="1"\n' + ''.join(f'filmcraft-{name}={{path={quote(reference/"crates"/name)}}}\n' for name in ['render','project','color','time','media','geom']))
    result = subprocess.check_output(['cargo', '+'+a.toolchain, 'run', '--quiet', '--manifest-path', str(manifest)], env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
    parsed = json.loads(result)
    if parsed['commit'] != SHA or len(parsed['cases']) != 28224:
        raise SystemExit('invalid generated fixture')
    target = ROOT/'backend/tests/fixtures/filmcraft-auto-key-point.json'
    if a.check:
        if parsed != json.loads(target.read_text()):
            raise SystemExit('saved fixture differs from upstream render')
    else:
        target.write_bytes(result)
print('FilmCraft auto-key-point SDR oracle: PASS')
