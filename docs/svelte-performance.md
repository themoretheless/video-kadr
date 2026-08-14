# Vue vs Svelte migration benchmark

Generated: 2026-08-14T10:49:07.005Z
Measured implementations: Vue, Svelte

This is a local, headless Chromium comparison of the same product fixture, not a claim about every Vue or Svelte application. Negative deltas mean that Svelte reported a lower value. Timing and byte metrics favor lower values; resource and DOM counts are descriptive. Missing paint entries are rendered as `—` rather than estimated.

## Conclusions

- Production JS + CSS (gzip): Vue 61.79 KiB, Svelte 49.69 KiB; Svelte is 19.6% lower.
- Production build median: Vue 631.91 ms, Svelte 740.46 ms; Svelte is 17.2% higher.
- Offline-shell FCP median: Vue 32.00 ms, Svelte 32.00 ms; Svelte is the same at the reported precision.
- Import-to-editor ready median: Vue 98.60 ms, Svelte 97.00 ms; Svelte is 1.6% lower.

Browser medians in this loopback microbenchmark are sensitive to scheduler and thermal noise; the raw samples and paired medians are evidence for these implementations on this host, not a universal framework ranking.

## Post-migration bundle verification

After the paired run, the canonical Svelte build gained the missing interactive
curve-editor handlers/tests and chroma-key controls. Its final verified bundle is
47.99 KiB JavaScript + 4.79 KiB CSS = 52.77 KiB gzip. That is 14.6% below the
historical Vue bundle of 61.79 KiB, but it is not a new paired benchmark because
the post-migration Svelte tree contains additional feature work. The tables below
remain the exact, reproducible results of the original same-fixture comparison.

## Build and bundle

| Metric | Vue | Svelte | Svelte vs Vue |
|---|---:|---:|---:|
| Build wall time, median | 631.91 ms | 740.46 ms | +17.2% |
| JavaScript, raw | 161.56 KiB | 129.31 KiB | -20.0% |
| JavaScript, gzip | 56.91 KiB | 44.98 KiB | -21.0% |
| CSS, raw | 21.83 KiB | 20.38 KiB | -6.6% |
| CSS, gzip | 4.88 KiB | 4.71 KiB | -3.5% |
| JS + CSS, raw | 183.38 KiB | 149.68 KiB | -18.4% |
| JS + CSS, gzip | 61.79 KiB | 49.69 KiB | -19.6% |

## Offline shell

| Metric | Vue | Svelte | Svelte vs Vue |
|---|---:|---:|---:|
| DOMContentLoaded | 18.30 ms | 15.50 ms | -15.3% |
| Load | 21.20 ms | 18.30 ms | -13.7% |
| First Contentful Paint | 32.00 ms | 32.00 ms | +0.0% |
| Largest Contentful Paint | 32.00 ms | 32.00 ms | +0.0% |
| Navigation transfer | 0.69 KiB | 0.74 KiB | +6.6% |
| Resource transfer | 63.12 KiB | 51.00 KiB | -19.2% |
| Total transfer | 63.81 KiB | 51.74 KiB | -18.9% |
| Total decoded | 183.85 KiB | 150.19 KiB | -18.3% |
| Resource count | 4 | 4 | +0.0% |
| DOM nodes after settle | 36 | 37 | +2.8% |

## Mocked editor

| Metric | Vue | Svelte | Svelte vs Vue |
|---|---:|---:|---:|
| DOMContentLoaded | 17.90 ms | 15.20 ms | -15.1% |
| Load | 20.60 ms | 18.10 ms | -12.1% |
| First Contentful Paint | 32.00 ms | 32.00 ms | +0.0% |
| Largest Contentful Paint | 32.00 ms | 32.00 ms | +0.0% |
| Navigation transfer | 0.69 KiB | 0.74 KiB | +6.6% |
| Resource transfer | 65.33 KiB | 53.20 KiB | -18.6% |
| Total transfer | 66.02 KiB | 53.94 KiB | -18.3% |
| Total decoded | 184.88 KiB | 151.22 KiB | -18.2% |
| Resource count | 8 | 8 | +0.0% |
| DOM nodes after settle | 248 | 250 | +0.8% |
| Import to editor ready | 98.60 ms | 97.00 ms | -1.6% |

## Environment

- Host: local darwin arm64 25.6.0; Apple M4 Max; 16 logical CPUs.
- Runtime: Node v26.7.0, npm 11.19.0, Chromium 149.0.7827.55.
- Vue implementation: Vue 3.5.35, Vite 6.4.3.
- Svelte implementation: Svelte 5.56.8, Vite 6.4.3.
- Git: `70ccd848bd94eba3a32b387133c17b5311909bc8`; working tree dirty during measurement.

## Method

- Build: `dist` is removed before every `npm run build`; dependencies and OS caches remain warm. Framework order alternates each round. Median of 5 repetitions.
- Bundle: sum of every production `.js`/`.mjs` and `.css` asset. Gzip level 9 is applied per file, matching separate HTTP transfers.
- Browser: production Vite previews on fixed loopback ports, headless Chromium, fresh context per sample, 1440×900 viewport, service workers blocked. Median of 7 repetitions.
- Offline shell: every `/api/**` request returns the same HTTP 503 fixture.
- Mocked editor: identical capabilities/import/job/library fixtures; measured from clicking Import until `Fixture clip` is rendered. Its FCP/LCP describe the initial navigation; the Import click finalizes Core Web Vitals LCP before the editor appears.
- Navigation and subresource transfers are separate in JSON and combined in the table totals. LCP is observer-based and remains null when Chromium does not expose a reliable entry.

## Decision record and limitations

This paired run was captured immediately before Svelte replaced Vue at the canonical `frontend/` path. The repository now intentionally contains only the selected Svelte implementation, so the historical Vue/Svelte run is not exposed as a command that would require retaining both applications.

The fixture intentionally removes backend and network variability. It does not replace field Core Web Vitals, low-end-device profiling, memory/leak analysis, interaction traces against real video, or a feature-parity audit.

The exact raw samples and environment metadata from the decision run are preserved in [svelte-performance-data.json](svelte-performance-data.json).
