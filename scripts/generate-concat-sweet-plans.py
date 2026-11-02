#!/usr/bin/env python3
"""Generate metadata-only plans using the pinned original Sweet compiler."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

PIN = '78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--output', type=Path, default=Path('backend/tests/fixtures/concat-sweet-plans.json'))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    ref = args.reference.resolve()
    assert subprocess.check_output(['git', '-C', str(ref), 'rev-parse', 'HEAD'], text=True).strip() == PIN
    paths = ['src/crates/concat-effects', 'src/crates/concat-project', 'src/crates/concat-core', 'src/Cargo.toml', 'src/Cargo.lock']
    assert not subprocess.check_output(['git', '-C', str(ref), 'status', '--porcelain', '--', *paths])
    adapter = repo / 'scripts/reference/concat_audio_sweet.rs'
    tracked = ['src/Cargo.lock', 'src/crates/concat-effects/packages/concat.sweet/effect.toml', 'src/crates/concat-effects/packages/concat.sweet/fixtures.toml']
    with tempfile.TemporaryDirectory(prefix='concat-sweet-plans-') as scratch:
        root = Path(scratch)
        (root / 'src').mkdir()
        shutil.copyfile(adapter, root / 'src/main.rs')
        shutil.copyfile(ref / 'src/Cargo.lock', root / 'Cargo.lock')
        deps = '\n'.join(f'{name}={{path={json.dumps(str(ref / "src/crates" / name))}}}' for name in ['concat-effects', 'concat-project'])
        (root / 'Cargo.toml').write_text('[package]\nname="video-kadr-concat-audio-oracle"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n' + deps + '\nserde_json="1"\n')
        env = os.environ.copy()
        env.update(CARGO_TARGET_DIR=str(repo / 'backend/target/concat-gpu-oracle'), CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0')
        plans = root / 'plans.json'
        subprocess.run(['cargo', '+1.98.1', 'run', '--manifest-path', str(root / 'Cargo.toml'), '--', str(plans)], env=env, check=True)
        result = json.loads(plans.read_text())
    result.update(repository='https://github.com/jub0t/concat', commit=PIN, license=tomllib.loads((ref / 'src/Cargo.toml').read_text())['workspace']['package']['license'], adapterSha256=hashlib.sha256(adapter.read_bytes()).hexdigest(), sourceHashes={p:hashlib.sha256((ref / p).read_bytes()).hexdigest() for p in tracked})
    args.output.write_text(json.dumps(result, indent=2, ensure_ascii=False, allow_nan=False) + '\n')
    print(f'{len(result["cases"])} original Sweet plans; no PCM payload')
if __name__ == '__main__':
    main()
