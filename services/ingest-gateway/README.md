# Live ingest gateway

This service is the only process that accepts live SRT publishers. It records
bounded MPEG-TS segments and exposes no editor API. MediaMTX invokes the handoff
hook only when a segment is complete; the hook hard-links the closed segment,
hashes it, and atomically publishes a versioned JSON manifest. The editor may
ingest only artifacts that have both the closed file and final `.json` manifest.

MediaMTX is pinned to `1.20.0`. Its UDP listener is bound to localhost by the
Compose profile; a public deployment must add authentication, TLS/VPN ingress,
and an explicit firewall policy. Recording retention is 24 hours. Immutable
artifacts are owned by the editor/backup retention policy and are not pruned by
MediaMTX.

Run `docker compose config` to validate the profile. Publish SRT to
`srt://127.0.0.1:8890?streamid=publish:<stream>` and consume only finalized
manifests under `/var/lib/ingest/artifacts`.
