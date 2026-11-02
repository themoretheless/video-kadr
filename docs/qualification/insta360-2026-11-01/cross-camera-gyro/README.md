# X4 legacy gyro and retained trim time maps

Target: Studio6.0.2 executable SHA256 `6a100040c7e39d202b64e2fc0470e2f52600a39c4cde4e3fb1e92cea5ed76423`, unchanged from the retained REA investigation. Original function capsules and source-bound switches are reused; no cold whole-app import or target execution was performed.

## Camera basis

Actual public X4 Hawaii original:214039326bytes, SHA256 `f3274d265e03f4f23ff5990df06b758a75129f19fe43b949d96edd9f8eb50009`, model X4, firmware v0.3.11_build1. Its ExtraMetadata hardware gyro_type is1. Calibration flags1095 (`0x447`) encode legacy lens71/version4; that version4 is NOT the hardware gyro_type.

Retained REA GetLensConfig dispatcher Evidence `ev_b2c2abf7994c3e84322fd73824f974d9328eb6ea162686452b67fd34be770935` omits its external switch tables. The separate original-byte supplement resolves them: lens71 selects branch0x1018b431c; version4 selects0x1018b4424, which assigns native GYROTYPE145 (`0x91`) and stores it at the returned configuration's offset0x10. The v6 lens193/version4 path returns the same145. `source-bound-switch.json`, `legacy-version4.json` and `lens-config-branches.llvm.asm` bind the exact table bytes and targets to the original executable. This is a supplemental byte observation, not a newly generated REA record.

ResolveGyroTypeFromOffset Evidence `ev_85c2f192d00aa9f1baf0d0765bb824aeb9315d70dd8dd40ce0ca556a3edb1161` remaps only native enums17/18/32/36 for the raw branch, leaving145 unchanged. Both supported dual-lens non-UAV models then use the same AlignAxes145, first-lens pose convention, native common reference, captured longitude origin and quaternion-to-ray consumer. Reused Evidence references are enumerated in `reused-evidence.json`. The legacy polynomial/crop changes source projection, not this shared gyro basis. The current Rust legacy renderer uses the same native pose and editor reference convention; it is not assigned a different model-name default.

Therefore the strictly qualified X4 raw/profile combination uses the same **final** mapping as qualified X6: gyro `[-z,x,-y]`, specific force `[z,-x,y]`; proper quaternion `[.5,-.5,-.5,.5]` plus embedded-only acceleration negation. The production source marker is `embedded_legacy_145`. GO Ultra and other mono cameras are not admitted by this dual-lens gate, even when a calibration version or hardware word happens to match.

## The public file is a retained original-time trim

The file contains762 decoded frames per lens at100fps (7.62sec), but47296 gyro records /4707 exposure events covering approximately47sec. The seemingly opaque block128 is decisive: it maps decoded video0..7610ms to original21000..28610ms in762 dense pairs. Sampling IMU directly from metadata.first_frame_timestamp without this map uses the wrong21-second interval and can produce a convincing but incorrect orientation.

Fresh source-bound REA function capsules establish the native contract:

- ExtraInfoAsset::CopyTimeMapRawData explicitly reads kind0x80. Evidence `ev_eb7590a4b2b8f5cf9692cfed7d1d936fa1dd352fbc0576b5370e87452ed56b57`.
- FixedTimeMapRawData::ParseHeader copies24 header bytes; verifies rangeBytes=rangeCount×32 and itemBytes=itemCount×16; reads ranges at24 and pairs at24+rangeBytes. Evidence `ev_113e300eb59b863733f90a6d2b379e789defa89773c1f50bc1e287c81c7d8225`.
- MapToOriginSrcTimeMs binary-searches the first pair column and linearly interpolates the second. Evidence `ev_ef3eaab206b6fe52fb057b15a9f57b95fff0922a8df3f932dfb31dcd62f3bd5e`.

These derived images omit outside memory/calls and use generic AArch64 ABI; full target execution/SDK quality is not established. The native format is broader than the implemented subset.

`insta360_time_map` admits only one complete range, dense uniform CFR pairs, unit slope/constant nonnegative frame-aligned translation, matching actual decoded frame count, consistent range endpoints and bounded sizes. Its opaque header word is never dereferenced. Other maps, malformed payloads, unqualified video clocks or missing exposure support fail BEFORE approximate fallback or manual mounting can return an orientation. They are not silently treated as identity timing.

For an admitted map, only the original exposure-index origin shifts. `sourceFrameClock` stays on the physical decoded file's100fps clock and authored sourceStartSeconds; `captureClock` retains actual original sensor capture times, exposure midpoint and jitter. The response declares `exposure_midpoint_trim_map` and `originalTimeMapOffsetSeconds:21`. Guard-band/endpoint extension remains explicitly labelled, preserving the existing clip-origin and row-readout contracts.

## Actual source check and limits

The actual Rust source-module harness selects decoded frame225 (2.25sec, original23.25sec) and its next half-second, then renders its raw and corrected views through the current Rust source-owned legacy profile rotation/projection. A viewed contact sheet shows the raw down/up directions becoming upright people and shoreline. `x4-frame225-summary.json` retains structural/numerical results and the external image artifact path. Actual clip0 capture is near original23.247756761sec after shutter/jitter, while `sourceFrameClock.sourceStartSeconds` remains2.25. The calibrated force rotates to world+Y to numerical precision.

Decoded frame0 has approximately1.24g, outside the gravity gate, so its first accelerometer sample alone cannot initialize absolute horizon. The final trim-only observed pre-roll path described below resolves this source using real retained history; disabling it explicitly reports unavailable gravity rather than claiming absolute level. Frame225 has approximately1.017g and continues to use the ordinary initial-sample path. Factory sensor compensation and proprietary stabilization quality remain separate obligations. The QA render uses actual source profile geometry with nearest sampling, not production blended preview/export or comparison to a Studio output.

`source-module-tests-qualified.log`:21 tests passed, exit0:19 ordinary tests and two explicitly authorized real X6/X4 tests. These compile the actual Rust metadata/profile/gyro/timing/drift/clock modules directly with a layout-equivalent Orientation value type, avoiding the busy whole-backend Cargo lock. They include the21-second original trim, padded start/end phase, preserving decoded sourceFrameClock, unknown/malformed/unsupported-map refusal even with manual mounting, profile-specific X4 auto gating and unchanged X6 behavior. Whole-backend tests/build integration are recorded by the parent final batch; this focused source-module run is not reported as that batch.


## Bounded observed pre-roll and explicit gravity readiness

The optional `gravityPrerollSeconds` request defaults to1 and must be finite0..1. It is consulted ONLY when a qualified retained trim map exists, accelerometer correction is enabled, and the first processing sample is outside the gravity band. Untrimmed sources and acceleration-disabled requests keep their existing path. At most one second /4096 history samples are searched. A seed requires10 or more actual samples spanning50–75ms, mean force magnitude0.9..1.1g, vector scatter≤0.03g, and observed angular speed≤0.35rad/s throughout the window. This is a conservative low-motion observation, not an inferred stationary-bias calibration.

The selected mean starts fusion at the observed window midpoint. Actual gyro history is integrated forward; no synthetic exposure or pose history is invented. A world-Y phase change at the authored clip origin removes the pre-roll heading gauge while retaining estimated gravity. The existing precise-track/NLERP compression budget still applies to transported clip-zero orientation. Captured exposure knots, decoded sourceFrameClock, motion-padding offset and authored output timestamps are unchanged.

`gravityAlignment` reports `status`, nullable `warning`, `requestedForceNormG` (initial processing sample including any existing observation guard), `observedPrerollSeconds`, and `initializationSamples`. Status is one of `aligned_initial_sample`, `aligned_observed_preroll`, `unavailable_dynamic_start`, `disabled`, or `sensor_axes_required`. A dynamic start without an admissible observation carries an explicit warning: gyro-relative integration is available, absolute gravity alignment is not established.

For the real X4 first frame,52 observed samples provide a seed0.731290sec before the requested capture. The first force is1.2396g, so it is not naively treated as gravity. The resulting first quaternion is approximately[.663860,.747647,0,-.017720], replacing the previous unaligned identity. A viewed actual-frame0 contact sheet shows the raw ground/sky views becoming upright people and near-level shoreline. `x4-dynamic-frame0-preroll-summary.json` retains structural/numerical evidence and the external image path. The source clock remains100fps/sourceStart0; original capture starts20.997882086sec. Disabling pre-roll preserves these clocks and returns `unavailable_dynamic_start` with a warning. Real frame225 remains on `aligned_initial_sample`, avoiding unnecessary history changes.

`preroll-source-tests-qualified.log`:25 actual-source-module tests passed (22 ordinary,3 explicitly supplied real-source checks). Added regressions cover bounded observed seeding, disabled/untrimmed/missing observation readiness, padded authored yaw and capture clocks, the upside-down quaternion singularity, and the actual X4 frame0/225 plus X6 cases. This supersedes the earlier21-test focused count. It remains a source-module qualification; the parent final batch owns whole-backend/API/UI validation.

## Dynamic-force safety correction (supersedes norm-only feedback)

Controlled Studio ON+AI diagnostic per-frame registration isolated a pitch/roll residual of approximately2.66° at1s and13.63° at3s. Integrating the same saved raw gyro without acceleration feedback reproduced that residual: the previous automatic2rad/s norm-only feedback was following dynamic specific force. Per-frame registrations are diagnostic only and excluded from parity measurements.

The independent fusion now requires a trailing observed0.45–0.5s window, at least10 samples, every force magnitude0.97..1.03g, angular speed≤0.1rad/s, and acceleration-vector RMS scatter≤0.003g. Even an admitted window must be within1° of the independent gyro-only gravity prediction. The TOTAL quaternion correction from that gyro-only trajectory is limited to0.25°, rather than a per-sample limit that could accumulate. Slow lateral-force ramps can therefore cause at most this explicit bounded uncertainty; constant-norm force and small sensor bias remain fundamentally ambiguous without external observations. The existing declared-stationary gyro-bias calibration is the route for larger observable bias corrections. Acceleration-disabled behavior, mounting, capture clocks and pre-roll time limits are unchanged.

`bounded-gravity-source-tests.log`:27 actual-source-module tests pass, including the original0.03rad/s for10s case with an explicit stationary declaration (full correction), small stationary bias, constant-norm lateral steps and3s/100s ramps, actual gyro tilt under dynamic force, and all three supplied real-source checks. The old automatic norm-only expectation of correcting an unobserved17° bias has intentionally been removed: that expectation conflated force with gravity.

A fresh actual-source3.1s analysis plus the previously fitted diagnostic rotations predicts residual orientation differences of approximately0.49° at1s and1.01° at3s. This is an analytic causal check, not a new production render or vendor equivalence certificate. Removing unsafe feedback also changes the clip0 tilt, so the initial absolute-gravity frame requires renewed independent validation.

Read-only inspection of original16..21s found ZERO admissible half-second windows among225 examined windows. The lowest scatter was approximately0.0338g, already more than ten times the admissible scatter. Widening pre-roll to5s therefore does not establish precise initial gravity. The existing50–75ms initialization is a usable approximate seed; it is not evidence of precise absolute horizon.

The final short-pre-roll status is `estimated_observed_preroll` with a nonempty warning, superseding the earlier `aligned_observed_preroll` label. Useful orientation is retained, while precise absolute gravity is explicitly unconfirmed. The default and maximum observed history remain one second.
