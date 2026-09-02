#!/usr/bin/env python3
from __future__ import annotations

import datetime as dt
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[1]
today = dt.date.today()


def load(path: str) -> dict:
    with (ROOT / path).open("rb") as handle:
        return tomllib.load(handle)


def validate(path: str) -> set[str]:
    document = load(path)
    if document.get("schema_version") != 1:
        raise ValueError(f"{path}: unsupported schema_version")
    ids: set[str] = set()
    for index, exception in enumerate(document.get("exceptions", []), start=1):
        missing = {"id", "owner", "rationale", "expires", "tracking"} - exception.keys()
        if missing:
            raise ValueError(f"{path} exception {index}: missing {sorted(missing)}")
        if not all(isinstance(exception[field], str) and exception[field].strip() for field in ("id", "owner", "rationale", "tracking")):
            raise ValueError(f"{path} exception {index}: fields must be non-empty strings")
        expires = exception["expires"]
        if not isinstance(expires, dt.date):
            raise ValueError(f"{path} exception {index}: expires must be YYYY-MM-DD")
        if expires < today:
            raise ValueError(f"{path} exception {exception['id']}: expired on {expires}")
        if exception["id"] in ids:
            raise ValueError(f"{path}: duplicate exception {exception['id']}")
        ids.add(exception["id"])
    return ids


try:
    advisory_ids = validate("security/advisory-exceptions.toml")
    validate("security/trivy-exceptions.toml")
    audit_ignores = set(load(".cargo/audit.toml").get("advisories", {}).get("ignore", []))
    if audit_ignores != advisory_ids:
        raise ValueError(
            "RustSec audit ignores and security/advisory-exceptions.toml IDs must match exactly"
        )
except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
    print(error, file=sys.stderr)
    raise SystemExit(1)

print("Security exceptions: schemas valid, sets match, no expiry is overdue")
