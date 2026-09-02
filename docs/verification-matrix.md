# Verification matrix

The default development loop remains `cargo test --all-features`. Extended
lanes isolate resource-heavy and host-sensitive checks without weakening their
failure semantics.

| Lane | Command / profile | Isolation and policy | Pinned tool |
| --- | --- | --- | --- |
| Unit and integration | `cargo nextest run --all-features --profile ci` | All failures are collected; a flaky retry is still a failure; JUnit is written under `target/nextest/ci/` | cargo-nextest 0.9.137 |
| Media | `cargo nextest run --all-features --profile media` | FFmpeg binaries share a one-process group and a two-thread profile | FFmpeg version is printed by the runner; CI image is rebuilt per run |
| Slow persistence/property | `cargo nextest run --all-features --profile slow` | SQLite writers are serialized and receive a bounded slow timeout | cargo-nextest 0.9.137 |
| Flaky diagnosis | `cargo nextest run --all-features --profile flaky` | Two retries aid diagnosis, but `flaky-result = "fail"` prevents a recovered retry from passing the gate | cargo-nextest 0.9.137 |
| Arithmetic proofs | `cargo kani --lib` | Pure bounded frame/sample/tick/range/chunk harnesses; no service dependencies | Kani 0.67.0 |
| Critical mutations | `cargo mutants --in-place --build-timeout 120` | Only artifact verification, arithmetic, edit validation, retry policy and redaction are mutated; their library and property-contract tests form the focused gate, while the full suite remains the preceding Nextest lane; CI runs in a disposable checkout so the shared cross-language fixture corpus remains visible; every survivor fails and the report is retained | cargo-mutants 27.1.0, cargo-nextest 0.9.137 |
| Golden media | `cargo test --all-features --test media_corpus` | Lavfi creates disposable VFR/HDR/rotation/5.1/subtitle/corrupt media; real ffprobe metadata is compared with the normalized domain | FFmpeg/ffprobe reported by the runner |

The golden corpus manifest is versioned at
`backend/tests/fixtures/media-corpus-v1/manifest.json`. Its fixtures are source
generated and CC0; no third-party media enters the repository. Existing real
render and composition integration tests execute compiled FFmpeg commands and
assert pixels, durations, streams and metadata, providing the differential
adapter boundary rather than checking argument strings alone.

## Current local evidence

On 2026-09-02, the pinned Rust `1.89.0` gate passed strict Clippy and the full
Nextest profile: 524/524 tests, including loopback transport, real FFmpeg,
generated properties, golden media and HTTP contracts. FFmpeg/ffprobe were
`9.0.1`; Kani `0.67.0` completed all five harnesses without a counterexample.

The focused mutation campaign enumerates 144 current mutants. Iterative runs
caught every current mutant; the final incremental run had 3 caught, 0 missed,
0 timeout, while the union of prior caught results covers the full current set.
CI repeats the version-pinned Nextest, Kani and cargo-mutants lanes; retained CI
artifacts remain the source of truth for cross-platform and fresh-checkout
mutation results.

Frontend validation on the same checkout passed ESLint, Svelte type checking,
350/350 Vitest tests, production and Storybook builds, design/lifecycle/bundle
policies, 10/10 Chromium/WebKit smoke and stateful axe scenarios, and 7/7
Storybook visual contracts.
