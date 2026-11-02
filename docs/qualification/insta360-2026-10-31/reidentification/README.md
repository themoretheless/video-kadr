# Source-owned SAM neural identity recovery

This implementation uses actual pinned SlimSAM encoder embeddings and decoder masks through the existing Rust/ONNX Runtime host. It is not a port of Studio's dedicated DeepTrack model or proof of DeepTrack quality parity.

## Native behavior

A source-owned seed mask initializes an immutable 256-channel foreground descriptor, centred against the image's global neural features. Source RGB appearance and an immutable seed-area prior guard instance identity and preserve the selected semantic part (for example a face rather than silently expanding to the whole person). Global encoder-feature proposals are decoded through SAM, admitted by identity similarity, deduplicated by connected-object containment and separated by an ambiguity margin. Disjoint strong instances and lookalikes yield `ambiguous`, not a silently selected target. Missing targets yield `occluded`. Identity is never adapted onto a new object.

Two longitude cuts are evaluated jointly. Alternate-cut masks are mapped back into source coordinates before instance grouping, allowing a target to cross the panorama seam without being mistaken for two independent identities. Neither method uses the prior two-second texture-tracker timeout. The native stage evaluates every sampled frame after loss and may reacquire much later in the requested interval.

`--sam-reidentify` accepts strict schema14 with the existing absolute host paths, rawRGBA stream/source SHA256,256x256 seed mask/SHA256, width/height/frame count/seed ordinal and a new output path. The result contains a256² mask plane for every input sample and one state per sample: `seed`, `tracked`, `reacquired`, `occluded` or `ambiguous`. Unmeasured planes are exactly zero. Source, seed, models and runtime identities are checked before publication. The old schema9 `--sam-track` behavior is preserved.

`--capabilities` advertises schema14 without loading models. The backend checks this protocol before staging media. The build/provision script builds the native runtime and atomically installs it to an explicitly chosen absolute host path, retaining an existing executable under a content-addressed backup. It does not change models or server configuration. SAM executable/model/ORT paths remain host-controlled, as in the existing application.

## API and UI

The source/project-authorized spherical-mask-track endpoint adds opt-in `reidentify:true`. It prepares bounded sampled originalRGBA, takes a verified saved SAM seed, fingerprints original source content before and after inference, validates the child output hashes and per-frame visibility, and rechecks project access/revision. Responses add `sampleIndices` and `states`; centres and optional FOV exist only for actual visible measurements. There are no invented masks inside occlusion intervals.

For raw dual-fisheye, the native worker decodes both source tracks (or a packed raster), projects the saved source mask through the same calibrated optical-flow/rolling-shutter sampler as RGB, and runs inference on a body-space equirectangular panorama. Global gyro is disabled only during inference preparation; measured rays then pass through the current proposed spherical orientation/stabilization/source-frame clock. Current unsaved trim origins are explicit and validated. The raw path requires neural inference and cannot interpret a first-lens mask directly as panorama coordinates.

The inspector's recovery checkbox is mandatory for raw dual-fisheye and opt-in for prepared equirectangular input. It requires an available local SAM runtime. It samples at most60 frames across the first60 seconds (about1Hz on long clips) instead of the default300-frame saved-mask-only path. Sparse camera keys retain their exact source timestamps and leave gaps to the existing interpolation. Cancellation, stale contexts, unavailable runtimes and invalid state/index correspondence preserve the existing authored keys. This offline neural stage can take several minutes and has a600-second neural-child deadline; existing verified-mask restore and bounded per-frame decoding are separate preparation stages.

## Verification and scope

- Actual encoder/decoder/ORT execution passed: blank occlusion followed by spatially moved target; rejection of a differently coloured distractor; refusal to choose between identical instances; seam-crossing target recovery.
- On explicitly selected private photographic material, recovery after a one-third-image longitude shift matches the shifted original selected-face mask with IoU0.97472286 (4396 intersecting pixels,4423 predicted,4483 expected). The initial implementation expanded a face to a whole-person mask and failed the0.5 criterion; the immutable area prior/identity representative fix passes the unchanged criterion. Personal derived mask images remain outside the repository.
- Three pure native identity tests cover moved identities, ambiguity, colour distractors, malformed inputs and cancellation. The actual neural suite totals5 passing tests.
- Both actual neural HTTP tests pass (2/2, 67.64s): prepared equirectangular input and raw dual streams. The raw-dual-stream test passes source ownership, original hashes, source-mask projection, panorama preparation, blank occlusion, colour distractor rejection, reacquisition and a 90-degree gyro camera transform. The saved-mask HTTP regression passes current proposed orientation and unsaved trim-origin mapping, stale origin rejection and raw-without-neural rejection.
- Fourteen frontend tests cover opt-in sparse measurements, gap validation and unavailable-model refusal, plus existing framing/undo-safe behavior.

A full comparative benchmark with Studio, dedicated learned instance-ReID training, broad pose/lighting/appearance changes, long-video real-time throughput and all-camera qualification remain open. These boundaries must not be replaced by a full-parity claim based on this passing test slice.

The retained Studio `CreateDeepTrackerOptions` REA observation in `focused-rea/run-vvewc9c9/procedure-3.pseudo.c` references separate DeepTrack model/configuration getters and bigboom model files. REA Evidence `ev_20a896c5bd191dd83d80e8c7b1a73578159535d48dafc614f7ae0cd2302d5029`. That static observation identifies distinct assets, not their runtime quality or equivalence to SAM. No vendor weights/code are copied into the product.
