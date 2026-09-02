# Backup chunk-dedup decision

Date: 2026-09-02. Command: `python3 bench/backup_dedup.py`.

The deterministic 45.3 MiB logical corpus contains four revisions of a
12 MiB media-like source: unchanged content, a 192 KiB in-place color patch, a
300 KiB inserted scene, and a trimmed revision. This intentionally exercises
the boundary-shift failure mode that whole-file backup cannot deduplicate.

| Chunking policy | Stored | Dedup ratio | Bytes saved | Unique / total chunks |
| --- | ---: | ---: | ---: | ---: |
| Fixed 1 MiB | 21.3 MiB | 2.127x | 53.0% | 22 / 46 |
| Content-defined (256 KiB–4 MiB, 1 MiB target) | 18.4 MiB | 2.458x | 59.3% | 15 / 42 |

Decision: do not add Borg as a runtime dependency. The current verified
snapshot remains the source of truth; a future repository adapter may consume
that snapshot. A dependency proposal must reproduce at least 2.2x dedup on this
corpus and complete a restore/check drill.

Prune policy for that future adapter:

- retain 7 daily, 5 weekly, and 12 monthly verified snapshots;
- never prune the newest successful snapshot or the last successful restore-drill snapshot;
- run integrity check before prune and compact only after prune completes;
- stop on lock, checksum, or repository-health errors;
- keep staging and incomplete snapshots outside retention accounting.

The benchmark is synthetic and measures storage efficiency, not throughput or
real camera entropy. A dependency ADR still needs a representative local-media
run, wall time, peak RSS, encrypted-repository overhead, and restore timing.
