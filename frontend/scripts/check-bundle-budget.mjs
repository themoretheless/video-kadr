import { readdir, readFile } from 'node:fs/promises'
import { gzipSync } from 'node:zlib'
import { extname, join } from 'node:path'

const distDirectory = new URL('../dist/', import.meta.url)
const assetDirectory = new URL('../dist/assets/', import.meta.url)
const manifest = JSON.parse(await readFile(new URL('.vite/manifest.json', distDirectory), 'utf8'))

// Feature-adjusted guardrail after the editor was split at the Multitrack
// boundary (2026-08-14). The accessible headless player/canvas controls added
// on 2026-09-02 moved 3 KiB of the unchanged total allowance from CSS headroom
// to JavaScript. Startup and whole-product bytes are intentionally
// separate: adding an optional editor must not masquerade as startup cost, but
// lazy chunks are still bounded individually and in aggregate. Wave 10's
// measured TanStack Query cache added 2.1 KiB to startup and 7.2 KiB overall;
// the guardrail moved by 3/8 KiB respectively instead of silently disabling it.
const budgets = {
  initialJs: envBytes('BUNDLE_BUDGET_INITIAL_JS_GZIP', 'BUNDLE_BUDGET_JS_GZIP', 115 * 1024),
  initialCss: envBytes('BUNDLE_BUDGET_INITIAL_CSS_GZIP', 'BUNDLE_BUDGET_CSS_GZIP', 12 * 1024),
  initialTotal: envBytes('BUNDLE_BUDGET_INITIAL_TOTAL_GZIP', undefined, 124 * 1024),
  allJs: envBytes('BUNDLE_BUDGET_ALL_JS_GZIP', undefined, 171 * 1024),
  allCss: envBytes('BUNDLE_BUDGET_ALL_CSS_GZIP', undefined, 16 * 1024),
  allTotal: envBytes('BUNDLE_BUDGET_ALL_TOTAL_GZIP', 'BUNDLE_BUDGET_TOTAL_GZIP', 184 * 1024),
  asyncJsChunk: envBytes('BUNDLE_BUDGET_ASYNC_JS_CHUNK_GZIP', undefined, 56 * 1024),
}

const files = await readdir(assetDirectory)
const compressed = new Map()
let productionJavaScript = ''
for (const file of files.sort()) {
  const extension = extname(file).slice(1)
  if (extension !== 'js' && extension !== 'css') continue
  const contents = await readFile(join(assetDirectory.pathname, file))
  compressed.set(`assets/${file}`, gzipSync(contents, { level: 9 }).byteLength)
  if (extension === 'js') productionJavaScript += contents.toString('utf8')
}

// CaptureController and VoiceoverPanel are compiled in different modules. The
// shared options key must remain stable across that boundary: esbuild property
// mangling once turned the constructor read into `.e` while the caller still
// emitted the literal `sourceAdapter`, silently disabling voiceover capture.
const sourceAdapterReferences = productionJavaScript.match(/\bsourceAdapter\b/g)?.length ?? 0
if (productionJavaScript.includes('video-kadr-render-dag-devtools')) {
  throw new Error('Development render DAG visualizer leaked into the production bundle')
}
const missingSourceAdapterEdges = [
  ['constructor', /this\.sourceAdapter=/],
  ['acquire', /\.sourceAdapter\.acquire\(/],
  ['prepare', /\.sourceAdapter\.prepare\(/],
  ['caller', /[,{]sourceAdapter:/],
].filter(([, pattern]) => !pattern.test(productionJavaScript)).map(([edge]) => edge)
if (missingSourceAdapterEdges.length) {
  throw new Error(
    `Production contract sourceAdapter was mangled across modules (missing ${missingSourceAdapterEdges.join(', ')})`,
  )
}
console.log(`Production contract: sourceAdapter preserved (${sourceAdapterReferences} references)`)

const entries = Object.entries(manifest).filter(([, record]) => record.isEntry)
if (entries.length === 0) throw new Error('Vite manifest has no entry chunk')

const initialFiles = new Set()
const visited = new Set()
for (const [key] of entries) collectStaticAssets(key, manifest, initialFiles, visited)
const allFiles = new Set(compressed.keys())
const asyncFiles = new Set([...allFiles].filter((file) => !initialFiles.has(file)))

const initial = summarize(initialFiles, compressed)
const all = summarize(allFiles, compressed)
const failures = [
  ['initial JavaScript', initial.js, budgets.initialJs],
  ['initial CSS', initial.css, budgets.initialCss],
  ['initial total', initial.total, budgets.initialTotal],
  ['all JavaScript', all.js, budgets.allJs],
  ['all CSS', all.css, budgets.allCss],
  ['all assets total', all.total, budgets.allTotal],
]

for (const file of [...initialFiles].sort()) logAsset('initial', file, compressed)
for (const file of [...asyncFiles].sort()) {
  logAsset('async', file, compressed)
  if (file.endsWith('.js')) failures.push([`async chunk ${file}`, compressed.get(file) ?? 0, budgets.asyncJsChunk])
}

console.log(
  `Initial: JS ${kib(initial.js)}, CSS ${kib(initial.css)}, total ${kib(initial.total)} gzip`,
)
console.log(
  `All chunks: JS ${kib(all.js)}, CSS ${kib(all.css)}, total ${kib(all.total)} gzip`,
)

const exceeded = failures.filter(([, actual, budget]) => actual > budget)
if (exceeded.length) {
  for (const [label, actual, budget] of exceeded) {
    console.error(`${label} exceeds budget: ${kib(actual)} > ${kib(budget)} gzip`)
  }
  process.exitCode = 1
}

function collectStaticAssets(key, records, result, seen) {
  if (seen.has(key)) return
  seen.add(key)
  const record = records[key]
  if (!record) throw new Error(`Vite manifest references missing chunk ${key}`)
  if (record.file) result.add(record.file)
  for (const css of record.css ?? []) result.add(css)
  for (const imported of record.imports ?? []) collectStaticAssets(imported, records, result, seen)
}

function summarize(selected, sizes) {
  let js = 0
  let css = 0
  for (const file of selected) {
    const size = sizes.get(file)
    if (size === undefined) throw new Error(`Manifest asset ${file} is missing from dist/assets`)
    if (file.endsWith('.js')) js += size
    else if (file.endsWith('.css')) css += size
  }
  return { js, css, total: js + css }
}

function logAsset(category, file, sizes) {
  console.log(`[${category}] ${file}: ${kib(sizes.get(file) ?? 0)} gzip`)
}

function envBytes(primary, legacy, fallback) {
  const raw = process.env[primary] ?? (legacy ? process.env[legacy] : undefined)
  if (raw === undefined) return fallback
  const value = Number(raw)
  if (!Number.isFinite(value) || value <= 0) throw new Error(`${primary} must be a positive byte count`)
  return value
}

function kib(bytes) {
  return `${(bytes / 1024).toFixed(2)} KiB`
}
