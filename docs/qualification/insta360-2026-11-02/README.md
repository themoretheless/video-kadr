# Insta360 qualification follow-up, 2026-11-02

The [seam/ReID continuation](seam-reid-followup.md) records the later geometry/support correction and multi-scale continuity changes. Earlier tables and logs below retain their original source-stage attribution.

Full DeepTrack/FlowState parity remains unqualified. Passing a refusal test is not evidence of successful recovery. Original media, decoded frames and masks remain outside the repository.

## Implemented changes

- Neural sampling now retains intermediate poses at 5 Hz for clips up to 12 seconds, within the existing 60-sample limit; longer clips spread those samples over the requested interval. Very short clips retain at least three requested instants before source-ordinal deduplication. This does not itself establish correct tracking or enrollment of every pose.
- Instance matching and gallery admission now honor cancellation even on their low-texture and insufficient-feature fast paths.
- A separate opt-in `actual_front_back_close_person_identity_coverage` test requires actual recovery of the two held-out poses. The original safety test can pass a refusal; the coverage gate cannot. Neither its IoU threshold nor its neural identity threshold was lowered.
- The private matrix harness accepts an explicit experimental ownership strength and records it in its ledger. Production defaults remain unchanged.

## Actual neural results

The Luna front/back/close-up safety run refuses the different helmeted person and does not return the previously incorrect close-up mask. It also refuses both positive held-out targets. The safety test passes; the separate recovery coverage test **fails**, at both 4 and 10 seconds. Strong-pose recovery remains open. See [safety](reid-hard.log) and [strict coverage failure](reid-hard-coverage.log).

Pinned SlimSAM synthetic return, twin ambiguity, appearance distractor, occlusion and longitude-seam controls pass in one actual encoder/decoder test. This is a bounded synthetic control, not photographic strong-pose qualification. See [learned control](reid-synthetic.log).

An initial reconstructed office fixture was mistakenly extracted directly from a packed dual-fisheye intermediate. Its failed mask-agreement run is invalid for the test's calibrated-panorama contract and is retained only as a diagnostic. It must not be presented as a ReID regression or a successful qualification. The replacement is rendered through the qualified camera profile at source 0/0.25/0.5/0.75/4 seconds. On that calibrated fixture, frame1 is `Tracked` with prompted-reference IoU0.962421; frame2 is `Occluded` with no accepted mask and the existing positive regression **fails**. Thus the candidate also has an ordinary-scene coverage gap. See [calibrated result](reid-office-calibrated.log). The remaining later frames are not claimed as qualified.

The existing QA runtime alias has not been republished from these results. No installed-release qualification is claimed.

### Rejected dedicated recognition-model experiment

The author's [OSNet-AIN model](https://github.com/KaiyangZhou/deep-person-reid), using [author-published weights](https://huggingface.co/kaiyangzhou/osnet), was evaluated separately on explicitly inspected person crops. Source commit `f8cd150fdf77e8d9e1ed143b7f308c2c609ded50`; model revision `a5c5cc037c24235cda3b21085b93ad77c9616224`; weights SHA256 `8a07e8da38946f7cee37f4561617bf8b6d2fe8f3a4027852893ea092e46d919f`. Only tensor weights were loaded (`weights_only=True`). No vendor model was copied and no global application configuration was changed.

Cosine similarity to the front-view seed is0.4555 for the same person's back view,0.4974 for their close-up,0.5712 for the nearer different person and0.4906 for the farther different person. Therefore no single similarity threshold can admit both positives while rejecting the nearer distractor in this diagnostic. The model is **not integrated** into recovery. These manually supplied crops bypass detection/mask proposal and do not constitute general model evaluation. See [diagnostic matrix](osnet-crop-diagnostic.json).

## Fresh native renders and Studio comparison

[The matrix](camera-matrix.json) covers three original sources: X6 garage, X6 office and X4 outdoors. Including X4 Locked/Horizon variants, it renders 14 held frames in both feather and optical-flow modes: 28 images, each with 524288/524288 opaque pixels. Calibration and metadata success are distinct from visual parity. The gyro and spherical-video source hashes match those of the retained safety qualification helper; the [snapshot](source-snapshot.json) records current source hashes.

Controlled Studio X4 references have FlowState on, Direction Lock off, AI/Fusion on and classic OpticalFlow off. Capture times 0/1/3 seconds correspond to export times 0/4/12 seconds. Each mode fits one initial feather-derived SO3, shared by both render variants and reused without refitting at later frames. That free initial alignment does **not** validate automatic absolute leveling under a dynamic start.

| Native mode | Capture seconds | Feather dB | Flow dB |
|---|---:|---:|---:|
| Horizon | 0 | 25.579 | 25.524 |
| Horizon | 1 | 16.843 | 16.835 |
| Horizon | 3 | 18.857 | 18.847 |
| Locked | 0 | 25.582 | 25.529 |
| Locked | 1 | 23.534 | 23.441 |
| Locked | 3 | 22.110 | 22.079 |

See [hashed measurements and frozen background controls](controlled-x4-comparisons.json). These pixel metrics combine trajectory, stitching, calibration and resampling differences. They establish neither perceptual equivalence nor best-vendor stitching quality. Horizon motion still differs materially from the reference. Short pre-roll gravity remains explicitly estimated.

Against byte-identical Studio reference images, the older Locked trajectory scored17.9937dB at1second and15.2492dB at3seconds; the safety-corrected renders score23.4410/22.0792dB. Each trajectory uses its own single initial alignment because initial gravity changed; neither later frame is refitted. This supports a relative-trajectory improvement, not precise absolute gravity. See [before/after receipts](locked-before-after.json).

## Rejected seam experiment

A private native candidate applied the graph's ownership consistently even at locations with measured flow, instead of switching ownership off there. It was built and rendered on the three-source matrix, with experimental ownership strength1. In the historical garage reference's fixed SO3 and predeclared rectangles, the near-object PSNR regressed from27.4851dB (default flow) to26.6827dB; background regressed35.2202→34.5316dB. Visual inspection still shows a wrong-view forearm splice. The historical export's stabilization/stitching settings remain unresolved; this is a same-reference candidate rejection, not a controlled vendor-mode claim. See [hashed comparison](rejected-continuous-ownership.json).

The candidate domain-source change was reverted exactly after its executable/source were isolated privately. Default stitching source is unchanged. Neither a partial fix nor a quality improvement is claimed for this experiment.

## Checks

- Native vision library:32passed,13opt-in tests ignored, [log](vision-unit-final.log).
- Sampling and clock mapping:5passed, [log](semantic-sampling.log); scoped ESLint passes; full Svelte check reports0errors/0warnings, [log](typecheck.log).
- The matrix example checks successfully against the restored production domain source, [log](matrix-check.log). Existing unrelated compiler warnings remain.
- Actual learned synthetic control:1passed; hard photographic safety:1passed; strict photographic positive coverage:1failed; calibrated ordinary-scene positive regression:1failed. These results are deliberately not collapsed into an all-green aggregate.
- The new coverage test and diagnostic mask output affect opt-in qualification; no runtime alias or installed release was published.

## Open gates

- Extreme near-object stitching: whole-object seam consistency and occlusion handling remain unqualified; the experimental ownership mode remains disabled by default.
- Positive strong-pose recovery and calibrated ordinary-scene regressions must pass before runtime publication.
- More original camera models, naturally changing appearance and difficult scenes need controlled reference qualification. X4/X6 renders and Luna refusal checks are not an all-camera certificate.
