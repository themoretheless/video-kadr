#!/usr/bin/env python3
"""Generate independent SDR vignette fixtures using the pinned upstream renderer."""
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
with tempfile.TemporaryDirectory(prefix='video-kadr-vignette-oracle-') as directory:
    manifest = Path(directory) / 'Cargo.toml'
    quote = lambda path: json.dumps(str(path))
    manifest.write_text('[package]\nname="video-kadr-vignette-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="vignette-oracle"\npath=' + quote(ROOT/'scripts/filmcraft-vignette-oracle.rs') + '\n[dependencies]\nserde_json="1"\n' + ''.join(f'filmcraft-{name}={{path={quote(reference/"crates"/name)}}}\n' for name in ['render','project','color','time']))
    result = subprocess.check_output(['cargo', '+'+a.toolchain, 'run', '--quiet', '--manifest-path', str(manifest)], env=os.environ)
    parsed = json.loads(result)
    if parsed['commit'] != SHA or len(parsed['cases']) != 45:
        raise SystemExit('invalid generated fixture')
    target = ROOT/'backend/tests/fixtures/filmcraft-color-vignette.json'
    if a.check:
        if parsed != json.loads(target.read_text()):
            raise SystemExit('saved fixture differs from upstream render')
    else:
        target.write_bytes(result)
print('FilmCraft vignette SDR oracle: PASS')
