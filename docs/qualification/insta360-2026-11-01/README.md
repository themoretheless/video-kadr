# Insta360 quality qualification, 2026-11-01

See the [2026-11-02 follow-up](../insta360-2026-11-02/README.md) for fresh production renders, the separate failing strong-pose recovery gate, sampling changes and current verification boundaries. This document retains the earlier stage's evidence.

Full DeepTrack/FlowState quality parity is **not established**. This stage separates implemented source handling, measured quality and unsupported cases. Current safety fixes require the final coherent verification recorded below; older passing runs are not silently attributed to later code.

## Camera and clock reconstruction

- Strict X4 legacy lens71 projection, compact non-ordinal metadata indexing and sensor-window crop are implemented. The actual8000×6000→5760² per-lens crop yields11520×5760 active full calibration reference; stretching raw16000×6000 calibration onto decoded square tracks was incorrect. See [camera evidence](camera-formats/README.md).
- The retained native X4 time map shifts original IMU/exposure time by21seconds while preserving decoded100fps frame phase. Unknown retiming remains rejected. See [gyro evidence](cross-camera-gyro/README.md).
- Automatic gravity feedback previously admitted dynamic forces from their near1g magnitude alone, causing large false tilt. It now requires observed quiet windows, small innovation against gyro-only integration and a total0.25degree correction budget. Explicit stationary calibration remains available for larger actual sensor bias.
- Horizon mode previously projected a nearly vertical camera-forward axis, creating artificial heading swings. Conditioned heading charts preserve phase through vertical poses and return to forward heading when observable. This independent convention is not a reconstruction of the vendor heading algorithm.
- The actual short X4 pre-roll force estimate is **estimated**, not precise absolute gravity. No half-second quiet window was found in a five-second prehistory diagnostic. Usable relative orientation remains available with an explicit warning.

The user's Media directory includes GO Ultra and Luna Ultra in ordinary MP4 files, not only X6 INSV. See [selected evidence](selected-camera-inventory.json). GO/Luna single-lens metadata and Luna tracking are separate from dual-fisheye panoramic support. This is not an exhaustive model census.

The additional X4 original comes from the [publisher-linked Insta360 X4 sample collection](https://www.reddit.com/r/Insta360/comments/1c5fkj9/insta360_x4_mega_thread/). Verified length214039326bytes; SHA256 `f3274d265e03f4f23ff5990df06b758a75129f19fe43b949d96edd9f8eb50009`. Partial transfer/footer failure was detected and corrected with validated HTTP range overlap. Originals, personal frames and masks remain outside the repository.

## Controlled Studio references

Studio6.0.2 was used with backed-up, explicitly owned temporary projects. Preview controls and delayed project persistence originally disagreed; [historical settings snapshots](reference-settings-audit.json) retain that uncertainty. Historical exports are not retroactively assigned verified modes.

New references use explicit serialized clip settings, project reload, corroborating UI, actual export and post-export JSON verification:

- X6 office and X4 outdoors: FlowState off, OpticalFlow on, AI/Fusion off. See [controlled settings and frame hashes](../insta360-2026-10-31/controlled-reference-off-20261101.json).
- X4: FlowState on, Direction Lock off, AI/Fusion on, OpticalFlow off. See [controlled best-mode settings](controlled-x4-on-ai-reference.json).

X4 captures100fps but Studio exports25fps slow playback: source0/1/3seconds correspond to output0/4/12seconds. Office uses the same source/output times. All quality tables reuse a single initial feather-derived spherical rotation within each source; later frames and flow variants are never independently fitted. Per-frame diagnostic refits are excluded from quality claims.

## Seam quality

Bounded multiscale correspondence, lens-rim coverage checks and a separate midpoint render field are implemented. Supported triangulation improves the synthetic overlap MSE from prior404.83368 to365.22465 (feather1854.90451). It does not establish recovery in untextured holes. See [native and real-image follow-up](../insta360-2026-10-31/optical-near-object-followup-20261101.md).

The actual very close X6 forearm still ghosts. At the historical fixed alignment, current/prior near-object PSNR is27.483930/27.484255dB: no meaningful improvement. Only two reliable, mutually inconsistent anchors were observed in that flat-hand region. Experimental graph ownership still creates a forearm splice and regresses the measured controls; occlusionStrength remains0by default. General edge-aware hole completion and temporal seam consistency remain open.

Controlled OpticalFlow/off comparisons, using the same fixed initial rotation, show mixed small changes rather than broad quality parity:

| Source | Capture seconds | Feather dB | Prior flow dB | Current flow dB |
|---|---:|---:|---:|---:|
| office | 0 | 28.747 | 28.918 | 28.924 |
| office | 1 | 28.738 | 29.018 | 29.036 |
| office | 3 | 28.380 | 28.553 | 28.541 |
| x4 | 0 | 25.512 | 25.475 | 25.458 |
| x4 | 1 | 24.505 | 24.274 | 24.438 |
| x4 | 3 | 25.144 | 25.080 | 25.098 |

See [hashed measurements](verified-off-comparisons.json). These pixel metrics include calibration, resampling and color differences; they are not perceptual quality ground truth. Classic Studio OpticalFlow also visibly differs from its cleaner AI stitching. The [eight-frame matrix](camera-matrix-final.json) proves calibrated decode/render coverage across three sources with opaque output; it does not certify parity.

## Subject identity

See [identity qualification](reidentification/README.md). Source/hash-bound safe-view enrollment, exposure-tolerant chromaticity and measured RGB continuity are implemented. Strong pose testing uses actual Luna Ultra front→back→front footage with a second helmeted person.

The candidate runtime `e45114190893627fe9295d95c49ba8602faea396d77ecca1a9a55a1307a48548` is **rejected** as final acceptance: its actual hard case recovered a wrong close-up mask and accepted a different person; the actual neural run passed4/6cases. Passing its three HTTP scenarios does not excuse these identity errors. A separate immutable, instance-local appearance/geometric recovery guard is being qualified. Unseen back views must remain unqualified if identity evidence is insufficient; thresholds are not relaxed to force recovery.

## Verification boundaries

- The initial coherent backend run passed81tests in13integration suites; three selected actual-X4 tests passed separately. See [initial summary](native-final-summary.json), [log](native-final.log) and [original-source tests](x4-original-final.log). These precede the Horizon and bounded-feedback safety fixes.
- Current exact-source-module checks pass27gyro tests, including3actual-source cases, plus6Horizon heading checks. See [current focused gyro log](cross-camera-gyro/bounded-gravity-source-tests.log). The final coherent backend/render rerun is pending.
- The rejected runtime's HTTP3/3and real child-cancellation1/1checks passed. Cancellation stopped after learned inference without publishing output or retaining private staging. These must be repeated for its accepted replacement.
- Full-library unit compilation is blocked by unrelated metadata/transition fixtures missing caption_text_tracks. The exact current neural-budget function and its existing test module separately compile and pass1test. See [compiler errors](reid-budget-final.log) and [scoped source test](reid-budget-source-final.log).
- Current UI coverage validates27unique tests:20component/video tests pass, and7tracking tests pass in an isolated rerun. The initial combined run hit the default5second wall timeout in one tracking case under host contention; the isolated rerun used a30second harness limit and completed its tests in1.06seconds. This is not a latency certificate. Full Svelte check reports0errors/0warnings; scoped ESLint passes. Full-workspace ESLint still reports644errors elsewhere. See [combined run](frontend-safety-final.log), [tracking rerun](tracking-loaded-host-rerun.log), [typecheck](typecheck-safety-final.log) and [scoped lint](lint-safety-final.log).

No commit, push, global model configuration change or full installed-release qualification is claimed.
