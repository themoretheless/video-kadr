# Procedural transition kernel pack

Adapted from Filmcraft commit `5231852443363f001c3f6b396dd9b1e6461ae2be`, MIT (see LICENSE-MIT). Contains the original 112-transition mathematical implementations and minimal image, affine, normal compositor, and Gaussian helpers. Parameter definitions come from the same clean reference through `scripts/generate-filmcraft-transition-definitions-oracle.py`.

Registered in the backend domain and native flat compositor through `TransitionSpec` and `transition_procedural`. New procedural export from the frontend remains gated pending full-scene and HTTP delivery qualification. Working input/output conversion and all 20244 saved default/control frames have matched the original f32 bits through the native adapter. This is bounded fixture evidence, not full runtime/profile/scale qualification.

`apply` is a low-level working-pixel function requiring valid image shape and lengths. It is not a client boundary: unknown IDs inherit the original dissolve fallback, which typed `TransitionSpec` and `RenderContext` reject. The owned context validates image shape, finite pixels and pixel count, and uses a separate 1..4-worker pool with cooperative cancellation. A caller-thread checkpoint monitors scoped work without moving non-Send callbacks.

Strict pixel-storage capacity accounting is connected to Image buffers, clones, Gaussian row scratch, and borrowed input storage. An isolated candidate passed the complete oracle and actual budget/release tests before integration. Metadata, worker overhead and the rest of the host pipeline are outside that ledger. Full-size peak memory and cancellation latency still require measurement.
