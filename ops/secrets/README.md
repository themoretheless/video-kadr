# Encrypted deployment secrets

No deploy secret is stored in this repository today. When one is introduced,
only `*.enc.yaml` SOPS ciphertext may be committed here; plaintext files and age
private keys are rejected by CI. Keys live in the deployment platform KMS or an
external age key service, never in GitHub variables containing the private key.

Encrypt with an explicit external recipient:

```sh
sops --encrypt --age "$SOPS_AGE_RECIPIENTS" secret.yaml > secret.enc.yaml
sops --decrypt secret.enc.yaml >/dev/null
```

Rotation drill (quarterly and after membership changes): create a new external
recipient, decrypt only in a tmpfs/CI secret mount, re-encrypt every `*.enc.yaml`,
verify the old key can no longer decrypt, deploy, inspect redacted logs, and
destroy the plaintext mount. Record operator/date/artifact digests in the
private operations tracker. Never redirect decrypted content into the repo.
