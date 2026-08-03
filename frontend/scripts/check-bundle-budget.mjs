import { readdir, readFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { extname, join } from 'node:path'

const assetDirectory = new URL('../dist/assets/', import.meta.url)
const budgets = {
  // The client-only build lazy-loads the small ffmpeg.wasm controller. The
  // 31 MiB core is a separately cached runtime asset and is intentionally not
  // counted as application JavaScript here. The 7 KiB fingerprint worker is
  // counted together with its main-thread Safari fallback: they provide
  // bounded-memory SHA-256 for durable large media. The immutable autosave
  // journal, checksum validation and accessible recovery dialog are also part
  // of the offline bootstrap and intentionally remain available before FFmpeg.
  // Project-level missing-media discovery, race-safe batch relink and its
  // progress UI add another small always-available recovery path.
  // The resource planner, disposable-engine watchdog, WORKERFS/bounded MEMFS
  // policy and chunk-streaming FSA fallback are the #85 crash-safety layer.
  // The #84 persistent DAG/task-center contract adds IndexedDB recovery,
  // priority/cancel controls and the server queue adapter.
  // The #82 proxy selector adds a durable artifact registry, provenance
  // resolver and fail-closed original fallback to the preview bootstrap.
  // Features #83-#93 add the offline editor shell, colour pipeline and the
  // client-side multicam synchronisation/flatten contract. Keep a narrow
  // measured ceiling so future growth still fails CI instead of silently
  // turning this historical pre-#83 baseline into a permanently red gate.
  // Feature #94 adds the always-available IndexedDB export queue, immutable
  // batch definitions, recovery runner and accessible task controls.
  // Feature #95 adds the shared target-size estimator and bitrate solver used
  // by both the live export controls and the fail-closed batch queue path.
  js: Number(process.env.BUNDLE_BUDGET_JS_GZIP || 172.75 * 1024),
  css: Number(process.env.BUNDLE_BUDGET_CSS_GZIP || 8.5 * 1024),
  total: Number(process.env.BUNDLE_BUDGET_TOTAL_GZIP || 181 * 1024),
}

const files = await readdir(assetDirectory)
const sizes = { js: 0, css: 0 }

for (const file of files.sort()) {
  const extension = extname(file).slice(1)
  if (extension !== 'js' && extension !== 'css') continue
  const contents = await readFile(join(assetDirectory.pathname, file))
  const compressedBytes = gzipSync(contents, { level: 9 }).byteLength
  sizes[extension] += compressedBytes
  console.log(`${file}: ${(compressedBytes / 1024).toFixed(2)} KiB gzip`)
}

const total = sizes.js + sizes.css
const failures = [
  ['JavaScript', sizes.js, budgets.js],
  ['CSS', sizes.css, budgets.css],
  ['total', total, budgets.total],
].filter(([, actual, budget]) => actual > budget)

console.log(
  `Bundle: JS ${(sizes.js / 1024).toFixed(2)} KiB, CSS ${(sizes.css / 1024).toFixed(2)} KiB, total ${(total / 1024).toFixed(2)} KiB gzip`,
)

if (failures.length) {
  for (const [label, actual, budget] of failures) {
    console.error(`${label} exceeds budget: ${(actual / 1024).toFixed(2)} > ${(budget / 1024).toFixed(2)} KiB gzip`)
  }
  process.exitCode = 1
}
