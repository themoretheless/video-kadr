# Seam and identity continuation

These changes fix two bounded failure mechanisms. They do **not** close extreme near-object occlusion or strong-pose identity recovery. Earlier measurements remain in [the stage report](README.md).

## Stitching: measured geometry versus confidence

Rendering previously multiplied the measured correspondence by its match confidence. Thus a correct but imperfectly scored match stopped short of its measured midpoint and retained residual double edges. Source-A correspondence confidence is still useful evidence for analysis, but it is not a physical displacement scale.

The field now carries spatial support separately from matching confidence. Fully covered, mutually consistent measured triangles reach their measured midpoint; missing spatial support still fades the warp. Conflicting depths remain rejected. RGB rendering, projected source masks and experimental ownership preparation use the same geometry. The existing `displacement` analysis contract and source-A gyro correspondences retain their confidence values.

Three new mathematical regressions cover full support at imperfect confidence, missing support and competing depths. Together with the existing exhaustive cut-energy check, the exact module passes4tests. The current main-checkout optical integration suite passes14/14, including the original X6 fixture. The known-midpoint image MSE improves from365.22465 to71.071875, versus feather1854.90451. This is a synthetic geometric control, not a perceptual vendor-parity score.

The real garage comparison reuses the historical reference and its exact prior SO3, with rectangles fixed before the candidate:

| Region | Previous flow dB | Corrected flow dB |
|---|---:|---:|
| Whole sphere | 30.8068 | 30.8255 |
| Near forearm | 27.4851 | 27.4855 |
| Background | 35.2202 | 35.3040 |

**The extremely close forearm still visibly ghosts.** Its score is effectively unchanged; this is not evidence of fixing that occlusion. The historical reference's export settings remain unresolved. It is suitable for comparing two candidates against the same image, not for assigning a vendor-mode parity claim.

Controlled FlowState-off / OpticalFlow-on frame0 controls are mixed: office28.92428→28.98051dB; X4 outdoors25.45753→25.40059dB. They share their original feather-derived rotations and unchanged decoded/feather frames. No per-camera switch or reference-dependent fitting was introduced. These metrics combine correspondence, calibration, resampling, color and vendor-rendering differences; broad visual improvement is not established.

An isolated512×256 analysis-grid experiment was also rejected: it does not improve the forearm (27.4851→27.4842dB). Production analysis remains256×128, feather remains the default mode, and experimental graph ownership remains disabled by default. No unqualified high-resolution or ownership candidate was copied into production.

## ReID: visible continuity at multiple scales

When fine RGB texture is insufficient, visible continuity now considers2×/4× downsampled evidence. This preserves NCC>=0.8, uniqueness ratio<=0.8, reverse-cycle error<=3original pixels, coherent-motion residual<=6original pixels and the original bounded search window. Error budgets account separately for horizontal/vertical scale, including non-divisible dimensions. A replacement cannot reduce the number of measured matches or either source-coordinate witness span. Evidence from separate scales is not pooled to inflate its support count.

The native sensor-noise regression initially exposed a new ambiguity: identical subjects with independent noise could produce two valid displacement clusters, and the previous median retained only the majority. Matching now refuses two separately cycle-checked coherent clusters when their transported object supports are distinct (IoU<0.25), instead of choosing their majority. Different limbs moving within the same overlapping support are not treated as different owners. A detected owner ambiguity cannot be silently overturned by a coarser pyramid level. The noisy-twin refusal, unique noisy-target tracking, flat-frame refusal, exact-twin refusal, exposure and cancellation tests pass.

On the calibrated X6 shirt sequence, three consecutive transitions have prompted-reference IoUs0.962421/0.9648962/0.9827846 instead of losing the target at the second transition. The standalone aligned image/mask continuity check also passes. The actual4second gap still ends `Occluded`, with no admitted extra view, so the existing positive recovery regression remains **failed**.

The standalone fixture uses calibrated images and masks in the same half-longitude-rolled coordinate frame. Pairing its masks with unrolled or raw dual-fisheye images is invalid evidence.

The actual Luna front→back→close-up safety case still refuses the different helmeted person and the incorrect close-up extent. Positive back/close recovery remains **unqualified** and its separate coverage gate remains failed. No identity threshold was lowered; immutable source identity and frozen-gallery safeguards remain enforced. Prompted reference masks are not independent hand-annotated ground truth.

The QA runtime alias and global model configuration were not republished. These are source/runtime-library changes, not a claim of installed-release acceptance.

## Evidence

See [machine-readable results](seam-reid-results.json), [source snapshot](seam-reid-source-snapshot.json), [main optical integration run](support-optical-main.log), [geometry module tests](render-support-main-tests.log), and the continuation logs alongside this report. Source images, camera originals and masks remain private.

Final vision-library checks pass33tests (14opt-in cases ignored). The actual continuity check passes separately. The real hard-case safety test passes, while strong-pose positive recovery and the office4second gap remain failed. No quality gate is reported green from a refusal alone.
