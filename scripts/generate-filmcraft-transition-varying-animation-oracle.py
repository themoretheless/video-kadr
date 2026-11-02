#!/usr/bin/env python3
"""Generate independent SDR transition-varying-animation fixtures using the pinned upstream renderer."""
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
with tempfile.TemporaryDirectory(prefix='video-kadr-transition-varying-animation-oracle-') as directory:
    manifest = Path(directory) / 'Cargo.toml'
    quote = lambda path: json.dumps(str(path))
    manifest.write_text('[package]\nname="video-kadr-transition-varying-animation-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="transition-varying-animation-oracle"\npath=' + quote(ROOT/'scripts/filmcraft-transition-varying-animation-oracle.rs') + '\n[dependencies]\nserde_json="1"\n' + ''.join(f'filmcraft-{name}={{path={quote(reference/"crates"/name)}}}\n' for name in ['render','project','color','time','media','frame']))
    result = subprocess.check_output(['cargo', '+'+a.toolchain, 'run', '--quiet', '--manifest-path', str(manifest)], env=dict(os.environ,CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-video-oracle-target'))
    parsed = json.loads(result)
    if parsed['commit'] != SHA or len(parsed['cases']) != parsed['definitions'] * 36 or len({case['effect'] for case in parsed['cases']}) != parsed['definitions']:
        raise SystemExit('invalid generated fixture')
    target = ROOT/'backend/tests/fixtures/filmcraft-transition-varying-animation.json'
    if a.check:
        if parsed != json.loads(target.read_text()):
            raise SystemExit('saved fixture differs from upstream render')
    else:
        target.write_bytes(result)
print('FilmCraft transition-varying-animation SDR oracle: PASS')
