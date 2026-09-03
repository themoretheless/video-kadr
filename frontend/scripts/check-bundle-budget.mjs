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
// Wave R's shared command boundary and opt-in semantic timeline added 0.4 KiB
// to startup and 1.7 KiB overall. Keep the per-kind and async-chunk limits,
// while rounding only the affected aggregate ceilings to the next KiB. The
// durable timeline Review panel (2026-09-03) did not change startup bytes and
// added 1.0 KiB JS plus 0.9 KiB CSS gzip to the lazy composition surface; its
// timeline marker adds another 0.2 KiB. Server-owned membership controls add
// 0.7 KiB to that lazy surface without changing its startup boundary; measured
// ceilings move by 3 KiB overall and 2 KiB for the largest lazy chunk. Expiring
// share-link creation/revocation adds another measured 0.6 KiB, rounded to one
// additional KiB in the aggregate JS and total ceilings. The public review
// route then moved App out of startup (124.8 -> 21.4 KiB total gzip): Rollup's
// new route boundary costs 8 KiB aggregate JS, while the 1.8 KiB review page
// remains isolated. App is a route shell with its own ceiling; nested optional
// feature chunks retain a separate strict limit. Session-bound Review auth
// adds 0.81 KiB JS and 0.12 KiB CSS to the lazy composition surface, rounded
// to one KiB in its chunk and aggregate total ceilings; startup is unchanged.
// Persistent Spaces management adds a measured 0.96 KiB to the same lazy
// surface and 1.06 KiB aggregate JS while startup remains exactly 21.4 KiB;
// round the affected ceilings by one KiB rather than weakening startup gates.
// Space-aware library headers and immediate selection refresh add 0.14 KiB
// aggregate JS with no startup change; round only the aggregate JS ceiling.
// Revision ETags and safe active-project polling add 0.40 KiB to lazy chunks
// with no startup regression; round only the aggregate total ceiling.
// Space team-template CRUD and reachable catalog controls add 0.61 KiB JS gzip
// to the optional composition chunk; startup remains unchanged. Round the
// affected per-chunk and aggregate ceilings by one KiB.
// The Space brand-kit editor plus direct text-style presets add a measured
// 1.52 KiB JS and 0.11 KiB CSS gzip to the optional composition surface.
// Round its per-chunk, aggregate JS, and aggregate total ceilings by 2 KiB;
// the startup ceilings remain unchanged.
// Authenticated Pexels search, attribution cards, and bounded photo/video
// import add 1.55 KiB JS and 0.12 KiB CSS gzip. Startup remains unchanged;
// move only the optional/aggregate ceilings by the next whole KiB.
// Streaming enforcement of the 64 MiB photo cap adds 0.11 KiB and pushes the
// aggregate across the rounded boundary; keep one further KiB of total headroom.
// Owner-only Space membership revocation adds 0.13 KiB JS to the lazy UI;
// round the aggregate JS ceiling by one KiB without changing startup limits.
// Safe empty-Space deletion and confirmation add 0.17 KiB aggregate JS and
// move the optional composition chunk past 64 KiB; round only that lazy limit.
// Canonical multi-project `.veproj` export adds another measured 0.20 KiB JS;
// round the aggregate total by one KiB while preserving startup ceilings.
// Name-confirmed Space teardown adds 0.26 KiB to the optional UI; round the
// aggregate JS ceiling by one KiB without moving startup or total ceilings.
// The authenticated YouTube connection and direct-publish panel adds 1.53 KiB
// JS gzip to the optional composition surface and 0.12 KiB CSS while startup
// remains exactly unchanged. Round only the affected lazy, aggregate JS, and
// total ceilings.
// Rotated Linear-mask polygon preview adds 0.19 KiB aggregate JS without
// changing startup or crossing the existing lazy-chunk/total ceilings. Round
// only the aggregate JS ceiling by one KiB.
// Local bounded waveform silence removal plus its timeline controls add
// 1.36 KiB aggregate JS and no startup/per-chunk regression. Round only the
// aggregate JS and total ceilings by 2 KiB.
// Extending the same detector to selected Multitrack clips adds 1.20 KiB
// aggregate JS and 0.54 KiB to the lazy composition chunk; startup is still
// unchanged. Round the affected aggregate and composition ceilings by 1 KiB.
// Handle-backed audio crossfade authoring and two-source browser preview add
// 1.05 KiB aggregate JS and 0.48 KiB to the optional composition chunk, with
// no startup change. Round only the affected lazy/aggregate ceilings.
// Atomic detach-audio authoring adds 0.49 KiB aggregate JS to the lazy editor
// state/UI path and leaves startup at 13.32 KiB. Round the route, aggregate JS,
// and aggregate total ceilings by one KiB; keep CSS/composition limits fixed.
// Vertical, circular, and diagonal preview geometry expands the transition
// catalog from six to sixteen and adds 0.18 KiB only to the lazy composition
// surface. Startup and aggregate totals remain below their existing ceilings;
// round only the affected async-chunk guardrail by one KiB.
// Hashed consume-once Space invites add 1.03 KiB aggregate JS across the API
// client and lazy collaboration UI; startup remains 13.33 KiB and the largest
// optional chunk remains inside 70 KiB. Round aggregate JS/total by one KiB.
// Undoable canvas size/FPS/color authoring and five social presets add 0.82 KiB
// to the optional composition surface without changing startup. Round only the
// lazy composition, aggregate JS, and aggregate total ceilings by one KiB.
// Source-derived blur and checker-pattern preview/authoring add another 0.83 KiB
// aggregate JS (0.67 KiB in the lazy composition chunk); startup stays fixed.
// Round the same optional and aggregate ceilings by one further KiB.
// Five typed clip-effect presets, ordered-stack authoring, and honest preview
// status add 0.91 KiB aggregate JS (0.69 KiB to the composition chunk). Round
// the affected lazy/aggregate ceilings by one KiB; startup changes only 0.05 KiB.
// Eight editable layer-animation presets add 1.06 KiB to the lazy composition
// chunk and 1.06 KiB aggregate JS. The builder stays behind that route boundary;
// App remains under its existing ceiling. Round lazy/aggregate limits by 1 KiB.
// Four smooth directional transition enum values add 0.01 KiB to the App route
// shell through its shared wire validator, without changing startup. Round only
// the route-shell ceiling by one KiB; keep startup and aggregate gates fixed.
// RGB Split and deterministic Posterize add 0.05 KiB aggregate JS to the lazy
// effect catalog and shared validator. Startup is unchanged; round only the
// crossed aggregate-total ceiling by one KiB.
const budgets = {
  initialJs: envBytes('BUNDLE_BUDGET_INITIAL_JS_GZIP', 'BUNDLE_BUDGET_JS_GZIP', 115 * 1024),
  initialCss: envBytes('BUNDLE_BUDGET_INITIAL_CSS_GZIP', 'BUNDLE_BUDGET_CSS_GZIP', 12 * 1024),
  initialTotal: envBytes('BUNDLE_BUDGET_INITIAL_TOTAL_GZIP', undefined, 125 * 1024),
  // Includes the delivery catalog and lazy In/Out range authoring workflow.
  allJs: envBytes('BUNDLE_BUDGET_ALL_JS_GZIP', undefined, 207.75 * 1024),
  allCss: envBytes('BUNDLE_BUDGET_ALL_CSS_GZIP', undefined, 17 * 1024),
  allTotal: envBytes('BUNDLE_BUDGET_ALL_TOTAL_GZIP', 'BUNDLE_BUDGET_TOTAL_GZIP', 224 * 1024),
  routeJsChunk: envBytes('BUNDLE_BUDGET_ROUTE_JS_CHUNK_GZIP', undefined, 84 * 1024),
  asyncJsChunk: envBytes('BUNDLE_BUDGET_ASYNC_JS_CHUNK_GZIP', undefined, 74.25 * 1024),
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
const routeShellFiles = new Set(
  Object.values(manifest)
    .filter((record) => record.isDynamicEntry && record.name === 'App')
    .map((record) => record.file),
)

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
  if (file.endsWith('.js')) {
    const isRouteShell = routeShellFiles.has(file)
    failures.push([
      `${isRouteShell ? 'route shell' : 'async chunk'} ${file}`,
      compressed.get(file) ?? 0,
      isRouteShell ? budgets.routeJsChunk : budgets.asyncJsChunk,
    ])
  }
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
