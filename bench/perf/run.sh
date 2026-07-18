#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RESULTS="${PERF_RESULTS_DIR:-$ROOT/bench/perf/results}"
BIN="$ROOT/backend/target/release/perf_corpus"
mkdir -p "$RESULTS"

export GIT_COMMIT="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || printf unknown)"
if test -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)"; then
  export GIT_DIRTY=true
else
  export GIT_DIRTY=false
fi
export RUSTC_VERSION="$(rustc --version)"
cargo build --manifest-path "$ROOT/backend/Cargo.toml" --release --bin perf_corpus
"$BIN" > "$RESULTS/corpus-latest.json"

if command -v hyperfine >/dev/null 2>&1; then
  hyperfine --warmup 2 --runs 10 --export-json "$RESULTS/hyperfine-latest.json" \
    --command-name probe "$BIN --only probe >/dev/null" \
    --command-name library-list "$BIN --only library-list >/dev/null" \
    --command-name cache-hit "$BIN --only cache-hit >/dev/null" \
    --command-name plan-compile "$BIN --only plan-compile >/dev/null"
else
  printf 'hyperfine is not installed; internal median/p95 report is available at %s\n' \
    "$RESULTS/corpus-latest.json"
fi
