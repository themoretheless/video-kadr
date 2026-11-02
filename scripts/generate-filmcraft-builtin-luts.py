#!/usr/bin/env python3
"""Generate independent combined SDR Lumetri fixtures using the pinned upstream renderer."""
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
with tempfile.TemporaryDirectory(prefix='video-kadr-builtin-luts-oracle-') as directory:
    manifest = Path(directory) / 'Cargo.toml'
    quote = lambda path: json.dumps(str(path))
    manifest.write_text('[package]\nname="video-kadr-builtin-luts-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="builtin-luts-oracle"\npath=' + quote(ROOT/'scripts/filmcraft-builtin-luts-oracle.rs') + '\n[dependencies]\nserde_json="1"\n' + ''.join(f'filmcraft-{name}={{path={quote(reference/"crates"/name)}}}\n' for name in ['render','project','color','time','geom']))
    result = subprocess.check_output(['cargo', '+'+a.toolchain, 'run', '--quiet', '--manifest-path', str(manifest)], env=os.environ)
    parsed = json.loads(result)
    import struct
    if parsed['commit'] != SHA or not 17 <= len(parsed['entries']) <= 64:
        raise SystemExit('invalid builtin catalogue')
    metadata=[];binary=bytearray()
    for entry in parsed['entries']:
        if entry['size'] != 33 or len(entry['data']) != 33**3:
            raise SystemExit('unexpected builtin grid')
        metadata.append({k:v for k,v in entry.items() if k!='data'})
        for rgb in entry['data']:
            if len(rgb)!=3 or any(not 0<=q<=1 for q in rgb):
                raise SystemExit('invalid builtin sample')
            binary.extend(struct.pack('<3f',*rgb))
    target=ROOT/'backend/src/domain/assets'
    outputs={'filmcraft-builtin-luts.json':(json.dumps({'commit':SHA,'entries':metadata},indent=2)+'\n').encode(),'filmcraft-builtin-luts.bin':bytes(binary)}
    if not a.check: target.mkdir(parents=True,exist_ok=True)
    for name,data in outputs.items():
        if a.check:
            if (target/name).read_bytes()!=data: raise SystemExit('saved builtin assets differ')
        else: (target/name).write_bytes(data)
print('FilmCraft builtin-luts SDR oracle: PASS')
