#!/usr/bin/env python3
from __future__ import annotations

import pathlib

root = pathlib.Path(__file__).resolve().parents[1]
secrets = root / "ops" / "secrets"
allowed = {"README.md", ".gitkeep"}
violations = []
for path in secrets.iterdir():
    if path.is_dir() or path.name in allowed:
        continue
    if not path.name.endswith(".enc.yaml"):
        violations.append(f"plaintext-shaped secret file: {path.relative_to(root)}")
        continue
    text = path.read_text(encoding="utf-8", errors="replace")
    if "sops:" not in text or "ENC[" not in text:
        violations.append(f"not recognisable SOPS ciphertext: {path.relative_to(root)}")
if violations:
    raise SystemExit("\n".join(violations))
print("Secrets: no plaintext deploy files; checked ciphertext has SOPS metadata")
