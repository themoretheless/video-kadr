import { readdir, readFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { extname, join } from 'node:path'

const assetDirectory = new URL('../dist/assets/', import.meta.url)
const budgets = {
  // The client-only build lazy-loads the small ffmpeg.wasm controller. The
  // 31 MiB core is a separately cached runtime asset and is intentionally not
  // counted as application JavaScript here.
  js: Number(process.env.BUNDLE_BUDGET_JS_GZIP || 64 * 1024),
  css: Number(process.env.BUNDLE_BUDGET_CSS_GZIP || 5 * 1024),
  total: Number(process.env.BUNDLE_BUDGET_TOTAL_GZIP || 70 * 1024),
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
