# Bounded multi-view SAM identity and exposure qualification

The identity guard now uses linearly binned RGB chromaticity rather than raw intensity histograms. The previous implementation refused the same foreground at half exposure even with identical learned features; `illumination-before.log` retains that failing reproduction. Dark-to-unobservable pixels, white balance changes and clothing changes are not guaranteed by an exposure-invariant colour guard.

Schema15 adds at most12 source/hash-bound saved-mask exemplar candidates and reports all exemplar hashes and admitted frame indices. Schema14 requests and responses remain supported unchanged. The legacy capability scalar stays14; the supported-schema list advertises14/15 and a separate latest-schema field advertises15, preserving old capability clients. An identity bank contains the original seed plus at most3 distinct admitted views. The original descriptor is retained. Global proposals and candidate similarity compare against the frozen bank, preserving the global competing-instance margin and colour/area guards.

A saved mask hash verifies bytes, not object identity. Enrollment therefore requires a consecutive sampled index, saved-mask overlap with the previous plane >=0.25, a unique global neural result classified `tracked`, and result/saved-mask IoU>=0.5. Admission stops at the first gap, ambiguity or disagreement. Recovered targets never update the bank. Handler source ACL, original/source/mask/model/runtime hashes, private staging, cancellation, dual-lens projection and zero unknown planes remain enforced. Hash-valid stale masks are an explicit HTTP negative case.

## Actual learned inference

Pinned local SlimSAM encoder/decoder and ONNX Runtime are used; no vendor tracking models are copied.

- Legacy real selected-face longitude recovery remains IoU0.97472286.
- A real video first/last pair separated by0.7s has actual hand/body motion; target mask agreement is IoU0.962215 at original exposure,0.9690993 at0.55 exposure,0.9560155 at1.35 exposure. End-frame reference masks come from explicitly supplied SAM prompts, not independent hand-annotated ground truth. Exposure variants are controlled transformations of a real end frame, not naturally changing lighting footage.
- A duplicated private photographic target is `ambiguous`; an image without the target is `occluded`, with no admitted mask in either case.
- Actual neural synthetic tests retain green appearance-distractor refusal, identical-target ambiguity, blank occlusion and seam recovery. Five pure identity tests cover the bounded view bank, immutable seed, illumination guard, distractors, malformed input and cancellation.

The second actual source is17 calibrated body-panorama frames across4s. It has camera roll and moderate subject motion, not a strong front/profile/back pose sequence. It exposes an ambiguous saved shirt/arm hypothesis at sample3 after confident samples1/2; enrollment must freeze rather than treat propagation as identity truth. The bank admits2 distinct views from samples1/2, freezes before ambiguous sample3 and reacquires sample16 after the gap with prompted-mask IoU0.9476791. This exercises actual multi-view inference and fail-closed admission, but does not demonstrate superiority over a single seed on strong pose changes. The initial test incorrectly required admission of sample3; `real-propagated-mask-before.log` retains that failure. The revised criterion checks the required refusal and keeps the later recovery threshold unchanged.

## Scope boundary

This remains source-owned SAM appearance matching. A dedicated instance-ReID model, strong front-to-back pose changes, wardrobe changes, naturally varying illumination, broad camera footage and comparative Studio DeepTrack accuracy are unqualified. Passing these bounded regressions does not establish full DeepTrack parity. Personal media, masks and source paths remain outside this repository.

The final local qualification executable SHA256 is `06ef27b3dcdbecf2a8eaaf087abe12b9309ff38131f8e9125a98d46d4d96215b`. Provisioning retained both previous versions under content-addressed backups and did not modify global SAM configuration.

Actual HTTP qualification passes3/3 (100.10s): prepared equirectangular, raw dual-stream calibrated panorama with gyro, and a hash-valid stale saved exemplar over a blank source frame. The stale exemplar is not enrolled; three gap planes remain zero and the real target is later reacquired. A separate schema15 native executable run confirms the same states, empty admissions and final output SHA256.
