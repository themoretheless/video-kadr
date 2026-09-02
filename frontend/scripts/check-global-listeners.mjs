import { readdir, readFile } from 'node:fs/promises'
import { join, relative } from 'node:path'

const root = new URL('../src/', import.meta.url)
const allowed = new Set([
  'composables/globalListeners.ts',
  'lib/adapters/player/mediaElement.ts',
  'lib/audio/voiceRecorder.ts',
  'lib/capture/mediaRecorder.ts',
  'lib/motion/browserFrameSampler.ts',
])

async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true })
  const nested = await Promise.all(entries.map((entry) => {
    const path = join(directory, entry.name)
    return entry.isDirectory() ? files(path) : [path]
  }))
  return nested.flat()
}

const rootPath = new URL('.', root).pathname
const violations = []
for (const path of await files(rootPath)) {
  if (!/\.(svelte|ts)$/.test(path) || path.endsWith('.test.ts')) continue
  const name = relative(rootPath, path)
  if (allowed.has(name)) continue
  const source = await readFile(path, 'utf8')
  if (/\b(window|document)\.addEventListener\s*\(/.test(source) || /\bnew ResizeObserver\s*\(/.test(source)) {
    violations.push(name)
  }
}
if (violations.length) {
  console.error(`Global listener/resize lifecycle bypass:\n${violations.join('\n')}`)
  process.exit(1)
}
