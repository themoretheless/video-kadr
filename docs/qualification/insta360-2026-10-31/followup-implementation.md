# Camera, IMU and semantic-following follow-up

For the later neural recovery, optical-flow stitching, rolling-shutter and drift stage, see [four-feature follow-up](four-feature-followup.md); it supersedes the corresponding absent-feature statements below.

This follow-up extends the previous bounded implementation. It does not establish numerical equivalence to Studio's proprietary FlowState or DeepTrack models.

## Source-owned camera profiles

ExtraMetadata fields111/112 contain X6 v6 calibration. `insta360_camera_profile` parses the exact two-lens records, preserves the 13 distortion terms, reference dimensions and poses, validates supported version/lens type and rejects unknown calibration. No model-name/default-radius substitution is used. The native spherical renderer applies the omnidirectional model and calibrated lens poses; `sourceLayout=dual_video_streams` makes the shared native source decoder combine the first two video streams before effects. Ordinary source decoding keeps its previous single-stream behavior.

`GET /api/library/:id/insta360/profile` applies the source access policy, reads the bounded trailer and probes the declared stream dimensions/clocks before enabling automatic pairing. The inspector applies the calibrated profile and pairing in one history entry. Explicit packed/manual lens controls remain available. Changing to panorama input clears the incompatible camera profile; editing generic lenses clears the camera profile.

REA source-bound native projection and pose evidence, limitations and hashes are in `camera-profile-rea/README.md`. Five native tests passed; the separately opted-in corpus test extracted193/193 profiles. A low-resolution real X6 dual-track frame rendered a full panorama with32768/32768 opaque and nonblack pixels. This is static geometric stitching with feathering, without optical flow, finite-depth parallax correction or per-row rolling shutter. Personal derived frame artifacts are kept outside the repository.

## IMU and stabilization

`insta360_gyro` bounds raw block reads, decodes X6 offset-binary acceleration and angular velocity using metadata ranges, and reconstructs a requested clip window on the source capture clock. Ordinary pts_type2 exposure records enable linear interpolation of exposure midpoints. The native MP4 clock reader proves a constant sample delta, compatible composition offsets/edit list, and matching lens-track clocks/sample counts; exact rational rates such as30000/1001 are retained instead of trusting nominal integer metadata. Unproved timing falls back to capture-clock alignment with an explicit timingWarning. Other timing representations, arbitrary PTS lists, multi-segment/timelapse retiming and rolling shutter remain unqualified.

Calibrated body-rate integration and accelerometer correction run in Rust before bounded output resampling. With a declared sensor-to-camera calibration, initial gravity establishes world up; accelerometer updates are rejected under strong linear acceleration. There is no magnetometer heading reference or guaranteed yaw-drift cancellation. Mounting is automatic only for the qualified X6 raw type1, v6 flags0x60400 and lens193 path. Its native gyro enum145, actual map ray callback and renderer basis give gyro→camera [-Z,X,-Y] and specific acceleration→camera [Z,-X,Y]. The ARV quaternion bridge is an output hint and is not applied to the native map input. The captured flow-map longitude origin additionally differs by180 degrees from the static profile origin; centre and quarter-turn ray regressions retain this distinction. The earlier Rz180 and intermediate P hypotheses were rejected by the real room check and subsequent full consumer-chain reconstruction. Unknown profiles require an explicit unit mounting quaternion; hardware gyro_type alone is not a mounting matrix.

`POST /api/library/:id/insta360/gyro` uses source authorization and returns explicit sensor-axis/ready status. The inspector imports the embedded data with qualified automatic mounting or an explicit override, preserving existing edits on error, stale clip context or cancellation. Trimmed ordinary forward clips use source in/out times and receive clip-local orientations.

The renderer now supports locked direction, symmetric antipodal-safe Gaussian path smoothing and horizon following. Horizon assumes world-up camera-to-world orientations; raw integrations without established mounting are not automatically treated as world orientations.

## Camera following

The bounded contrast tracker adds motion prediction, immutable identity anchoring, small scale changes and global reacquisition after a gap up to2 seconds. Ambiguous lookalikes and unresolved loss reject the whole update. The browser preparation interval extends to60 seconds/300 sampled frames with explicit memory/computation bounds.

Saved SAM semantic regions can drive the spherical camera through Rust circular mask centroids. The native endpoint `/api/composition-projects/:id/spherical-mask-track` reuses the existing project-authorized FFV1 cache, verifies the saved region identity/hash, maps VFR source timestamps to mask ordinals and rechecks project revision/access before returning centres. Missing, ambiguous or lost semantic masks reject the update. Existing library sources retain the application's immutable-source assumption; the algorithm's actual mask bytes are verified.

Optional perspective auto-framing derives FOV from semantic mask extent, conservatively fits both source/canvas aspect ratios for any roll, retains the authored initial FOV and limits later zoom to30 degrees/second. It is opt-in; disabled requests keep pan/tilt-only behavior. Arbitrary canvas crops are outside this fit model.

Semantic following uses the application's existing SAM inference/propagation and does not introduce or qualify a new detector. It does not reconstruct DeepTrack's neural detector/reidentification. The new contrast reacquisition and SAM following are separately qualified capabilities.

## Additional runtime/static boundary

The bridge investigation attempted an owned LLDB startup under REA. macOS debugserver refused attach; there was no successful runtime bridge read and no security settings were changed. Evidence `ev_b908b7e15f24ecf0226f6e966b66ef3e446f14812c0c17295f1952e34b7a778b` records that failure. The bridge was subsequently reconstructed from source-bound static initializer/native quaternion code (`gyro-bridge/`), rather than treating the failed runtime probe as an observation of values. Further consumer tracing established that this ARV bridge affects an output hint rather than the map input; `gyro-frame-ray/` closes the actual quaternion-to-projected-ray path. Owned processes were cleaned up.

## Remaining parity boundary

Automatic calibration and IMU mounting are qualified for the inspected X6 v6/lens193 profile shape. Other profile versions/camera hardware, accessory/underwater conversion, factory IMU calibration matrices, full arbitrary-PTS/retime/rolling-shutter synchronization, yaw drift correction and optical-flow/parallax stitching remain open. The camera-follow additions do not reproduce DeepTrack's automatic neural reidentification or unrestricted automatic framing. No side-by-side Studio quality/performance benchmark establishes full FlowState/DeepTrack parity.

## Follow-up validation

- Camera profile:5 native tests,193/193 real calibration records, one real paired-track panorama; see camera profile logs.
- Final IMU/clock:15 tests passed in `gyro-longitude-mounting-qualified.log`, including actual X6, midpoint timing, gravity polarity and an independent longitude/basis regression. The previous30-test batch is superseded for the mounting hypothesis; its unchanged metadata, render and tracking checks remain separately scoped evidence.
- Extended semantic auto-framing:7 native/HTTP tests and16 frontend tests passed; see `tracking-framing-native.log` and `tracking-framing-ui-final.log`.
- Browser+real Rust API:2 tests passed in `browser-followup-qualified.log`: explicit/automatic gyro, camera profile+dual-track packing, native previews, undo/redo, video exports decoded for pixel checks. The automatic path also checks the signed rotation produced by a nonzero raw sensor-axis rate, so the intermediate wrong longitude basis fails this regression. These inputs are synthetic; they do not establish real-camera stabilization quality.
- Consolidated frontend:24 tests passed in `frontend-followup-qualified.log`. Final Svelte-check and focused ESLint are recorded separately.

The final corrected profile/IMU chain was also rendered on two real sources. A parking-garage scene has ceiling above, floor below and upright columns/person after correction. A room provides two independently measured window-frame edges (raw up-lean13.496 and14.306 degrees), agreeing with the corrected predicted14.55-degree direction. Derived personal images remain outside the repository. See final camera production logs for actual corrected outputs.

On the second room scene, gradient/Hough measurements of two window-frame edges change from approximately14 degrees to0.000/0.286 degrees; a more conservative outer-frame fit stays below1 degree. These are bounded image-orientation measurements on two local scenes, not an end-to-end FlowState quality benchmark. Final camera logs: `camera-flow-basis-final.log`, `camera-flow-basis-second-final.log`.
