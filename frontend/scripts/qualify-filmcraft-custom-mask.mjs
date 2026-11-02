import assert from 'node:assert/strict'
import {chromium} from 'playwright'
import {spawn} from 'node:child_process'
import {createServer} from 'node:net'
import {randomUUID,createHash} from 'node:crypto'
import {readFile,writeFile,mkdir,unlink} from 'node:fs/promises'
import {fileURLToPath} from 'node:url'

const root=fileURLToPath(new URL('../',import.meta.url)),repo=new URL('../../',import.meta.url)
const id=randomUUID(),htmlName=`.custom-mask-${id}.html`,componentName=`.custom-mask-${id}.svelte`
const html=new URL(`../${htmlName}`,import.meta.url),component=new URL(`../${componentName}`,import.meta.url)
const output=new URL(`backend/target/filmcraft-custom-mask-browser-${id}/`,repo)
const fixturePath=new URL('backend/target/filmcraft-custom-mask-browser.json',repo)
const fixture=JSON.parse(await readFile(fixturePath,'utf8'))
assert.equal(fixture.path.vertices.length,3)
assert.notEqual(fixture.frames[0].rgbaHex,fixture.frames[1].rgbaHex)
assert.equal(fixture.frames[1].rgbaHex,fixture.frames[2].rgbaHex)
const inputs=['frontend/src/lib/components/composition/FilmcraftScalarParameterEditor.svelte','frontend/src/lib/components/composition/FilmcraftMaskControls.svelte','frontend/src/lib/components/composition/FilmcraftPathValueEditor.svelte','frontend/src/lib/api.ts','backend/src/domain/filmcraft_parameter_edit.rs','frontend/scripts/qualify-filmcraft-custom-mask.mjs']
const hashes=async()=>Object.fromEntries(await Promise.all(inputs.map(async path=>[path,createHash('sha256').update(await readFile(new URL(path,repo))).digest('hex')])) )
const before=await hashes();await mkdir(output,{recursive:true})
await writeFile(component,`<script>
import Editor from '/src/lib/components/composition/FilmcraftScalarParameterEditor.svelte';
let {fixture}=$props();let doc=$state(fixture.document),undo=$state([]),redo=$state([]);
window.accepted=[];window.currentDocument=()=>doc;
function accept(project){undo.push(doc);redo=[];doc=project.documentText;window.accepted.push(doc);}
</script><button onclick={()=>{if(undo.length){redo.push(doc);doc=undo.pop();}}}>Undo</button><button onclick={()=>{if(redo.length){undo.push(doc);doc=redo.pop();}}}>Redo</button><Editor document={doc} sequenceId="6" clipId="1010" timeTicks="0" timeFrames="0" sources={{}} token={null} onedit={accept}/>`)
await writeFile(html,`<!doctype html><meta charset="utf-8"><title>Filmcraft custom mask qualification</title><main id="app"></main><script type="module">import '/src/app.css';import {mount} from 'svelte';import Harness from '/${componentName}';const fixture=await fetch('/custom-mask-fixture.json').then(r=>r.json());mount(Harness,{target:document.querySelector('#app'),props:{fixture}});</script>`)
const reservation=createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));const port=reservation.address().port;await new Promise(resolve=>reservation.close(resolve))
let server,browser,log='';const requests=[],errors=[]
try{
  server=spawn(process.execPath,[`${root}/node_modules/vite/bin/vite.js`,'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,stdio:['ignore','pipe','pipe'],detached:true})
  for(const stream of [server.stdout,server.stderr])stream.on('data',chunk=>{log=(log+chunk).slice(-1000000)})
  const url=`http://127.0.0.1:${port}`;let ready=false
  for(let i=0;i<100;i++){if(server.exitCode!==null)throw new Error(log);try{const r=await fetch(`${url}/${htmlName}`);if(r.ok&&(await r.text()).includes('Filmcraft custom mask qualification')){ready=true;break}}catch{}await new Promise(resolve=>setTimeout(resolve,100))}assert(ready,log)
  browser=await chromium.launch({headless:true});const page=await browser.newPage({viewport:{width:1100,height:1000}});page.on('pageerror',error=>errors.push(String(error)))
  await page.route('**/custom-mask-fixture.json',route=>route.fulfill({json:fixture}))
  await page.route('**/api/compositions/filmcraft/*',async route=>{
    const input=route.request().postDataJSON(),path=new URL(route.request().url()).pathname
    if(path.endsWith('/parameters')){const index=input.document===fixture.document?0:1;assert.equal(input.document,index?fixture.created.documentText:fixture.document);await route.fulfill({json:fixture.reports[index]});return}
    assert(path.endsWith('/edit-mask'),path);assert.equal(input.document,fixture.document)
    assert.deepEqual(input.command,{kind:'create',name:'Bezier',shape:{kind:'path',path:fixture.path}})
    requests.push(input);await route.fulfill({json:fixture.created})
  })
  await page.goto(`${url}/${htmlName}`);await page.getByText('Ключи параметров Filmcraft',{exact:true}).click();await page.getByText('Инструменты масок Filmcraft',{exact:true}).click()
  await page.getByLabel('Форма новой маски Filmcraft',{exact:true}).selectOption('path');await page.getByLabel('Имя новой маски Filmcraft',{exact:true}).fill('Bezier')
  for(const [index,vertex] of fixture.path.vertices.entries()){
    for(const axis of ['x','y'])await page.getByLabel(`Вершина ${index+1} ${axis.toUpperCase()} Filmcraft`,{exact:true}).fill(String(vertex.p[axis]))
    for(const side of ['t_in','t_out'])if(vertex[side]){await page.getByRole('button',{name:`Добавить ${side} вершины ${index+1}`,exact:true}).click();for(const axis of ['x','y'])await page.getByLabel(`Вершина ${index+1} ${side} ${axis.toUpperCase()} Filmcraft`,{exact:true}).fill(String(vertex[side][axis]))}
  }
  assert.equal(requests.length,0);assert.equal(await page.evaluate(()=>window.accepted.length),0)
  await page.screenshot({path:fileURLToPath(new URL('draft.png',output)),fullPage:true})
  await page.getByRole('button',{name:'Создать маску Filmcraft',exact:true}).click();await page.waitForFunction(()=>window.accepted.length===1)
  assert.equal(requests.length,1);assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.created.documentText)
  await page.waitForFunction(()=>document.querySelector('[aria-label="Маска параметров Filmcraft"]')?.value==='0')
  await page.screenshot({path:fileURLToPath(new URL('created.png',output)),fullPage:true})
  await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(doc=>window.currentDocument()===doc,fixture.document)
  await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(doc=>window.currentDocument()===doc,fixture.created.documentText)
  assert.equal(requests.length,1);assert.deepEqual(errors,[]);const after=await hashes();assert.deepEqual(after,before,'source changed during qualification')
  await writeFile(new URL('trace.json',output),JSON.stringify({request:requests[0],expectedProject:fixture.created},null,2))
  await writeFile(new URL('evidence.json',output),JSON.stringify({scope:'Real Chromium custom asymmetric Bezier mask creation in the source parameter editor',api:'Native-prepared HTTP response fixtures, not a live backend',sourceHashes:after,fixtureSha256:createHash('sha256').update(await readFile(fixturePath)).digest('hex'),history:{accepted:1,draftCommits:0,undo:true,redo:true},errors},null,2))
  process.stdout.write(fileURLToPath(output)+'\n')
}finally{await writeFile(new URL('vite.log',output),log);await browser?.close();if(server?.pid&&server.exitCode===null){try{process.kill(-server.pid,'SIGTERM')}catch{}}await Promise.all([unlink(html).catch(()=>{}),unlink(component).catch(()=>{})])}
