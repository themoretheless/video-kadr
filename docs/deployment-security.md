# Deployment security ADR

Status: accepted on 2026-09-02.

## Decision

Video Kadr has three explicit deployment profiles:

- `local`: backend binds loopback and may use plain HTTP.
- `lan`: an authenticated reverse proxy terminates TLS; backend is reachable
  only on the private service network.
- `public`: TLS 1.2+ terminates at a managed reverse proxy/load balancer;
  plaintext ingress to either container is forbidden.

The backend does not currently derive identity, scheme, client IP, redirects,
or authorization from `Forwarded`/`X-Forwarded-*`. Those headers are ignored.
If a future feature needs them, it must add a configured CIDR/identity allowlist
for the single trusted proxy hop, reject multiple/ambiguous header chains, and
ship spoofing tests before the field may affect security decisions.

The Compose file is a local/LAN topology: only the frontend proxy publishes a
host port and the backend has no `ports` mapping. A public deployment must add
an external TLS proxy and authentication/rate-limit policy; changing
`BIND_ADDR=0.0.0.0` alone never creates a supported public profile.

## Enforced invariants

- The image defaults to `BIND_ADDR=127.0.0.1` and a non-root runtime user.
- No public plaintext profile exists in checked-in configuration.
- Public health probes run behind the same TLS boundary, not on a new port.
- HSTS is owned by the TLS terminator after HTTPS rollout is verified.
- Proxy access logs must use the repository redaction policy and never log URL
  query strings, credentials, uploaded filenames, or secret headers.

## Operational verification

Before promoting a public release: verify the certificate chain/renewal alarm,
HTTP-to-HTTPS redirect, direct-backend network denial, spoofed forwarded-header
rejection/ignore behavior, request-size/rate limits, and rollback. Record the
proxy version and configuration digest with the release evidence.
