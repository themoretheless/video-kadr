#!/usr/bin/env python3
"""Compile actual upstream audio_fx configured() without the rest of render/GUI."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[1]
sha = '5231852443363f001c3f6b396dd9b1e6461ae2be'
parser = argparse.ArgumentParser()
parser.add_argument('--reference', type=Path, required=True)
parser.add_argument('--check', action='store_true')
parser.add_argument('--pcm', action='store_true')
parser.add_argument('--nondefault', action='store_true')
args = parser.parse_args()
assert not args.nondefault or args.pcm, "--nondefault requires --pcm"
reference = args.reference.resolve()
assert subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip() == sha
assert not subprocess.check_output(['git', '-C', str(reference), 'status', '--porcelain', '--untracked-files=no'], text=True).strip()
with tempfile.TemporaryDirectory(prefix='video-kadr-audio-mapping-oracle-') as directory:
    directory = Path(directory)
    rust = (root/('scripts/filmcraft-audio-pcm-oracle.rs' if args.pcm else 'scripts/filmcraft-audio-mapping-oracle.rs')).read_text().replace('"__AUDIO_FX_PATH__"', json.dumps(str(reference/'crates/render/src/audio_fx.rs')))
    (directory/'main.rs').write_text(rust)
    manifest = '[package]\nname="video-kadr-audio-mapping-oracle"\nversion="0.1.0"\nedition="2024"\n[[bin]]\nname="audio-mapping-oracle"\npath="main.rs"\n[dependencies]\nserde_json="1"\n'
    manifest += ''.join(f'filmcraft-{name}={{path={json.dumps(str(reference/"crates"/name))}}}\n' for name in ['project','audio-dsp','time'])
    (directory/'Cargo.toml').write_text(manifest)
    environment = dict(os.environ, CARGO_TARGET_DIR='/tmp/video-kadr-filmcraft-audio-mapping-target')
    output = subprocess.check_output(['cargo', '+1.98.1', 'run', '--quiet', '--manifest-path', str(directory/'Cargo.toml')] + (['--', '--nondefault'] if args.nondefault else []), env=environment)
    parsed = json.loads(output)
    assert parsed['sourceCommit'] == sha and len(parsed['cases']) == (495 if args.nondefault else 165 if args.pcm else 220)
    target = root/('backend/tests/fixtures/filmcraft-audio-pcm-nondefault.json' if args.nondefault else 'backend/tests/fixtures/filmcraft-audio-pcm.json' if args.pcm else 'backend/tests/fixtures/filmcraft-audio-mapping.json')
    if args.check:
        assert parsed == json.loads(target.read_text()), 'source configured parameter targets changed'
    else:
        target.write_bytes(output)
print(('495 actual source nondefault PCM requests' if args.nondefault else '165 actual source PCM requests' if args.pcm else '220 actual source project-to-DSP configurations') + (' match' if args.check else ' generated'))
