# Camera-follow qualification follow-up

This change adds owned camera-follow behavior; it does not establish Insta360 DeepTrack equivalence.

## Selected-texture path

`frontend/src/lib/motion/sphericalTracking.ts` uses an immutable identity template plus a conservatively refreshed appearance template, normalized correlation, bounded motion prediction and small scale changes. Longitude is sampled with wrap addressing without allocating three copies of each frame. Local search failure triggers a bounded full-panorama search; independent competing matches are rejected. A target can be reacquired after a gap of up to two seconds. No measurements or predicted keys are created inside the gap: camera animation interpolates the measured endpoints. A persistent or trailing loss fails atomically. Pixel memory is limited to 16 MiB and template comparisons to 80 million.

The UI samples up to the first 60 seconds, 300 frames at 5 fps, with a 120-second browser decode deadline and explicit cancellation. This is a selected texture tracker, without semantic detection, learned re-identification or arbitrary deformation handling.

## Semantic camera-follow path

The spherical inspector can use the existing saved SAM region inventory on a video cutout. A new authenticated POST `/api/composition-projects/:id/spherical-mask-track` accepts an exact saved project revision, clip, retained region and up to 300 increasing source presentation instants spanning at most 60 seconds. The handler verifies project editor access, current clip original/copy binding, exact attachment membership, source interval and target loss boundaries. It restores the existing lossless FFV1 SAM movie through `concat_video_region_cache::load`, which validates project reservations, file/mask hashes and original VFR presentation clock. Each selected plane is reverified before measurement. Source files and project documents are unchanged. Original-source identity follows the existing immutable-library-source contract; this request rehashes the algorithm input (the retained mask movie and planes), rather than rereading the full original camera video.

The Rust domain computes circular longitude mass centroids and vertical mask centroids, rejecting empty and longitude-ambiguous regions. The browser receives centre measurements and creates ordinary continuous pan/tilt keys. Requests are cancelled on unmount and stale clip/region/project context cannot apply late results. This connects existing SAM semantic propagation to the spherical camera; it does not add neural inference or re-identification to the SAM tracker.

## Verification

- `tracking-ui-followup-final.log`: 12 focused frontend tests passed, including seam continuity, short occlusion reacquisition, duplicate ambiguity, permanent loss, 300-frame interval, affine illumination changes, VFR sample ordinals, response validation, native request and unmount cancellation.
- `tracking-native-centres-direct.log`: 3 native tests passed, including actual FFV1 encode/decode, VFR source-to-plane selection, verified region reads, circular seam centroid and mutation rejection. Procedural semantic masks; no SAM inference.
- `gyro-native-combined.log`: final isolated-target qualification passed all 4 tracking native tests (3 domain/FFV1 tests plus 1 authenticated HTTP integration test). `tracking-native-followup.log` retains the earlier fixture source-index-order failure; `tracking-http-followup-2.log` retains the cancelled redundant shared-target wait.

The HTTP integration test constructs a saved valid composition and a real procedural FFV1 mask movie, checks owner-centre output, anonymous/outsider denial, stale revision, forged attachment, invalid source times, file mutation rejection, stage cleanup and unchanged document/revision.


## Optional semantic auto-framing

The semantic control now offers an opt-in perspective auto-zoom. Disabled requests omit `framing`; disabled responses omit `fov`, and the UI preserves existing FOV tracks. Enabled requests carry `{aspectRatio,targetFill,initialFov}`. The native estimator projects every retained mask pixel corner into the same spherical camera tangent plane and fits a circumscribed circle, using the larger of canvas and working-source aspect. The fit remains conservative for any camera roll and avoids seam-coordinate bounding boxes. `targetFill` is a desired upper bound, not exact object occupancy. The estimator rejects targets extending behind the perspective camera or beyond its 179-degree FOV limit.

The generated first FOV key exactly preserves the authored clip-start FOV. Subsequent keys ease toward the size-dependent estimate with a maximum zoom speed of 30 degrees/second. During this transition the target fill can exceed its desired bound; the smoothing avoids an instantaneous first-frame zoom. The UI checks this initial-value invariant, rejects incomplete FOV results, and offers auto-framing only for perspective output. Custom canvas transforms/crops remain authored independently of this projection-space fit.

Final follow-up evidence:

- `tracking-framing-native.log`: 7 native tests passed (6 domain/actual FFV1 tests and 1 real authenticated HTTP integration test). Includes mask-size/FOV changes, seam equivalence, near-pole stability, portrait aspect, elongated target containment at roll 0/45/90/137 degrees, tall targets, first-FOV preservation and bounded zoom, omitted-default/opt-in HTTP response contracts.
- `tracking-framing-ui-final.log`: 16 frontend tests passed. Includes complete optional FOV channel validation, non-default authored initial FOV preservation, rejection of an unexpected first zoom and ignoring unsolicited zoom when disabled.
- `tracking-framing-typecheck-final.log`: final Svelte/TypeScript qualification result.
