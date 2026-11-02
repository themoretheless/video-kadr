#!/usr/bin/env python3
"""Run saved 112-kernel oracle/admission regressions independently of backend integration."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--toolchain', default='1.89.0')
parser.add_argument('--filter', default='', help='Optional Rust test name filter')
args = parser.parse_args()
with tempfile.TemporaryDirectory(prefix='video-kadr-kernel-pack-check-') as directory:
    work = Path(directory)
    module = root / 'backend/src/domain/transition_kernel_pack/mod.rs'
    regression = root / 'scripts/transition-kernel-checkpoint-regression.rs'
    (work / 'lib.rs').write_text('#[path=' + json.dumps(str(module)) + ']\nmod pack;\n#[path=' + json.dumps(str(regression)) + ']\nmod checkpoint_regression;\n')
    (work / 'Cargo.toml').write_text('''[package]
name="video-kadr-transition-pack-check"
version="0.1.0"
edition="2021"
[lib]
path="lib.rs"
[dependencies]
serde={version="1",features=["derive"]}
serde_json="1"
rayon="1"
''')
    subprocess.run(['cargo', '+' + args.toolchain, 'test', '--manifest-path', str(work / 'Cargo.toml'), args.filter, '--', '--nocapture'], check=True, env=dict(os.environ, CARGO_INCREMENTAL='0', CARGO_TARGET_DIR='/tmp/video-kadr-transition-pack-check/target'))
