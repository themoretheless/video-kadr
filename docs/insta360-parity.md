# Insta360 Studio comparison and spherical video

Target: installed Insta360 Studio 6.0.2, com.insta360.studio, main executable SHA-256 `6a100040c7e39d202b64e2fc0470e2f52600a39c4cde4e3fb1e92cea5ed76423`. Date: 2026-10-31. This is a bounded comparison, not a complete parity claim.

## Evidence and authority

REA Mach-O parent Evidence `ev_2dad431cc8d86dc0cdf87682695aa7f774ac5dedfc810f6f2989e30447165115` reports 70 dependencies, 8017 returned imports and 75111 returned exports. Import/export discovery is not exhaustive. The export names identify feature candidates, not execution or numerical correctness. REA Info.plist Evidence `ev_d68597216ebc51bb9d5aed5f2f779bc83888e611c8a76c9b7f1635c9827dde7a` declares INSV/LRV/INSP associations.

`qualification/insta360-2026-10-31/feature-symbols.json` selects 1819 matching symbols from the retained REA Mach-O Evidence. `resource-inventory.json` is a separate read-only directory parse of two Qt RCC v3 packages (661 files), with source hashes and byte ranges. It is not REA Evidence. No Insta360 code, assets, models or profiles are copied into the product.

The original MCP analysis hit the 330-second provider timeout and then the 450-second client timeout. The standalone default-profile CLI attempt then hit its 1800-second provider deadline. The completed private REA run uses a distinct reduced-coverage profile with Decompiler Switch Analysis, Basic Constant Reference Analyzer and Objective-C Message Analyzer disabled. It reports 133841 defined procedures, 288877 defined strings, 11 selected function decompilations and 19 retained Evidence records; these are not feature counts or proof of exhaustive discovery. No Codex configuration was changed. Status and output are in `qualification/insta360-2026-10-31/`.

## Completed native investigation

Whole-program overview Evidence: `ev_6b30aff4b5c477de0e83e4462dfe29f41d5f6dcd16f6ef38aba48e96fc56a213`. Full-context decompilations, the provider profile, Evidence export and provider-neutral snapshot are under `docs/qualification/insta360-2026-10-31/focused-rea/run-vvewc9c9`. The snapshot retains selected observations; it is not a persistent Ghidra project or a guarantee of arbitrary offline query replay.

Selected findings are static observations:

- The gyro parser constructor chooses 20-byte raw records or 56-byte motion records, and the selected raw `ReadNext` enforces the 20-byte stride. See parser Evidence in `analysis-summary.json`. The separate LLVM supplement also checks the 56-byte `ReadNext` variant.
- Conversion Evidence `ev_83baab164ae5414de566fc22ea7aa3e3eb8b3668ea3c3113fb5f06030265660d` calls separate acceleration and angular-rate conversion routines, consumes the three acceleration words before the three gyro words in the raw record, and converts raw timestamps using a 1,000,000 scale. That does not establish camera-axis calibration or correct video synchronization.
- Timing Evidence `ev_6632a9c2c59983fd3ae28be8421625a7121d32593938a410fc62d7c5cbf82fd3` includes segment offsets and optional exposure-related adjustments. A gyro timestamp alone is insufficient to reproduce this whole path.
- Lens parser and geometry-wrapper decompilations show multiple parameter representations. Raw calibration text is retained for further interpretation, rather than silently mapped into the generic equidistant lens parameters.

Eleven function-only subranges were independently analyzed through REA as well; `capsules/manifest.json` binds their byte ranges and SHA-256 digests to the original `LC_FUNCTION_STARTS` entries and arm64 mapping. Those derived views omit outside memory/calls and use a generic AArch64 ABI; they are supplementary to the full-context observations, and are not 11 additional distinct functions.

The initial static pass did not execute the target. The later four-feature follow-up runs an owned Studio reference export and reports its narrowly scoped image comparison below.

## Coverage

| Capability | Existing / new Video Kadr behavior | Remaining qualification or functionality |
|---|---|---|
| Timeline, trim, keyframes, color/audio/export | Existing editor infrastructure | Full Insta360 equivalence unproven |
| Spherical rotation / VR effects | Existing VR Projection and Rotate Sphere | Their presence is not perspective-reframe parity |
| 360 to perspective reframe | New `spherical_video`, animated pan/tilt/roll/FOV | Synthetic geometry and shared render-stack tests; real camera/UI/export footage pending |
| Tiny planet | New stereographic mapping | Synthetic uniform/alpha test; real footage pending |
| Dual fisheye stitching | New equidistant two-lens projection with per-lens centre/radius/FOV/rotation, overlap feathering in premultiplied linear color | Source-owned v6 profiles, dual-video-track packing and bounded subpixel optical flow are implemented; large/near-field parallax, temporal/depth refinement and separate-file pairing remain open |
| Gyro / world orientation stabilization | New camera-to-world quaternion sidecar import, shortest-arc SLERP, inverse camera orientation in renderer; calibrated rad/s integration in importer | Follow-up adds bounded X6 raw IMU decoding, capture/exposure sync and independent accelerometer fusion; Observed yaw-drift correction and rotational sensor-row rolling shutter are implemented in the later stage. Other camera mountings, factory IMU calibration, arbitrary timing and FlowState quality equivalence remain separate |
| Selected subject camera following | New bounded selected-texture tracking on equirectangular frames; longitude wrapping; camera key generation | Follow-ups add contrast reacquisition, saved SAM masks and opt-in neural identity recovery across occlusion, including raw dual lenses and gyro transforms. Neural recovery is bounded to60 samples/60seconds; DeepTrack model/quality equivalence remains unproven |
| Native INSV metadata | New bounded v3 index/ExtraMetadata parser, source-scoped API and inspector camera information; camera model, firmware, dimensions, frame rate, calibration declarations and opaque block inventory | Validated on 193 X6 files / four firmware strings. Other models, INSP and other trailer versions remain unqualified. Native raw gyro decoding and automatic v6 camera profile application are added by the follow-up; see its qualification ledger |
| AI tracking / stitching calibration internals | REA decompiled selected parser, gyro conversion, timing, tracker-options and camera-parameter functions | Multiple calibration representations and timing/exposure policies remain separate reconstruction tasks. The new point tracker and generic lens geometry do not establish DeepTrack/FlowState parity |

## Use

The nested composition inspector has **360° видео**. Add it as the first effect of a panorama clip. Choose source layout and output projection. Pan, tilt, roll and FOV support existing clip-local keyframe editing and undo/redo. Output retains the input working-frame raster; the composition canvas/clip transform determines final delivery size. A full 360 equirectangular result normally needs a 2:1 raster.

The default dual-fisheye lens values are an explicit side-by-side example, not an Insta360 camera profile. Calibrate the two equidistant lenses with the JSON control. Coordinates are normalized full-source coordinates; rotation is `[tiltX, panY, rollZ]` degrees. Both lenses use +Z as the optical axis and +Y up. The input must contain both lens views in one raster.

Orientation sidecar, version 1:

```json
{"version":1,"samples":[{"timeSeconds":0,"quaternion":[1,0,0,0]}]}
```

Quaternions are normalized camera-to-world `[w,x,y,z]`, with X right, Y up, Z forward according to the renderer convention. Time is seconds relative to the clip parameter clock. Orientation offset samples the sidecar at `clipTime + offset`. The renderer applies the inverse camera orientation to hold the chosen world-space view. Track ends clamp to the first/last orientation.

Alternatively import calibrated body-axis angular velocity in radians per second:

```json
{"version":1,"gyro":[{"timeSeconds":0,"radiansPerSecond":[0,0,0]},{"timeSeconds":0.01,"radiansPerSecond":[0,0.1,0]}]}
```

The importer uses trapezoidal angular increments with quaternion exponential updates, identity at the first sample, and normalization. This is basic gyro integration, without bias estimation or accelerometer correction. Camera-axis mapping and timestamps must be calibrated first. Unknown fields, mixed schemas, nonfinite data, non-unit quaternions, non-increasing times and >20000 samples are rejected. File imports are limited to 4 MiB.

Selected-texture following is available for the first spherical effect on eligible forward ordinary clips with local browser-decodable media, no effect-time override, speed ramp or gyro. Prepare the preview, select a contrast point, then generate pan/tilt keys. It uses bounded local/global correlation with an immutable identity anchor and wrapped longitude sampling. It can reacquire after a gap of up to2 seconds; unresolved loss, ambiguity or stale clip data leaves the existing keys unchanged. Sampling is limited to the first60 seconds/300 frames. Saved SAM object regions additionally drive native semantic camera following with optional smooth auto-framing. Generated tracking replaces the camera pan/tilt tracks with keys for the sampled interval; before/after that interval the standard keyframe extrapolation applies. Other camera channels are retained.

## Camera metadata

`GET /api/library/:id/insta360` uses the existing source-access policy and storage resolver. It reads the v3 footer/index and bounded metadata; it returns null when there is no matching footer, and rejects malformed bounds, overlap, header/index mismatch or unsupported metadata encoding. Serial-number, GPS, IP and creation-time fields are excluded. Calibration strings remain source declarations on this metadata endpoint. The separate `/insta360/profile` endpoint interprets qualified v6 omnidirectional calibration, rather than treating it as generic equidistant lenses. The nested inspector exposes **Сведения камеры** for an authenticated source.

The user-supplied corpus contained 203 INSV entries (not deduplicated by full-content hash): 193 recognized X6 trailers (firmware v1.0.20: 88; v1.1.7: 35; v1.2.35: 58; v1.2.36: 12), nine zero-byte entries, and one nonempty file without trailer magic. A native Rust read-only test validates all indexed block headers and selected protobuf fields in the 193 recognized files. This establishes metadata parser compatibility for that corpus, not camera video decode, image quality, gyro synchronization or support for every camera the user owns. Original files were not modified or copied into the repository.

## Validation

- `cargo check --lib`: passed, existing warnings retained.
- `cargo test --test insta360_metadata -- --include-ignored --nocapture` with the explicitly supplied private inventory: 3 passed, including 193 real trailers and one no-trailer file; zero-byte entries excluded.
- `cargo test --test spherical_video`: 7 passed (shared native render stack, serialization, longitude geometry, SLERP, lens blending, exact 180-degree rim coverage, alpha, cancellation and camera animation).
- Frontend sidecar/tracking tests: 5 passed, including a selected texture crossing the panorama seam and loss without partial key output.
- Inspector authoring tests: 127 passed, including the new effect and undo/redo.
- Svelte check: 0 errors / 0 warnings.
- ESLint on the new frontend files: passed.
- Chromium (installed Chrome Dev, isolated test profile) + real Rust API + uploaded FFV1 panorama: native preview, source metadata API and inspector, private-field exclusion, camera key, orientation sidecar, undo/redo and pixel checks passed. A subsequent test also exported a one-second native video job, downloaded it and decoded a frame to check the projected pixels; see `browser-export.log`. See `qualification/insta360-2026-10-31/browser-native-chrome.log` and `native-preview.png`. The initial attempt failed before execution because the Playwright bundled browser was absent; that failure log is retained.

These checks do not establish real-camera video decoding/render quality, broad export-profile coverage, broad browser pixel equivalence or GPU performance. Keep those separate from the passing synthetic checks. Existing worktree modifications were retained; nothing was committed or pushed by this change.

## Follow-up: automatic calibration, embedded IMU and semantic camera following

See [follow-up implementation and limits](qualification/insta360-2026-10-31/followup-implementation.md), [camera projection evidence](qualification/insta360-2026-10-31/camera-profile-rea/README.md), and [tracking qualification](qualification/insta360-2026-10-31/tracking-followup.md). This later work supersedes the earlier feature-boundary descriptions where explicitly stated. Automatic source-owned X6 v6 calibration, embedded IMU axes/exposure timing, native stabilization modes and semantic camera following/auto-framing are implemented and tested. A later [four-feature follow-up](qualification/insta360-2026-10-31/four-feature-followup.md) adds neural recovery, optical-flow stitching, sensor-row rolling shutter and drift correction, with linked native, neural and real Studio reference evidence. Full DeepTrack/FlowState quality parity remains unproven; the linked report gives the remaining camera, motion, appearance and seam-quality limits.

## Remaining quality qualification, 2026-11-01

See [close-object seams, hard pose recovery and additional cameras](qualification/insta360-2026-11-01/README.md). This stage adds source-owned X4 legacy projection/crop and retained trim-time mapping, while qualifying real Luna Ultra appearance changes separately. Studio preview/timeline settings disagree in the new reference session; export-mode attribution is being audited. Full vendor quality parity remains unproven.
