# Continuous fuzzing

The five targets cover edit normalization, path tokens from multipart/archive
boundaries, persisted library JSON, URL policy, and render-cache identity.

Run a bounded local smoke pass from `backend/`:

```sh
for target in edit_normalization multipart_path library_json url_policy cache_key; do
  cargo fuzz run "$target" -- -max_total_time=30
done
```

Seed corpora are reviewed source. Any minimized crash must first become a
regression fixture/test in the production crate, then land in the matching
corpus. Security-impacting crashes have a 1 business-day acknowledgement and
7-day remediation target; other reproducible crashes have 3/30 days.
