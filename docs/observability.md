# Observability contract

The backend exposes OpenMetrics at `GET /metrics`. Metric labels are closed
enums only: resource class, state, result, tool, outcome, and direction. Job or
request IDs, URLs, filenames, titles, and user-provided values are forbidden as
labels. The exporter-neutral `TelemetryPort` keeps collection independent from
Prometheus and supplies noop, test, and Prometheus adapters.

Structured logs may promote only `service`, `env`, `level`, and `error_kind` to
Loki labels. Request, job, and trace IDs remain fields for correlation. URL,
path, authorization, token, cookie, and filename-shaped values must pass the
shared `RedactionTransform` before they enter logs or diagnostic bundles.

Durable jobs store a bounded trace link. Workers restore that link after the
SQLite queue boundary and record it on the job span. Ten percent of request
links are deterministically marked for detailed sampling; ordinary lifecycle
logs remain available without promoting trace IDs to labels.

Operational dashboards should cover queue wait, used/available permits,
terminal outcomes, process outcomes, cache hit/miss, and ingress/egress bytes.
Alerts use rates and saturation windows rather than per-user dimensions.
