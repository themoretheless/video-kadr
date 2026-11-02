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
    parser.add_argument("--project-delete", action="store_true", help="Generate actual original project.delete cases")
    parser.add_argument("--marks", action="store_true", help="Generate original ordinary/split mark cases")
    parser.add_argument("--navigation", action="store_true", help="Generate original source monitor and mark navigation cases")
    parser.add_argument("--source-edits", action="store_true", help="Generate original source Insert/Overwrite cases")
    parser.add_argument("--subclips", action="store_true", help="Generate original subclip authoring/default cases")
    parser.add_argument("--attributes", action="store_true", help="Generate original paste/remove attributes cases")
    parser.add_argument("--replace", action="store_true", help="Generate original replacement cases")
    parser.add_argument("--video-options", action="store_true", help="Generate original video clip options cases")
    parser.add_argument("--media-modify", action="store_true", help="Generate original source media modification cases")
    parser.add_argument("--clip-edits", action="store_true", help="Generate original timeline clip edit cases")
    parser.add_argument("--match-reveal", action="store_true", help="Generate original match frame and reveal cases")
    args = parser.parse_args()
    if sum([args.apply_effects,args.rack_edits,args.set_param,args.keyframe_edits,args.animation,args.mask_commands,args.mask_track_batch,args.clip_rename,args.bin_rename,args.bin_edits,args.labels,args.label_group,args.selection,args.duplicate,args.project_delete,args.marks,args.navigation,args.source_edits,args.subclips,args.attributes,args.replace,args.video_options,args.media_modify,args.clip_edits,args.match_reveal])>1: parser.error("choose one oracle mode")
    reference = args.reference.resolve()
    assert subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip() == PIN
    assert not subprocess.check_output(["git", "-C", str(reference), "status", "--porcelain", "--untracked-files=no"], text=True).strip()
    with tempfile.TemporaryDirectory(prefix="filmcraft-effects-list-") as directory:
        build = Path(directory)
        (build / "src").mkdir()
        import json
        dependencies = "\n".join(f'filmcraft-{name}={{path={json.dumps(str(reference / "crates" / name))}}}' for name in ["engine", "project", "time", "media", "frame"])
        (build / "Cargo.toml").write_text('[package]\nname="filmcraft-effects-list-oracle"\nversion="0.1.0"\nedition="2024"\n[dependencies]\n' + dependencies + '\nserde_json="1"\nsha2="0.10"\n')
        (build / "src/main.rs").write_text((ROOT / ("scripts/filmcraft-match-reveal-oracle.rs" if args.match_reveal else "scripts/filmcraft-clip-edit-oracle.rs" if args.clip_edits else "scripts/filmcraft-media-modify-oracle.rs" if args.media_modify else "scripts/filmcraft-video-options-oracle.rs" if args.video_options else "scripts/filmcraft-replace-oracle.rs" if args.replace else "scripts/filmcraft-attributes-oracle.rs" if args.attributes else "scripts/filmcraft-subclip-oracle.rs" if args.subclips else "scripts/filmcraft-source-edit-oracle.rs" if args.source_edits else "scripts/filmcraft-navigation-oracle.rs" if args.navigation else "scripts/filmcraft-marks-oracle.rs" if args.marks else "scripts/filmcraft-project-delete-oracle.rs" if args.project_delete else "scripts/filmcraft-duplicate-oracle.rs" if args.duplicate else "scripts/filmcraft-selection-oracle.rs" if args.selection else "scripts/filmcraft-label-group-oracle.rs" if args.label_group else "scripts/filmcraft-label-oracle.rs" if args.labels else "scripts/filmcraft-bin-edit-oracle.rs" if args.bin_edits else "scripts/filmcraft-bin-rename-oracle.rs" if args.bin_rename else "scripts/filmcraft-clip-rename-oracle.rs" if args.clip_rename else "scripts/filmcraft-mask-track-batch-oracle.rs" if args.mask_track_batch else "scripts/filmcraft-mask-command-oracle.rs" if args.mask_commands else "scripts/filmcraft-animation-oracle.rs" if args.animation else "scripts/filmcraft-keyframe-edit-oracle.rs" if args.keyframe_edits else "scripts/filmcraft-set-param-oracle.rs" if args.set_param else "scripts/filmcraft-effect-apply-oracle.rs" if args.apply_effects else "scripts/filmcraft-effect-edit-oracle.rs" if args.rack_edits else "scripts/filmcraft-effects-list-oracle.rs")).read_text())
        env = dict(os.environ, KADR_SOURCE_ROOT=str(ROOT), CARGO_TARGET_DIR=str(ROOT / "backend/target/filmcraft-effects-list-oracle"), CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_BUILD_JOBS="2")
        subprocess.run(["cargo", "+" + args.toolchain, "run", "--offline", "--manifest-path", str(build / "Cargo.toml"), "--", str(ROOT / ("backend/tests/fixtures/filmcraft-match-reveal-oracle.json" if args.match_reveal else "backend/tests/fixtures/filmcraft-clip-edit-oracle.json" if args.clip_edits else "backend/tests/fixtures/filmcraft-media-modify-oracle.json" if args.media_modify else "backend/tests/fixtures/filmcraft-video-options-oracle.json" if args.video_options else "backend/tests/fixtures/filmcraft-replace-oracle.json" if args.replace else "backend/tests/fixtures/filmcraft-attributes-oracle.json" if args.attributes else "backend/tests/fixtures/filmcraft-subclip-oracle.json" if args.subclips else "backend/tests/fixtures/filmcraft-source-edit-oracle.json" if args.source_edits else "backend/tests/fixtures/filmcraft-navigation-oracle.json" if args.navigation else "backend/tests/fixtures/filmcraft-marks-oracle.json" if args.marks else "backend/tests/fixtures/filmcraft-project-delete-oracle.json" if args.project_delete else "backend/tests/fixtures/filmcraft-duplicate-oracle.json" if args.duplicate else "backend/tests/fixtures/filmcraft-selection-oracle.json" if args.selection else "backend/tests/fixtures/filmcraft-label-group-oracle.json" if args.label_group else "backend/tests/fixtures/filmcraft-label-oracle.json" if args.labels else "backend/tests/fixtures/filmcraft-bin-edit-oracle.json" if args.bin_edits else "backend/tests/fixtures/filmcraft-bin-rename-oracle.json" if args.bin_rename else "backend/tests/fixtures/filmcraft-clip-rename-oracle.json" if args.clip_rename else "backend/tests/fixtures/filmcraft-mask-track-batch-oracle.json" if args.mask_track_batch else "backend/tests/fixtures/filmcraft-mask-command-oracle.json" if args.mask_commands else "backend/tests/fixtures/filmcraft-animation-oracle.json" if args.animation else "backend/tests/fixtures/filmcraft-keyframe-edit-oracle.json" if args.keyframe_edits else "backend/tests/fixtures/filmcraft-set-param-oracle.json" if args.set_param else "backend/tests/fixtures/filmcraft-effect-apply-oracle.json" if args.apply_effects else "backend/tests/fixtures/filmcraft-effect-edit-oracle.json" if args.rack_edits else "backend/tests/fixtures/filmcraft-effects-list-oracle.json"))], env=env, check=True)
if __name__ == "__main__":
    main()
