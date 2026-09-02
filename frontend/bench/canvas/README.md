# Canvas renderer baseline

Run `npm run bench:canvas` from `frontend/`. The fixed `long-timeline-v1`
workload synchronously renders 2,500 items for 60 frames in headless Chromium.

Baseline captured on 2026-09-02 on the local development host:

| Renderer | p50 | p95 |
| --- | ---: | ---: |
| DOM | 5.7 ms | 8.8 ms |
| Canvas2D | 0.1 ms | 0.2 ms |
| WebGL | 0.0 ms | 0.1 ms |

These numbers compare renderer overhead on one machine; they are not a product
SLO. The current product path remains DOM/Canvas2D. WebGL adoption requires a
representative interactive workload, a documented threshold, context-loss
fallback, and three comparable runs on the supported browser matrix.
