#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="${PERF_RESULTS_DIR:-$ROOT/bench/perf/profiles}"
WORKLOAD="${1:-plan-compile}"
case "$WORKLOAD" in
  probe|library-list|cache-hit|plan-compile) ;;
  *) printf 'unknown workload: %s\n' "$WORKLOAD" >&2; exit 2 ;;
esac
command -v cargo-flamegraph >/dev/null 2>&1 || {
  printf 'cargo-flamegraph is required: cargo install flamegraph\n' >&2
  exit 2
}
mkdir -p "$RESULTS"
export CARGO_PROFILE_RELEASE_DEBUG=true
export PERF_WARM_ITERATIONS="${PERF_WARM_ITERATIONS:-5000}"
export PERF_COLD_ITERATIONS="${PERF_COLD_ITERATIONS:-200}"

cd "$ROOT/backend"
cargo flamegraph --deterministic --bin perf_corpus \
  --output "$RESULTS/$WORKLOAD.svg" \
  --post-process "tee $RESULTS/$WORKLOAD.folded" \
  -- --only "$WORKLOAD"
