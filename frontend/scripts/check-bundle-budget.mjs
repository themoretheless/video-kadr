import { readdir, readFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { extname, join } from 'node:path'

const assetDirectory = new URL('../dist/assets/', import.meta.url)

// Two budgets, because they answer two different questions.
//
// The "initial" budget is what the browser downloads before first paint: the
// entry chunk and its stylesheet. That is the number a user feels.
//
// The "total" budget covers every chunk in dist/assets, lazily loaded editor
// panels included. It only guards against unbounded growth; a panel the user
// never opens is never fetched.
//
// The client-only build lazy-loads the small ffmpeg.wasm controller. The 31 MiB
// core is a separately cached runtime asset and is not counted here.
const budgets = {
  initialJs: Number(process.env.BUNDLE_BUDGET_INITIAL_JS_GZIP || 80 * 1024),
  initialCss: Number(process.env.BUNDLE_BUDGET_INITIAL_CSS_GZIP || 6 * 1024),
  js: Number(process.env.BUNDLE_BUDGET_JS_GZIP || 135 * 1024),
  css: Number(process.env.BUNDLE_BUDGET_CSS_GZIP || 12 * 1024),
  total: Number(process.env.BUNDLE_BUDGET_TOTAL_GZIP || 145 * 1024),
}

/// Vite names the entry chunk and its stylesheet `index-<hash>.<ext>`.
const isEntry = (file) => /^index-[^.]+\.(js|css)$/.test(file)

const files = await readdir(assetDirectory)
const sizes = { js: 0, css: 0 }
const initial = { js: 0, css: 0 }

for (const file of files.sort()) {
  const extension = extname(file).slice(1)
  if (extension !== 'js' && extension !== 'css') continue
  const contents = await readFile(join(assetDirectory.pathname, file))
  const compressedBytes = gzipSync(contents, { level: 9 }).byteLength
  sizes[extension] += compressedBytes
  if (isEntry(file)) initial[extension] += compressedBytes
  console.log(
    `${file}: ${(compressedBytes / 1024).toFixed(2)} KiB gzip${isEntry(file) ? ' (initial)' : ''}`,
  )
}

const total = sizes.js + sizes.css
const failures = [
  ['initial JavaScript', initial.js, budgets.initialJs],
  ['initial CSS', initial.css, budgets.initialCss],
  ['JavaScript', sizes.js, budgets.js],
  ['CSS', sizes.css, budgets.css],
  ['total', total, budgets.total],
].filter(([, actual, budget]) => actual > budget)

console.log(
  `Initial: JS ${(initial.js / 1024).toFixed(2)} KiB, CSS ${(initial.css / 1024).toFixed(2)} KiB gzip`,
)
console.log(
  `Bundle: JS ${(sizes.js / 1024).toFixed(2)} KiB, CSS ${(sizes.css / 1024).toFixed(2)} KiB, total ${(total / 1024).toFixed(2)} KiB gzip`,
)

if (failures.length) {
  for (const [label, actual, budget] of failures) {
    console.error(
      `${label} exceeds budget: ${(actual / 1024).toFixed(2)} > ${(budget / 1024).toFixed(2)} KiB gzip`,
    )
  }
  process.exitCode = 1
}
