#!/usr/bin/env python3
"""Generate native authoring traces and check Rust sampling, geometry and MCP schemas."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
TARGET = ROOT / "backend" / "target"


def main():
    TARGET.mkdir(parents=True, exist_ok=True)
    output = Path(tempfile.mkdtemp(prefix="native-scalar-tangents-", dir=TARGET))
    oracles = {"NATIVE_SCALAR_TANGENT_ORACLE": output / "ui-curve.json",
               "NATIVE_SCALAR_GRAPH_P1_ORACLE": output / "ui-graph-p1.json",
               "NATIVE_SCALAR_GRAPH_P2_ORACLE": output / "ui-graph-p2.json",
               "NATIVE_PATH_PROGRESS_ORACLE": output / "ui-path-progress.json",
               "NATIVE_PATH_GEOMETRY_ORACLE": output / "ui-path-geometry.json",
               "NATIVE_PATH_SPATIAL_ORACLE": output / "ui-path-spatial.json",
               "NATIVE_POSITION_CONVERSION_CLOCK_ORACLE": output / "ui-position-conversion-clock.json",
               "NATIVE_SCALAR_TIMELINE_ORACLE": output / "ui-scalar-timeline.json",
               "NATIVE_SCALAR_TIMELINE_SCALE_ORACLE": output / "ui-scalar-timeline-scale.json",
               "NATIVE_SCALAR_TIMELINE_PASTE_ORACLE": output / "ui-scalar-timeline-paste.json",
               "NATIVE_SCALAR_TIMELINE_GRAPH_ORACLE": output / "ui-scalar-timeline-graph.json",
               "NATIVE_SCALAR_TIMELINE_TANGENT_ORACLE": output / "ui-scalar-timeline-tangent.json",
               "NATIVE_SCALAR_TIMELINE_VIEWPORT_ORACLE": output / "ui-scalar-timeline-viewport.json",
               "NATIVE_SCALAR_TIMELINE_AUTOPAN_ORACLE": output / "ui-scalar-timeline-autopan.json",
               "NATIVE_SCALAR_TIMELINE_MARQUEE_AUTOPAN_ORACLE": output / "ui-scalar-timeline-marquee-autopan.json",
               "NATIVE_SCALAR_TIMELINE_GRAPH_AUTOPAN_ORACLE": output / "ui-scalar-timeline-graph-autopan.json",
               "NATIVE_EFFECT_SCALAR_UI_ORACLE": output / "ui-effect-scalar.json",
               "NATIVE_EFFECT_COLOR_SCALAR_UI_ORACLE": output / "ui-effect-color-scalar.json",
               "NATIVE_SOURCE_RUN_GRAPH_ORACLE": output / "ui-source-run-graph.json",
               "NATIVE_SOURCE_EMPTY_GRAPH_ORACLE": output / "ui-source-empty-graph.json",
               "NATIVE_SOURCE_GROUP_GRAPH_ORACLE": output / "ui-source-group-graph.json",
               "NATIVE_SOURCE_CLIPBOARD_GRAPH_ORACLE": output / "ui-source-clipboard-graph.json",
               "NATIVE_LUMETRI_SCALAR_UI_ORACLE": output / "ui-lumetri-scalar.json"}
    env = {**os.environ, **{key: str(path) for key, path in oracles.items()},
           "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0",
           "CARGO_TARGET_DIR": str(TARGET)}
    commands = [
        ("ui.log", ["npm", "--prefix", "frontend", "run", "test", "--",
                    "--configLoader", "runner",
                    "src/lib/components/composition/NestedScalarKeyInspector.test.ts",
                    "src/lib/composition/nativeEffectScalar.test.ts",
                    "src/lib/components/composition/NestedSourceRunGraph.test.ts",
                    "src/lib/composition/sourceRunSelection.test.ts",
                    "src/lib/composition/sourceRunClipboard.test.ts",
                    "src/lib/composition/sourceRunCutTransaction.test.ts",
                    "src/lib/components/composition/NestedConcatGainInspector.test.ts",
                    "src/lib/composition/concatSourceWindow.test.ts",
                    "src/lib/components/composition/NestedEffectScalarTimeline.test.ts",
                    "src/lib/components/composition/NestedMotionPathInspector.test.ts",
                    "src/lib/components/composition/NestedMotionPathOverlay.test.ts",
                    "src/lib/composition/positionConversion.test.ts",
                    "src/lib/components/composition/CompositionPositionConversion.test.ts",
                    "src/lib/components/composition/NestedPositionConversion.test.ts",
                    "src/lib/components/composition/NestedScalarTimeline.test.ts",
                    "src/lib/composition/nativeScalarTimeline.test.ts",
                    "src/lib/composition/keyframeSelection.test.ts",
                    "src/lib/composition/keyframeClipboard.test.ts",
                    "src/lib/composition/nativeScalarClipboard.test.ts",
                    "src/lib/components/composition/NestedScalarClipboard.test.ts",
                    "src/lib/agent/nativeBatchRuntime.test.ts",
                    "src/lib/components/composition/NestedScalarGraph.test.ts",
                    "src/lib/composition/keyframeCurve.test.ts",
                    "src/lib/components/composition/NestedScalarViewport.test.ts",
                    "src/lib/composition/nativeScalarViewport.test.ts"]),
        ("conversion.log", ["cargo", "+1.89.0", "test", "--manifest-path",
                            "backend/vendor/weftcut-conversion/Cargo.toml"]),
        ("rust.log", ["cargo", "+1.89.0", "test", "--manifest-path", "backend/Cargo.toml",
                      "--test", "native_scalar_tangent_ui", "--test", "native_position_conversion_clock", "--test", "mcp_native_batch_catalog",
                      "--", "--include-ignored"]),
    ]
    inputs = [
        "frontend/src/lib/components/composition/NestedConcatGainInspector.svelte",
        "frontend/src/lib/components/composition/NestedConcatGainInspector.test.ts",
        "frontend/src/lib/composition/sourceRunCutTransaction.ts",
        "frontend/src/lib/composition/sourceRunCutTransaction.test.ts",
        "frontend/src/lib/composition/sourceRunClipboard.ts",
        "frontend/src/lib/composition/sourceRunClipboard.test.ts",
        "frontend/src/lib/state/sourceRunClipboard.svelte.ts",
        "frontend/src/lib/composition/sourceRunSelection.ts",
        "frontend/src/lib/composition/sourceRunSelection.test.ts",
        "frontend/src/lib/components/composition/SourceKeyRunGraph.svelte",
        "frontend/src/lib/components/composition/NestedSourceRunGraph.test.ts",
        "frontend/src/lib/composition/concatSourceWindow.ts",
        "frontend/src/lib/composition/concatSourceWindow.test.ts",
        "frontend/src/lib/composition/nativeEffectScalar.ts",
        "frontend/src/lib/composition/nativeEffectScalar.test.ts",
        "frontend/src/lib/composition/fixtures/nativeEffectScalarProject.ts",
        "frontend/src/lib/components/composition/NestedEffectScalarTimeline.test.ts",
        "frontend/src/lib/composition/nativeScalarHistoryView.ts",
        "frontend/src/lib/composition/nativeScalarViewport.ts",
        "frontend/src/lib/composition/nativeScalarViewport.test.ts",
        "frontend/src/lib/components/composition/NestedScalarViewport.test.ts",
        "frontend/src/lib/components/composition/NestedScalarGraph.svelte",
        "frontend/src/lib/components/composition/NestedScalarGraph.test.ts",
        "frontend/src/lib/composition/keyframeCurve.ts",
        "frontend/src/lib/composition/keyframeCurve.test.ts",
        "frontend/src/lib/components/composition/NestedScalarClipboard.svelte",
        "frontend/src/lib/components/composition/NestedScalarClipboard.test.ts",
        "frontend/src/lib/composition/nativeScalarClipboard.ts",
        "frontend/src/lib/composition/nativeScalarClipboard.test.ts",
        "frontend/src/lib/composition/fixtures/nativeScalarClipboardProject.ts",
        "frontend/src/lib/state/nestedScalarClipboard.svelte.ts",
        "frontend/src/lib/composition/keyframeClipboard.ts",
        "frontend/src/lib/composition/keyframeClipboard.test.ts",
        "frontend/src/lib/agent/nativeBatchRuntime.test.ts",
        "frontend/src/lib/components/composition/NestedScalarTimeline.svelte",
        "frontend/src/lib/components/composition/NestedScalarTimeline.test.ts",
        "frontend/src/lib/composition/nativeScalarTimeline.ts",
        "frontend/src/lib/composition/nativeScalarTimeline.test.ts",
        "frontend/src/lib/agent/nativeKeyframes.ts",
        "frontend/src/lib/composition/keyframeSelection.ts",
        "frontend/src/lib/composition/keyframeSelection.test.ts",
        "frontend/src/lib/composition/timeGrid.ts",
        "backend/Cargo.toml",
        "backend/Cargo.lock",
        "backend/vendor/weftcut-conversion/Cargo.toml",
        "backend/vendor/weftcut-conversion/Cargo.lock",
        "frontend/src/lib/components/composition/NestedPositionConversion.svelte",
        "frontend/src/lib/components/composition/NestedPositionConversion.test.ts",
        "frontend/src/lib/composition/nativePositionConversion.ts",
        "backend/src/handlers/position_conversion.rs",
        "backend/vendor/weftcut-eval/src/lib.rs",
        "backend/vendor/weftcut-eval/src/path.rs",
        "backend/vendor/weftcut-conversion/src/temporal.rs",
        "backend/vendor/weftcut-conversion/src/spatial.rs",
        "frontend/src/lib/components/composition/CompositionPositionConversion.svelte",
        "frontend/src/lib/components/composition/CompositionPositionConversion.test.ts",
        "frontend/src/lib/composition/positionConversion.ts",
        "frontend/src/lib/composition/positionConversion.test.ts",
        "frontend/src/lib/api.ts",
        "backend/vendor/weftcut-conversion/src/lib.rs",
        "backend/vendor/weftcut-conversion/src/tests.rs",
        "backend/tests/native_position_conversion_clock.rs",
        "frontend/src/lib/components/composition/NestedMotionPathOverlay.svelte",
        "frontend/src/lib/components/composition/NestedMotionPathOverlay.test.ts",
        "frontend/src/lib/components/composition/CompositionWorkspace.svelte",
        "frontend/src/lib/state/nestedPathOverlay.svelte.ts",
        "frontend/src/lib/components/composition/NestedScalarKeyInspector.svelte",
        "frontend/src/lib/components/composition/NestedMotionPathInspector.svelte",
        "frontend/src/lib/components/composition/NestedMotionPathInspector.test.ts",
        "frontend/src/lib/state/nestedComposition.svelte.ts",
        "frontend/src/lib/components/composition/GraphicKeyTangents.svelte",
        "frontend/src/lib/components/composition/NativeScalarBezierEditor.svelte",
        "frontend/src/lib/components/composition/BezierControlEditor.svelte",
        "frontend/src/lib/components/composition/NestedCompositionPreview.svelte",
        "frontend/src/lib/state/nestedEffectPreview.svelte.ts",
        "frontend/src/lib/components/composition/NestedScalarKeyInspector.test.ts",
        "frontend/src/lib/agent/nativeKeyframes.ts",
        "frontend/src/lib/agent/nativePath.ts",
        "frontend/src/lib/composition/tangents.ts",
        "frontend/src/lib/composition/keyframes.ts",
        "frontend/src/lib/composition/keyframeClock.ts",
        "backend/src/domain/keyframes.rs",
        "backend/src/domain/native_keyframe_clock.rs",
        "backend/src/domain/motion_path.rs",
        "backend/src/domain/nested_composition.rs",
        "backend/src/domain/flat_surface.rs",
        "backend/src/domain/animation_sample_time.rs",
        "backend/tests/native_scalar_tangent_ui.rs",
        "backend/src/mcp.rs",
        "backend/tests/mcp_native_batch_catalog.rs",
        "scripts/qualify-native-scalar-tangents.py",
    ]
    inputs.extend(str(path.relative_to(ROOT)) for path in sorted((ROOT / "frontend/src/lib/components/composition").glob("Concat*SourceKeyEditor.svelte")))
    hashes = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in inputs}
    (output / "inputs-before.json").write_text(json.dumps(hashes, indent=2) + "\n")
    print(f"Evidence: {output}", flush=True)
    for log, command in commands:
        with (output / log).open("wb") as stream:
            command_env = {**env, "CARGO_TARGET_DIR": str(TARGET / "conversion-clock")} if log == "conversion.log" else env
            result = subprocess.run(command, cwd=ROOT, env=command_env, stdout=stream,
                                    stderr=subprocess.STDOUT, check=False)
        if result.returncode:
            raise SystemExit(f"Failed ({result.returncode}): {output / log}")
    current = {name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in inputs}
    if hashes != current:
        (output / "inputs-after.json").write_text(json.dumps(current, indent=2) + "\n")
        changed = ", ".join(name for name in inputs if hashes[name] != current[name])
        raise SystemExit(f"Source changed during qualification: {changed}; {output}")
    hashes.update({path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in oracles.values()})
    (output / "evidence.json").write_text(json.dumps({"status": "passed", "inputs": hashes,
        "commands": [command for _, command in commands]}, indent=2) + "\n")
    print("Mounted UI edit and Rust sampling passed.", flush=True)


if __name__ == "__main__":
    main()
