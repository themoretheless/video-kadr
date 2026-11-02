#!/usr/bin/env python3
"""Run the pinned original engine; emit compact command oracle cases."""
import argparse
import os
from pathlib import Path
import subprocess
import tempfile
PIN = "5231852443363f001c3f6b396dd9b1e6461ae2be"
ROOT = Path(__file__).resolve().parents[1]
def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--toolchain", default="1.98.1")
    parser.add_argument("--rack-edits", action="store_true", help="Generate actual original effect-rack mutation cases")
    parser.add_argument("--apply-effects", action="store_true", help="Generate original effects.apply cases")
    parser.add_argument("--set-param", action="store_true", help="Generate original effects.setParam cases")
    parser.add_argument("--keyframe-edits", action="store_true", help="Generate original keyframe mutation cases")
    parser.add_argument("--animation", action="store_true", help="Generate original animation toggle/add cases")
    parser.add_argument("--mask-commands", action="store_true", help="Generate original masks.* cases")
    parser.add_argument("--mask-track-batch", action="store_true", help="Generate original sequential masks.track cases")
    parser.add_argument("--clip-rename", action="store_true", help="Generate original clip.rename addressing cases")
    parser.add_argument("--bin-rename", action="store_true", help="Generate actual original project.renameBin cases")
    parser.add_argument("--bin-edits", action="store_true", help="Generate original bin create/group/move cases")
    parser.add_argument("--labels", action="store_true", help="Generate original generic/named label cases")
    parser.add_argument("--label-group", action="store_true", help="Generate original selectLabelGroup cases")
    parser.add_argument("--selection", action="store_true", help="Generate original matching/project-selection cases")
    parser.add_argument("--duplicate", action="store_true", help="Generate original project-item duplicate cases")
    args = parser.parse_args()
    if sum([args.apply_effects,args.rack_edits,args.set_param,args.keyframe_edits,args.animation,args.mask_commands,args.mask_track_batch,args.clip_rename,args.bin_rename,args.bin_edits,args.labels,args.label_group,args.selection,args.duplicate])>1: parser.error("choose one oracle mode")
    reference = args.reference.resolve()
    assert subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip() == PIN
    assert not subprocess.check_output(["git", "-C", str(reference), "status", "--porcelain", "--untracked-files=no"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="filmcraft-effects-list-") as directory:
        build = Path(directory)
        (build / "src").mkdir()
        import json
        dependencies = "\n".join(f'filmcraft-{name}={{path={json.dumps(str(reference / "crates" / name))}}}' for name in ["engine", "project", "time", "media", "frame"])
        (build / "Cargo.toml").write_text('[package]\nname="filmcraft-effects-list-oracle"\nversion="0.1.0"\nedition="2024"\n[dependencies]\n' + dependencies + '\nserde_json="1"\nsha2="0.10"\n')
        (build / "src/main.rs").write_text((ROOT / ("scripts/filmcraft-duplicate-oracle.rs" if args.duplicate else "scripts/filmcraft-selection-oracle.rs" if args.selection else "scripts/filmcraft-label-group-oracle.rs" if args.label_group else "scripts/filmcraft-label-oracle.rs" if args.labels else "scripts/filmcraft-bin-edit-oracle.rs" if args.bin_edits else "scripts/filmcraft-bin-rename-oracle.rs" if args.bin_rename else "scripts/filmcraft-clip-rename-oracle.rs" if args.clip_rename else "scripts/filmcraft-mask-track-batch-oracle.rs" if args.mask_track_batch else "scripts/filmcraft-mask-command-oracle.rs" if args.mask_commands else "scripts/filmcraft-animation-oracle.rs" if args.animation else "scripts/filmcraft-keyframe-edit-oracle.rs" if args.keyframe_edits else "scripts/filmcraft-set-param-oracle.rs" if args.set_param else "scripts/filmcraft-effect-apply-oracle.rs" if args.apply_effects else "scripts/filmcraft-effect-edit-oracle.rs" if args.rack_edits else "scripts/filmcraft-effects-list-oracle.rs")).read_text())
        env = dict(os.environ, KADR_SOURCE_ROOT=str(ROOT), CARGO_TARGET_DIR=str(ROOT / "backend/target/filmcraft-effects-list-oracle"), CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_BUILD_JOBS="2")
        subprocess.run(["cargo", "+" + args.toolchain, "run", "--offline", "--manifest-path", str(build / "Cargo.toml"), "--", str(ROOT / ("backend/tests/fixtures/filmcraft-duplicate-oracle.json" if args.duplicate else "backend/tests/fixtures/filmcraft-selection-oracle.json" if args.selection else "backend/tests/fixtures/filmcraft-label-group-oracle.json" if args.label_group else "backend/tests/fixtures/filmcraft-label-oracle.json" if args.labels else "backend/tests/fixtures/filmcraft-bin-edit-oracle.json" if args.bin_edits else "backend/tests/fixtures/filmcraft-bin-rename-oracle.json" if args.bin_rename else "backend/tests/fixtures/filmcraft-clip-rename-oracle.json" if args.clip_rename else "backend/tests/fixtures/filmcraft-mask-track-batch-oracle.json" if args.mask_track_batch else "backend/tests/fixtures/filmcraft-mask-command-oracle.json" if args.mask_commands else "backend/tests/fixtures/filmcraft-animation-oracle.json" if args.animation else "backend/tests/fixtures/filmcraft-keyframe-edit-oracle.json" if args.keyframe_edits else "backend/tests/fixtures/filmcraft-set-param-oracle.json" if args.set_param else "backend/tests/fixtures/filmcraft-effect-apply-oracle.json" if args.apply_effects else "backend/tests/fixtures/filmcraft-effect-edit-oracle.json" if args.rack_edits else "backend/tests/fixtures/filmcraft-effects-list-oracle.json"))], env=env, check=True)
if __name__ == "__main__":
    main()
