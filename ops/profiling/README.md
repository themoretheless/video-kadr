# Staging profiling

Continuous profiling is staging-only. The deployment must reject this policy if
the environment is not exactly `staging`, bind Parca to loopback, retain data
for seven days, and expose it only through authenticated operator tunnelling.
Production enables neither Parca nor tokio-console.

Build the backend with symbolized optimized code:

```sh
cargo build --profile profiling
```

For every PR labelled `performance` or `concurrency`, capture the same replay
corpus for 15 minutes on the baseline commit and 15 minutes on the candidate.
Attach both Parca profile links, total samples, top changed frames, and the
measured wall/RSS result. A profile without a same-corpus baseline is not a
performance claim.

## Tokio task-leak drill

Build staging diagnostics explicitly:

```sh
RUSTFLAGS="--cfg tokio_unstable" cargo build --profile profiling --features tokio-console
```

Start with `DEPLOY_ENV=staging`, `ENABLE_TOKIO_CONSOLE=true`, and
`TOKIO_CONSOLE_BIND=127.0.0.1:6669`. Tunnel the loopback port, reproduce the
workload, then sort tasks by idle duration and poll count. Capture task name,
spawn location, busy/idle time, and resource class. Cancel the workload and
verify the task count returns to baseline within 30 seconds. Disable the
console and destroy the staging instance after the drill.
