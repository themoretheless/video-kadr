#!/usr/bin/env python3
"""Extract source project metadata from the authoritative runtime catalog emitter."""
import argparse
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--runtime-catalog", type=Path, required=True)
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
source = json.loads(args.runtime_catalog.read_text())
assert source["sourceCommit"] == "5231852443363f001c3f6b396dd9b1e6461ae2be"
effects = [effect for effect in source["effects"] if effect["kind"] == "Audio"]
assert len(effects) == 58 and len({effect["id"] for effect in effects}) == 58
result = json.dumps({"schemaVersion": 1, "sourceCommit": source["sourceCommit"],
                     "effects": effects}, ensure_ascii=False, indent=2) + "\n"
target = Path(__file__).resolve().parents[1] / "backend/src/domain/filmcraft_audio_project_catalog.json"
if args.check:
    assert target.read_text() == result, "source project audio metadata differs"
else:
    target.write_text(result)
print("58 source project audio definitions match" if args.check else "58 source project audio definitions extracted")
