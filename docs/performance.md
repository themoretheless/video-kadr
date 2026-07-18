# Performance workflow

Performance work starts from the versioned corpus in `bench/perf/corpus-v1.json`.
The release binary records cold and warm median/p95 for probe normalization,
library listing, render-cache hits, and `EditPlan` compilation together with the
commit, Rust version, target, and logical CPU count.

## Baseline

Run `make perf-baseline`. The internal report is written to
`bench/perf/results/corpus-latest.json`; when Hyperfine is installed, the same
four process-level workloads are written to `hyperfine-latest.json`. Compare
only matching workload schema and environment. A p95 increase above 20% is a
signal only after three repeated runs; one noisy sample is not a regression.

The encode matrix has three explicit policies: `interactive`, `balanced`, and
`quality`. Record wall time, peak RSS, output size, and an agreed quality metric
before changing their defaults. `EncodeBudget` rejects thread, tile, speed, and
memory combinations that exceed runtime/cgroup limits.

## Profile

Run `make perf-profile WORKLOAD=plan-compile` (or `probe`, `library-list`,
`cache-hit`). The command uses `cargo-flamegraph`, keeps deterministic SVG in
`bench/perf/profiles/<workload>.svg`, and uses `--post-process tee` to retain the
actual folded stacks in `<workload>.folded`. On macOS cargo-flamegraph uses
`xctrace`; on Linux it uses `perf`.

A performance change is accepted as a perf fix only when its review includes:

1. The before/after corpus JSON from the same environment.
2. The profile command and folded/SVG artifacts used to choose the hot path.
3. Median and p95, plus correctness and memory checks.
4. A note when the result is inconclusive or trades latency for memory/quality.

Generated result/profile files are ignored by default because they are machine
specific. Attach the relevant pair to the review or deliberately force-add a
named baseline when it is the agreed reference machine.
