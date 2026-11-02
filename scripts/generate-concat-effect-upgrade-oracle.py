#!/usr/bin/env python3
"""Run the pinned original effect-link catalogue migration, without a GPU."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PIN = '78e4be101c2d3c41a7dc73c112c6b1f11b9ce170'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reference', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--target', required=True, type=Path)
    args = parser.parse_args()
    reference = args.reference.resolve()
    output = args.output.resolve()
    repo = Path(__file__).resolve().parent.parent
    if repo.joinpath('backend/tests').resolve() in output.parents:
        raise SystemExit('generate into an unregistered qualification artifact')
    if subprocess.check_output(['git', '-C', str(reference), 'rev-parse', 'HEAD'], text=True).strip() != PIN:
        raise SystemExit('wrong reference commit')
    def clean():
        if subprocess.check_output(['git', '-C', str(reference), 'status', '--porcelain', '--untracked-files=all', '--', 'src/crates', 'src/Cargo.lock', 'src/Cargo.toml'], text=True):
            raise SystemExit('reference source is dirty')
    clean()
    files = ['src/Cargo.lock', 'src/crates/concat-core/src/animate.rs', 'src/crates/concat-project/src/model.rs', 'src/crates/concat-effects/src/catalogue.rs', 'src/crates/concat-effects/src/shader.rs', 'src/crates/concat-effects/src/manifest.rs']
    files += [f'src/crates/concat-effects/packages/concat.{name}/effect.toml' for name in ['chroma-key', 'gaussian-blur']]
    def hashes():
        return {name: hashlib.sha256((reference / name).read_bytes()).hexdigest() for name in files}
    fingerprints = hashes()
    adapter = Path(__file__).resolve().parent / 'reference/concat_effect_upgrade.rs'
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='video-kadr-concat-upgrade-') as directory:
        root = Path(directory)
        (root / 'src').mkdir()
        shutil.copyfile(adapter, root / 'src/main.rs')
        shutil.copyfile(reference / 'src/Cargo.lock', root / 'Cargo.lock')
        dependencies = '\n'.join(name + ' = { path = ' + json.dumps(str(reference / 'src/crates' / name)) + ' }' for name in ['concat-effects', 'concat-project'])
        (root / 'Cargo.toml').write_text('[package]\nname="video-kadr-concat-upgrade-oracle"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n' + dependencies + '\nserde_json="1"\n')
        env = os.environ.copy()
        env.update(CARGO_TARGET_DIR=str(args.target.resolve()), CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0')
        subprocess.run(['cargo', '+1.98.1', 'run', '--manifest-path', str(root / 'Cargo.toml'), '--', str(output)], env=env, check=True)
    clean()
    if fingerprints != hashes():
        raise SystemExit('reference changed during qualification')
    data = json.loads(output.read_text())
    data['reference'] = {'repository': 'https://github.com/jub0t/concat', 'commit': PIN, 'files': fingerprints, 'adapterSha256': hashlib.sha256(adapter.read_bytes()).hexdigest()}
    output.write_text(json.dumps(data, indent=2, allow_nan=False) + '\n')
    print(f'Original Concat catalogue: {len(data["cases"])} migration cases; {output}')

if __name__ == '__main__':
    main()
