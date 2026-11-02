# Image-derived dual-fisheye overlap correction

**2026-11-01 reference-mode audit:** settings selected in Studio UI did not establish effective export settings. Saved OWN timeline snapshots retain FlowState enabled, AI Stitching enabled and Optical Flow Stitching disabled. Later physical toggle/selected-timeline/exit-and-reopen checks still showed OFF in UI without changing the saved snapshot. Export mode remains unresolved; previous mode labels below describe UI observations only. Frame-zero fixed-rotation measurements remain renderer diagnostics, not a certified Optical Flow/Stab-OFF comparison. See `reference-settings-audit.json`.

The native `spherical_video` effect now has opt-in `stitching`:

```json
{"mode":"optical_flow","searchRadius":6,"strength":1}
```

`feather` is the backward-compatible default. Optical mode requires dual-fisheye input; search radius is1..12 analysis pixels and strength is0..1. A zero strength preserves the previous calibrated blend exactly. No camera-model defaults replace source calibration.

The renderer projects both lenses through the same calibrated/generic geometry and source-row sampling hook into a256x128 spherical analysis grid. It measures 2D displacement on a4-pixel grid using5x5 local normalized correlation, distinct-peak rejection and forward/backward consistency. Continuous inverse-compositional bilinear patch refinement bounds small-motion quantization. Independently measured matches are exposed to the visual drift estimator as normalized spherical correspondences, without changing source-frame coordinates.

In a supported overlap, the renderer applies symmetric half-displacements to the two lens directions before premultiplied-linear-color blending. Confidence fades missing neighbouring evidence back to ordinary calibration, and conflicting neighbouring vectors are not averaged across a depth discontinuity. Both analysis and final sampling use `lens_sample_at`, so rolling-shutter source-row correction is not bypassed. Longitude wraps; poles, missing coverage, ambiguous/flat patches and inconsistent matches retain geometric blending instead of fabricating image motion.

This is bounded patch optical flow with an interpolated field, not a copy or quality qualification of Studio's optical-flow engine. It has no learned dense depth, multi-frame temporal regularizer, large-displacement pyramid or global seam optimizer. The analysis resolution/range limits apply even to a high-resolution source. Occlusion rejection is photometric/consistency based, not semantic. These boundaries remain explicit when comparing to Studio.

## Qualification

The standalone temporary harness imports the actual production module by absolute path and reuses the public API tests from `backend/tests/spherical_optical_flow.rs`. It avoids the monolithic library while unrelated modules are being edited; it is algorithm evidence, not a substitute for native renderer integration. Three tests pass in `optical-flow-module-qualified.log`, including wrapped longitude, flat/occluded/cancelled input and fractional translation. On band-limited random texture, true shifts0.2/0.4/0.7 pixels produce mean measurements0.1989177/0.4001003/0.6997070 from855/822/850 independently admitted nodes. This bounds bias on that fixture rather than on arbitrary camera footage.

An initial native render-stack test (before final subpixel refinement) reduced midpoint overlap MSE from1854.9 to969.3 on a source-derived textured dual-fisheye parallax fixture (ratio0.523), retaining opaque alpha. Final native tests pass6/6 in `optical-flow-native-final.log`, including zero-strength identity and the real source. The final textured fixture MSE is1854.9045→404.8337 (ratio0.21825, a78.2percent reduction on this known synthetic midpoint target).

Two real X6 sources pass. The garage provides415 overlap candidates/55 admitted matches, mean accepted-patch NCC0.219582→0.880784,22264 changed output pixels and524288/524288 opaque pixels. The room provides283/58 matches, NCC0.234205→0.890985,26005 changed pixels and the same full opaque coverage; see `optical-flow-native-second.log`. These are selected consistent-patch registration metrics, not full-image ground truth. Both output pairs were visually inspected: no holes or large global warp; close-hand/low-texture/large-disparity ghosting remains where correspondence is unsupported.

Observed debug render times were1.569/2.376seconds (feather/flow) on the first source and3.422/3.504seconds on the second. Concurrent builds affected wall time; this is not a throughput or peer-performance benchmark.

Supplemental vendor artifact investigation is recorded in `optical-flow-rea/README.md`; it is partial. Two actual Studio video references were later exported after its existing updater disappeared: AI Stitching with FlowState on, and explicitly selected Optical Flow Stitching with FlowState off. Both are 7680x3840, 50 fps, 38 frames, 0.76 seconds. Their valid decoded frames were visually inspected; an earlier black snapshot was rejected. The vendor and native fixture have different static spherical orientations, including with the vendor FlowState toggle off, so unaligned pixel errors cannot qualify vendor stitching parity. See the REA README for recorded settings and provenance. The local real test requires an explicitly supplied read-only media inventory and writes personal derived PNGs outside the repository.

## Single-frame Studio comparison

`optical-flow-studio-reference.json` records the two real export hashes, ffprobe results, UI settings, input-image hashes and measured output. `scripts/compare-insta360-panorama-reference.py` accepts explicit reference/feather/flow paths and a fresh private output directory; it requires NumPy, SciPy and Pillow. Private footage and derived pictures are not checked into this repository.

For frame 0 of the second (Optical Flow Stitching, FlowState off) export, a single fixed SO3 was fitted to the feather image's luma and reused unchanged for both native variants. Sphere-area-weighted RGB PSNR was 30.7457 dB for feather and 30.7891 dB for native flow. In the 9,727 pixels changed by native flow (defined by a >1/255 RGB change, then rotating the mask), PSNR was 30.5736 versus 32.4839 dB. This mask reports the local effect of our change; it is not a complete vendor seam mask. The small whole-image improvement and remaining close-hand ghosting do not establish full parity. Differences in calibration, chromatic calibration, resampling and frame selection can also contribute. Both the reference and aligned native image were visually inspected.
