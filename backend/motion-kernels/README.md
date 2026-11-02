# Native motion kernels

Shi–Tomasi features, pyramidal Lucas–Kanade, forward/backward filtering and
RANSAC adapted from the pinned Filmcraft source; MIT attribution is in LICENSE.

The backend keeps its existing public point/affine API in
`src/domain/motion_tracker.rs`. The adapter copies coordinates and matrix fields;
warp correction, smoothing and cover calculations stay in the backend. Dev
optimization applies to this crate only, with assertions and overflow checks
retained. Root and crate tests preserve source sampling/pyramid/feature digests
and tracking behavior.
