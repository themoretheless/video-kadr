import { readFile, readdir } from 'node:fs/promises'
import { extname, join } from 'node:path'

const featureRoot = new URL('../src/lib/features/', import.meta.url)
const tokenCss = await readFile(new URL('../src/lib/ui/tokens.css', import.meta.url), 'utf8')
if (!/--design-token-version:\s*\d+;/.test(tokenCss)) {
  throw new Error('Semantic design token contract must declare --design-token-version')
}

const rawPalette = /#[0-9a-f]{3,8}\b|\brgba?\s*\(|\bhsla?\s*\(/i
const violations = []
for (const file of await walk(featureRoot.pathname)) {
  if (!['.css', '.svelte', '.ts'].includes(extname(file)) || file.endsWith('.test.ts')) continue
  const lines = (await readFile(file, 'utf8')).split('\n')
  lines.forEach((line, index) => {
    if (rawPalette.test(line)) violations.push(`${file}:${index + 1}`)
  })
}
if (violations.length) {
  throw new Error(`Raw palette values are forbidden in feature code:\n${violations.join('\n')}`)
}
console.log('Design tokens: versioned contract present; feature palette is semantic')

async function walk(directory) {
  const result = []
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) result.push(...await walk(path))
    else result.push(path)
  }
  return result
}
