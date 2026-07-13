# Threat model: local media upload

Status: implemented and regression-tested on 14 July 2026. This model follows
the layered controls in the
[OWASP File Upload Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html).
It applies to `POST /api/upload`; URL import has a separate SSRF/egress boundary.

## Trust boundary

```text
untrusted browser multipart
  -> Axum whole-request limit and upload semaphore
  -> storage/staging/<server UUID>.upload (private)
  -> bounded ffprobe and container/codec allowlist
  -> atomic rename to storage/sources/<server UUID>.<probed extension>
  -> /files/sources with nosniff and sandbox CSP
```

The client filename and declared part MIME are metadata, never authority. A file
is public only after probing succeeds. `storage/staging`, the database, and other
storage files have no HTTP mount.

## Security invariants

1. No client-controlled path or extension reaches published storage.
2. Declared MIME alone can neither accept nor reject content.
3. Unprobed or unsupported content is never served by `/files`.
4. Every failed, timed-out, cancelled, or disconnected upload loses its staging file.
5. Request bytes and concurrent upload count are bounded independently from render jobs.
6. Published responses cannot opt into browser MIME sniffing or active-document execution.

## Control matrix

| Threat or hostile fixture | Enforced control | Regression test |
|---|---|---|
| Valid MP4 named `payload.html` with `application/octet-stream` | Ignore client extension; derive suffix from `ffprobe`; publish under UUID | `published_name_and_content_type_come_from_probe_not_client_name` |
| HTML/script bytes named `.mp4` and declared `video/mp4` | Treat declared MIME as untrusted; require successful probe and allowlisted container | `spoofed_extension_and_declared_mime_cannot_publish_non_media` |
| Path-like, colliding, or executable client filename | Generate UUID server-side; original basename is display-only | `published_name_and_content_type_come_from_probe_not_client_name` |
| Partial body followed by TCP disconnect | Private staging plus `StagingFile` drop cleanup; multipart/receive errors remove partial data | `disconnected_upload_is_removed_from_private_staging` |
| Direct request for a quarantined file | Mount only `sources/` and `outputs/`; never mount `staging/` or storage root | `staging_is_not_served_and_limits_fail_closed` |
| Oversized request body | Route-local `DefaultBodyLimit`; reject with JSON `413 payload_too_large` | `staging_is_not_served_and_limits_fail_closed` |
| Concurrent slow uploads starving render/download work | Independent bounded upload semaphore; fail fast with `429` and recover the permit | `upload_slots_limit_concurrency_and_recover` |
| Empty or missing file part | Reject before probe/publish and leave no media entry | `empty_or_missing_file_fields_are_rejected` |
| Browser executes a valid upload under a misleading name | Probed media `Content-Type`, `X-Content-Type-Options: nosniff`, sandbox CSP | `published_name_and_content_type_come_from_probe_not_client_name` |

The focused suite is
`backend/tests/upload_security.rs`; shared HTTP setup is in
`backend/tests/support/mod.rs`. Lower-level timeout and extension mapping tests
remain beside `backend/src/handlers/upload.rs`.

## Residual risk

- The service is single-tenant and has no authentication. Keep the default
  loopback bind; an externally exposed deployment still requires auth and ownership.
- Request bytes and concurrency are bounded, but there is no aggregate disk quota,
  per-user quota, media-duration cap, or decoded-pixel budget yet.
- `ffprobe` has a wall-clock timeout but no dedicated OS sandbox/cgroup profile.
  A parser exploit or memory-heavy valid file therefore remains a host risk.
- The handler accepts the first file field. It does not yet reject a request that
  appends extra file parts after it.
- Declared MIME is intentionally not an early allow/deny gate. Adding it may save
  work, but it must remain only a hint because clients can forge it.
- No antivirus/CDR stage is enabled. That is an explicit deployment decision, not
  a substitute for probe, generated names, private staging, and response headers.

Run the matrix with:

```bash
cd backend
cargo test --test upload_security
```
