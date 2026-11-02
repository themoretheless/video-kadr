import esbuild from '../frontend/node_modules/esbuild/lib/main.js'
import { readFile, writeFile, mkdir } from 'node:fs/promises'
import { fileURLToPath, pathToFileURL } from 'node:url'
import path from 'node:path'
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const output = path.join(root, 'backend/target/qualification-logs/concat-merge-fixture-module.mjs')
await mkdir(path.dirname(output), { recursive: true })
await esbuild.build({ entryPoints: [path.join(root, 'scripts/concat-merge-preview-fixtures.ts')], outfile: output, bundle: true, platform: 'node', format: 'esm', alias: { '$lib': path.join(root, 'frontend/src/lib') } })
const { fixture } = await import(pathToFileURL(output).href)
if (fixture.cases.length !== 4 || fixture.holds.length !== 2 || fixture.versions.times.length !== 6 || fixture.createdTimeline.timelineOrder.length !== 2 || fixture.removedTimelines.invalidatedIds.length !== 3 || fixture.videoSettings.resized.compositions.root.content.canvas.width !== 32) throw new Error('Incomplete transition matrix')
const text = JSON.stringify(fixture, null, 2) + '\n'
const target = path.join(root, 'backend/tests/fixtures/concat-merge-preview.json')
if (process.argv.includes('--check')) {
  if (await readFile(target, 'utf8') !== text) throw new Error('Registered merge fixtures differ from current frontend output')
} else await writeFile(target, text)
console.log('PASS: four current frontend original/split/joined wire cases')
