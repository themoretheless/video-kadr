#!/usr/bin/env python3
"""Deterministic media-revision dedup benchmark; uses only stdlib."""

import hashlib
import json
import random

MIB = 1024 * 1024
GEAR = [int.from_bytes(hashlib.sha256(bytes([i])).digest()[:8], "big") for i in range(256)]


def scene(seed: int, size: int = 2 * MIB) -> bytes:
    rng = random.Random(seed)
    return rng.randbytes(size)


def corpus() -> list[bytes]:
    base = b"".join(scene(seed) for seed in range(6))
    color_patch = bytearray(base)
    color_patch[5 * MIB : 5 * MIB + 192 * 1024] = scene(99, 192 * 1024)
    inserted = base[: 4 * MIB] + scene(77, 300 * 1024) + base[4 * MIB :]
    trimmed = base[MIB : 10 * MIB]
    return [base, bytes(color_patch), inserted, trimmed]


def fixed(data: bytes) -> list[bytes]:
    return [data[start : start + MIB] for start in range(0, len(data), MIB)]


def content_defined(data: bytes) -> list[bytes]:
    minimum, maximum, mask = 256 * 1024, 4 * MIB, (1 << 20) - 1
    chunks, start, rolling = [], 0, 0
    for index, byte in enumerate(data, 1):
        rolling = ((rolling << 1) + GEAR[byte]) & ((1 << 64) - 1)
        size = index - start
        if size >= minimum and ((rolling & mask) == 0 or size >= maximum):
            chunks.append(data[start:index])
            start, rolling = index, 0
    if start < len(data):
        chunks.append(data[start:])
    return chunks


def measure(chunker) -> dict[str, float | int]:
    snapshots = corpus()
    logical = sum(map(len, snapshots))
    unique: dict[bytes, int] = {}
    chunks = 0
    for snapshot in snapshots:
        for chunk in chunker(snapshot):
            chunks += 1
            digest = hashlib.sha256(chunk).digest()
            unique.setdefault(digest, len(chunk))
    stored = sum(unique.values())
    return {
        "snapshots": len(snapshots),
        "logicalBytes": logical,
        "storedBytes": stored,
        "chunks": chunks,
        "uniqueChunks": len(unique),
        "dedupRatio": round(logical / stored, 3),
        "savedPercent": round((1 - stored / logical) * 100, 1),
    }


print(json.dumps({"fixed1MiB": measure(fixed), "contentDefined": measure(content_defined)}, indent=2))
