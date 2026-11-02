#!/usr/bin/env python3
"""Build and atomically provision the host-controlled Rust SAM runtime.

The destination must be explicitly selected. Existing executables are retained
under a content-addressed backup; no model paths or server configuration change.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def capabilities(path):
    result = subprocess.run([str(path), '--capabilities'], capture_output=True,
                            text=True, check=True, timeout=10)
    value = json.loads(result.stdout)
    assert value['schemaVersion'] == 1 and value['samReidentificationSchema'] in (14, 15)
    return value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--destination', type=Path, required=True)
    parser.add_argument('--profile', choices=['debug', 'release'], default='release')
    args = parser.parse_args()
    destination = args.destination
    assert destination.is_absolute() and not destination.is_symlink()
    root = Path(__file__).resolve().parents[1]
    target = root / 'backend/vision-runtime/target'
    command = ['cargo', 'build', '--manifest-path', str(root / 'backend/vision-runtime/Cargo.toml'),
               '--target-dir', str(target), '--bin', 'video-kadr-vision-runtime']
    if args.profile == 'release':
        command.append('--release')
    subprocess.run(command, check=True)
    source = target / args.profile / 'video-kadr-vision-runtime'
    identity = sha(source)
    value = capabilities(source)
    destination.parent.mkdir(parents=True, exist_ok=True)
    backup = None
    if destination.exists():
        assert destination.is_file()
        previous = sha(destination)
        if previous == identity:
            print(json.dumps({'destination': str(destination), 'sha256': identity,
                              'unchanged': True, 'capabilities': value}))
            return
        backup = destination.with_name(destination.name + '.backup-' + previous[:16])
        if not backup.exists():
            os.link(destination, backup)
        assert sha(backup) == previous
    descriptor, temporary = tempfile.mkstemp(prefix='.sam-runtime-', dir=destination.parent)
    os.close(descriptor)
    temporary = Path(temporary)
    try:
        shutil.copyfile(source, temporary)
        temporary.chmod(0o700)
        assert sha(temporary) == identity
        capabilities(temporary)
        with temporary.open('rb') as stream:
            os.fsync(stream.fileno())
        os.replace(temporary, destination)
    finally:
        temporary.unlink(missing_ok=True)
    print(json.dumps({'destination': str(destination), 'sha256': identity,
                      'backup': str(backup) if backup else None, 'capabilities': value}))


if __name__ == '__main__':
    main()
