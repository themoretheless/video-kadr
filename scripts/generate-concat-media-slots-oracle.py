#!/usr/bin/env python3
"""Run the pinned, unmodified Concat project engine and compact its slot corpus."""
import argparse
import copy
import json
from pathlib import Path
import subprocess

PIN = "78e4be101c2d3c41a7dc73c112c6b1f11b9ce170"
ROOT = Path(__file__).resolve().parents[1]


def inputs():
    clip = dict(id="c", trackId="v", mediaId="slot", kind="video", name="Creator",
                start=3.5, duration=10, sourceStart=9, speed=2, audioStream=3,
                futureClip={"retain": 2})
    base = {"version": 1, "activeTimelineId": "active", "media": [
        dict(id="slot", name="Creator", path="/never/old", kind="video", duration=30,
             width=16, height=16, placeholder=True, origin="speech", colorRange="full",
             futureMedia={"retain": 1}),
        dict(id="other", name="Other", path="/never/other", kind="image")],
        "timelines": [dict(id=name, tracks=[dict(id="v", kind="video", name="V")],
                           clips=[] if name == "unused" else [clip])
                      for name in ["active", "shelved", "unused"]]}
    cases = []
    for kind in ["video", "audio", "image"]:
        for duration in [None, .25, 40]:
            for stream in [None, 0, 3]:
                for placeholder in [False, True]:
                    for media_id in ["slot", "other", "missing"]:
                        doc = copy.deepcopy(base)
                        doc["media"][0]["placeholder"] = placeholder
                        for timeline in doc["timelines"]:
                            for value in timeline["clips"]:
                                value["audioStream"] = stream
                        item = dict(path="/never/new", name="Replacement", kind=kind,
                                    duration=duration, width=8, height=6, frameRate=24,
                                    frameRateFraction="24/1", videoCodec="v", audioCodec="a",
                                    hasAudio=True, audioTracks=[], colorSpace="sdr", origin="processed")
                        cases.append(dict(document=doc, command=dict(op="fillSlot", mediaId=media_id, item=item)))
    for old in [False, True]:
        for flag in [False, True]:
            for media_id in ["slot", "missing"]:
                doc = copy.deepcopy(base)
                doc["media"][0]["placeholder"] = old
                cases.append(dict(document=doc, command=dict(op="setMediaPlaceholder", mediaId=media_id, placeholder=flag)))
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, default=ROOT / "backend/target/concat-reference")
    parser.add_argument("--output", type=Path, default=ROOT / "backend/tests/fixtures/concat-media-slots-oracle.json")
    parser.add_argument("--toolchain", default="1.98.1")
    args = parser.parse_args()
    reference = args.reference.resolve()
    actual = subprocess.check_output(["git", "-C", str(reference), "rev-parse", "HEAD"], text=True).strip()
    if actual != PIN:
        raise SystemExit(f"Reference pin mismatch: {actual}")
    dirty = subprocess.check_output(["git", "-C", str(reference), "status", "--porcelain", "--", "src/crates/concat-project", "src/crates/concat-core"], text=True)
    if dirty.strip():
        raise SystemExit("Reference project/core sources are modified")
    work = ROOT / "backend/target/concat-slot-oracle"
    (work / "src").mkdir(parents=True, exist_ok=True)
    dependency = reference / "src/crates/concat-project"
    (work / "Cargo.toml").write_text('[package]\nname="concat-slot-oracle"\nversion="0.0.0"\nedition="2024"\n[dependencies]\nconcat-project={path=' + json.dumps(str(dependency)) + '}\nserde_json="1"\n')
    (work / "src/main.rs").write_text((ROOT / "scripts/concat-media-slots-oracle.rs").read_text())
    source, output = work / "inputs.json", work / "results.json"
    source.write_text(json.dumps(inputs()))
    subprocess.run(["cargo", "+" + args.toolchain, "run", "--manifest-path", str(work / "Cargo.toml"), "--", str(source), str(output)], check=True)
    fixture = json.loads(output.read_text())
    documents, afters = [], []
    for case in fixture["cases"]:
        for key, pool in [("document", documents), ("after", afters)]:
            value = case.pop(key)
            if value not in pool:
                pool.append(value)
            case[key + "Index"] = pool.index(value)
    fixture.update(documents=documents, afters=afters)
    args.output.write_text(json.dumps(fixture, separators=(",", ":")) + "\n")
    print(f"{len(fixture['cases'])} cases -> {args.output.stat().st_size} bytes")


if __name__ == "__main__":
    main()
