# Observable gyro drift correction

This implementation adds native calibration and image-observed relative heading correction. It does not infer an absolute compass direction or classify every slow yaw as sensor bias.

## Evidence boundary

Source-bound REA capsules in `gyro-drift/` target the installed Studio6.0.2 executable with the original SHA256 recorded in the manifest. `InertialStaticDetection::StaticDetection`, Evidence `ev_77f22d8c8806d900d313d3863dd770f3b5bd3b049c770d9c8f15c3b67fe917c4`, maintains acceleration/angular-rate means and variances. `getGyroBias`, Evidence `ev_53ae77317b556d01e016871e1c22e80cb872a82f2244f5eea15e832b6acb7851`, returns the stored three-vector. Function-only analysis omits external constants/calls; these observations do not establish native thresholds or numerical parity. No vendor implementation is embedded into product code.

A bounded read-only check of the three smallest valid local X6 files found no ExtraMetadata field31 (`gyro_calib`). Its bytes are not guessed as a matrix or automatically applied. Factory calibration coverage on other files/models remains separate.

## Request and response

`GyroRequest` adds:

- `driftCorrection.biasRadiansPerSecond`: explicit calibrated SENSOR-axis bias, in radians/second. Raw returned samples retain original values; subtraction occurs only in the integration input.
- `driftCorrection.stationaryIntervals`: caller-declared clip-relative start/end intervals. At least25 samples, stable approximately1g acceleration, low angular-rate variance and bounded bias magnitude are required. Explicit bias and stationary intervals are mutually exclusive. A steady slow pan is not automatically treated as stillness.
- `driftCorrection.headingAnchors`: at least two independently observed clip-relative world headings in degrees. Angles unwrap around the integrated path, so genuine turns remain present. Anchors and the visual option are mutually exclusive.
- `visualDriftCorrection`: opt-in source-video heading observation service.
- `orientationToleranceDegrees`: adaptive SO(3) output. Native gyro samples and actual source exposure/shutter clock breakpoints are retimed before compression. The default uniform diagnostic sensor grid remains separate.
- `motionPaddingSeconds`: explicit observed IMU guard up to0.2s; it requires adaptive output and sufficient actual capture data. Returned `orientationOffsetSeconds` carries the shift. With gravity disabled, the requested clip start remains the relative world origin, not the pre-roll start.

`driftCorrection` in the response reports source, applied bias, calibration sample count, heading observation count, sampled video frames, admitted/rejected pairs, final correction and warnings. `sourceFrameClock` retains the qualified rational source presentation rate and source in-point. `captureClock` retains the physical piecewise-linear exposure clock, including actual known pre/post-trim events. It is compressed to1ns knot error rather than approximated by the diagnostic output frequency. Outside the real video only the guard-band coordinate is extended affinely with an explicit warning; actual IMU observations still bracket it.

## Automatic image observations

`services::insta360_visual_drift::analyze_path` runs inside the caller's existing source authorization, Analysis permit and blocking worker. It reads the source only. A cancellation monitor propagates the request token into the existing owned frame decoder, with20s per decode and180s total limits.

At most32 exact held source frames, targeting0.5s temporal spacing, are sampled across the requested source interval. Qualified X6 calibration packs both lens streams and projects each frame into an IMU-predicted world panorama. Thus intentional camera yaw/pitch/roll are already removed before matching. The shared native subpixel optical-flow matcher uses a sparse grid for temporal analysis; the ordinary seam correction grid is unchanged.

A robust spherical SO(3) fit uses deterministic two-ray RANSAC followed by confidence-weighted least squares, allowing residual tilt without confusing it with yaw. Only the resulting world-Y twist is used; the gravity estimate remains unchanged. A yaw-only distributed fit is a fallback. If rigid rotation fails, calibrated spherical eight-point essential-matrix RANSAC separates translation from rotation. Its bounded512 hypotheses require at least50 rays,70 percent epipolar consensus, an independently ranked nonplanar design,85 percent positive-depth cheirality with an unambiguous solution, median parallax at least0.7°, and four leave-quarter-out fits agreeing in yaw within0.15°. Pure rotation, planar and low-parallax scenes decline this branch and retain the rigid fallback. An observation requires at least30 retained correspondences,60 percent consensus, at least6 of12 longitude sectors and2 latitude bands. Large vertical disagreement, high angular residuals, weak/ambiguous texture and a localized moving foreground fail admission. The correction rate is limited to0.5degrees/second. A fixed accepted reference avoids summing matching noise; an admitted previous reference can bridge a larger accumulated drift. Unobserved ends hold the nearest correction instead of inventing an unbounded heading extrapolation.

The actual correction is a world-Y rotation. Gravity alignment and observed intentional motion remain present. Every heading observation is inserted as an orientation knot, even when a constant gyro path compressed to two keys. Noncommuting camera rotation and heading correction receive additional curvature-control samples before re-compression. The combined input/compression knot budget is reported; this is not a continuous-time truth certificate between sensor measurements.

## Limits

The automatic path assumes a spatially distributed static background. Strong translation/parallax, moving backgrounds, insufficient texture, unqualified source timing or missing camera calibration can prevent an observation. The response leaves the IMU heading unchanged and reports missing consensus rather than resetting yaw. Absolute magnetic heading, unknown factory-calibration byte formats and proprietary FlowState fusion/tuning are not reconstructed. Root integration supplies calibrated source-row rolling shutter separately.

## Validation

Focused results are recorded in the associated drift test logs. Tests include declared stationary bias versus an untouched constant intentional pan, observed-heading correction across multiple turns, invalid calibration, sparse distributed consensus versus foreground/flat images, fractional image motion, interior heading knots in a two-key gyro path, source clock/padding/clip-origin regressions and encoded dual-lens source-video integration. Final encoded/real-source outcomes are appended after execution.


### Final controlled evidence (2026-10-31)

`gyro-drift/offline-so3-qualified.log` records seven drift-math tests, fourteen gyro/clock tests and two service tests passing. The service test decodes actual lossless encoded dual-lens MP4 tracks with source-bound X6 calibration and embedded gyro/exposure metadata. An injected stationary heading error fell from1.953125° to0.026259° (98.66 percent reduction); a genuine constant camera pan remained13.671875° before and after correction. This is controlled end-to-end native evidence, not a proprietary Studio comparison or a claim about arbitrary scenes.

The authorized real room source produced150 matches over0.48s, but only39 of141 qualified rays agreed with a rigid rotation within0.65°; the required60 percent consensus was absent. The automatic service correctly declined correction. This diagnostic justified adding translation-aware geometry rather than relaxing rigid gates. An independent NumPy assessment admitted134/141 rays, with133 positive-depth rays and1.484° median parallax. The native essential implementation admitted127/141 rays over all12 longitude sectors and3 latitude bands at0.1446° epipolar RMS. These geometry observations alone do not establish end-to-end drift reduction on this capture.

The optional193-file IMU guard audit was interrupted by a pCloud FUSE disconnect. It did not finish or establish corpus-wide preroll/postroll coverage; no production default may assume all captures have0.1s guards. Normal gyro analysis can use precision output without padding; row correction must request and validate actual guard samples.


The second actual X6 source (garage) admitted one background observation: with an additional injected0.114592° gyro error, independent source-image residual fell from0.235457° to0.033095° (85.94 percent reduction). See `gyro-drift/real-garage-admission.log`. The room run with the earlier rigid-only build safely left orientation byte-for-byte unchanged; its log is explicitly fallback evidence, not a drift-removal pass.


The final translation-aware integrated build passed8 drift tests,14 gyro/exposure-clock tests and2 actual encoded-source service tests (`gyro-drift/offline-essential-qualified.log`). At0.5s sampling cadence,17 source frames/16 accepted observations retained the same controlled stationary reduction and exactly preserved the genuine13.671875° pan. The new numerical regression explicitly verifies translation with known rotation and rejects pure rotation, planar geometry, distant low-parallax scenes and cancellation. The solver adds no external numerical dependency.


**Moving real-source result after translation-aware integration:** the room capture passed with6 sampled frames,3 admitted and2 rejected temporal pairs. With an additional injected0.568374° heading error, independently reprojected source-image residual decreased from0.826750° to0.162333° (80.36 percent). See `gyro-drift/real-room-essential-qualified.log`. Rejected intervals retained the stated interpolation/hold behavior; neither confidence nor correction-rate limits were relaxed to obtain admission. This supersedes the earlier rigid-only unchanged fallback result for this source.


The same final essential/cadence build also passed the actual garage regression with visual admission required, retaining0.235457°→0.033095° residual (`gyro-drift/real-garage-essential-qualified.log`). Both final real-source checks used the authorized read-only pCloud originals sequentially. No claim is made that these sampled relative-image measurements reproduce proprietary fusion numerics or qualify other camera models.
