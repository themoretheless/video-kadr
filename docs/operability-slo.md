# Operability SLO and reliability gates

The service reports user-visible paths separately; an average across them is
not an SLO. Release candidates use these initial objectives until production
traffic justifies a stricter, versioned policy.

| Path | Indicator | Objective | Window |
| --- | --- | --- | --- |
| API control calls | successful non-5xx latency p95 | <= 250 ms | rolling 28 days |
| First preview frame | request to verified frame p95 | <= 2 s | rolling 28 days |
| Export | queue to conformance-verified publication p95 | <= 2x media duration + 30 s | rolling 28 days |

Queue, probe, first-preview, render, and publish are distinct critical phases.
The runtime/CI histogram records p50/p95/p99 with coordinated-omission
correction; arithmetic averages are diagnostic only. Labels are bounded and
must never contain job IDs, source URLs, filenames, or tenant identities.

Resource shedding is class-aware: interactive preview and control requests do
not wait behind ingest/export, while saturated classes return a bounded
`Retry-After`. Retriable tool/source pairs have a finite budget and observable
closed/open/half-open state. Validation and security errors never retry.

Startup reconciliation owns pending/running durable jobs through request,
outbox, lease, event history and age. It requeues complete durable envelopes;
otherwise it records interruption and removes private request payloads. File
artifacts publish only after file fsync, atomic rename, and parent-directory
fsync. The failpoint matrix accepts only a complete old or complete new
manifest and leaves no partial temporary artifact.

Performance comparison is allowed only for matching schema, tool version,
target and logical CPU count. A release regression requires the median result
of exactly three candidate runs to cross the configured median or p95
threshold; one noisy run cannot fail CI.
