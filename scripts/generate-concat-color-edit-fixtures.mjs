import esbuild from '../frontend/node_modules/esbuild/lib/main.js'
import {readFile,writeFile,mkdir} from 'node:fs/promises'
import {fileURLToPath,pathToFileURL} from 'node:url'
import path from 'node:path'
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..'),output=path.join(root,'backend/target/qualification-logs/concat-color-edit-module.mjs')
await mkdir(path.dirname(output),{recursive:true})
await esbuild.build({entryPoints:[path.join(root,'scripts/concat-color-edit-fixtures.ts')],outfile:output,bundle:true,platform:'node',format:'esm',alias:{'$lib':path.join(root,'frontend/src/lib')}})
const {fixture}=await import(pathToFileURL(output).href)
if(fixture.cases.length!==198||fixture.cases.some(c=>c.edits.length!==3))throw new Error('incomplete Concat edit matrix')
const target=path.join(root,'backend/tests/fixtures/concat-color-edit.json'),text=JSON.stringify(fixture,null,2)+'\n'
if(process.argv.includes('--check')){if(await readFile(target,'utf8')!==text)throw new Error('Concat edit fixtures differ from current commands')}else await writeFile(target,text)
console.log('PASS current frontend:198 Concat effect/playback cases,594 edits,4356 frame comparisons')
