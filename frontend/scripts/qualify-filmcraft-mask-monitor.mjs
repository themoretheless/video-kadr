import assert from 'node:assert/strict'
import {chromium} from 'playwright'
import {spawn} from 'node:child_process'
import {createServer} from 'node:net'
import {randomUUID,createHash} from 'node:crypto'
import {readFile,writeFile,mkdir,unlink} from 'node:fs/promises'
import {fileURLToPath} from 'node:url'
const root=fileURLToPath(new URL('../',import.meta.url)),repo=new URL('../../',import.meta.url)
const id=randomUUID(),htmlName=`.mask-monitor-${id}.html`,componentName=`.mask-monitor-${id}.svelte`
const html=new URL(`../${htmlName}`,import.meta.url),component=new URL(`../${componentName}`,import.meta.url)
const output=new URL(`backend/target/filmcraft-mask-monitor-browser-${id}/`,repo)
const newMask=process.env.FILMCRAFT_NEW_MASK==='1'
const fixturePath=new URL(newMask?'backend/target/filmcraft-new-mask-browser.json':'backend/target/filmcraft-mask-monitor-browser.json',repo)
const fixture=JSON.parse(await readFile(fixturePath,'utf8'))
assert.equal(fixture.penCases.length,6,'fresh native pen fixture required')
if(!newMask){assert.equal(fixture.falloffCases?.length,2,'fresh native falloff fixture required');assert.equal(fixture.bodyCases?.length,2,'fresh native body hit fixture required');assert.equal(fixture.clickCases?.length,3,'fresh native body click fixture required')}
const inputs=['frontend/src/lib/components/composition/FilmcraftMaskMonitor.svelte','frontend/src/lib/components/composition/FilmcraftMonitorFrame.svelte','frontend/src/lib/api.ts','frontend/src/lib/filmcraft/monitor-queue.ts','backend/src/domain/filmcraft_mask_monitor.rs','frontend/scripts/qualify-filmcraft-mask-monitor.mjs']
const hashes=async()=>Object.fromEntries(await Promise.all(inputs.map(async path=>[path,createHash('sha256').update(await readFile(new URL(path,repo))).digest('hex')])) )
const before=await hashes();await mkdir(output,{recursive:true})
await writeFile(component,`<script>
import Monitor from '/src/lib/components/composition/FilmcraftMaskMonitor.svelte';
let {fixture}=$props();let doc=$state(fixture.document);let undo=$state([]),redo=$state([]);let creating=$state(${newMask});
window.accepted=[];window.selectionCleared=0;window.currentDocument=()=>doc;
function accept(project,index){creating=false;undo.push(doc);redo=[];doc=project.documentText;window.accepted.push(doc);}
</script><button onclick={()=>{if(undo.length){redo.push(doc);doc=undo.pop();creating=${newMask}&&doc===fixture.document;}}}>Undo</button><button onclick={()=>{if(redo.length){undo.push(doc);doc=redo.pop();creating=${newMask}&&doc===fixture.document;}}}>Redo</button><Monitor document={doc} createMask={creating} address={creating?fixture.address:{...fixture.address,...(${newMask}?{maskIndex:0}:{})}} sources={{}} token={null} timeFrames="0" timeTicks="0" disabled={false} onbusy={()=>{}} ondeselect={()=>{window.selectionCleared++;}} onedit={accept}/>`)
await writeFile(html,`<!doctype html><meta charset="utf-8"><title>Filmcraft mask monitor qualification</title><main id="app"></main><script type="module">import '/src/app.css';import {mount} from 'svelte';import Harness from '/${componentName}';const fixture=await fetch('/mask-fixture.json').then(r=>r.json());mount(Harness,{target:document.querySelector('#app'),props:{fixture}});</script>`)
const reservation=createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));const port=reservation.address().port;await new Promise(resolve=>reservation.close(resolve))
let server,browser,log='';const requests=[];const frameRequests=[];const errors=[]
try{
 server=spawn(process.execPath,[`${root}/node_modules/vite/bin/vite.js`,'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,stdio:['ignore','pipe','pipe'],detached:true});for(const stream of [server.stdout,server.stderr])stream.on('data',chunk=>{log=(log+chunk).slice(-1000000)})
 const url=`http://127.0.0.1:${port}`;let ready=false
 for(let i=0;i<100;i++){if(server.exitCode!==null)throw new Error(log);try{const response=await fetch(`${url}/${htmlName}`);if(response.ok&&(await response.text()).includes('Filmcraft mask monitor qualification')){ready=true;break}}catch{}await new Promise(resolve=>setTimeout(resolve,100))}assert(ready,log)
 browser=await chromium.launch({headless:true});const page=await browser.newPage({viewport:{width:1000,height:760}});page.on('pageerror',error=>errors.push(String(error)))
 await page.route('**/mask-fixture.json',route=>route.fulfill({json:fixture}))
 const snapshots=newMask?[{doc:fixture.document,overlay:fixture.read.overlay,frame:fixture.frame},{doc:fixture.penResult.project.documentText,overlay:fixture.penResult.overlay,frame:fixture.penFrame}]:[{doc:fixture.document,overlay:fixture.read.overlay,frame:fixture.frame},{doc:fixture.edited.project.documentText,overlay:fixture.edited.overlay,frame:fixture.editedFrame},{doc:fixture.penResult.project.documentText,overlay:fixture.penResult.overlay,frame:fixture.penFrame},{doc:fixture.insertResult.project.documentText,overlay:fixture.insertResult.overlay,frame:fixture.insertFrame}]
 if(!newMask)for(const c of [...(fixture.falloffCases??[]),...(fixture.bodyCases??[])])if(c.result.project)snapshots.push({doc:c.result.project.documentText,overlay:c.result.overlay,frame:c.frame})
 const snapshot=doc=>{const found=snapshots.find(s=>s.doc===doc);assert(found,'unknown native document');return found}
 const penKey=op=>JSON.stringify([op.closed,op.points.map(p=>[p.point,p.handle??null])])
 await page.route('**/api/compositions/filmcraft/mask-monitor',async route=>{const input=route.request().postDataJSON();requests.push(input);let response;if(input.operation.kind==='read')response={overlay:snapshot(input.document).overlay,project:null};else if(input.operation.kind==='bodyClick'){const match=fixture.clickCases.find(c=>['point','pixelScale'].every(key=>c.operation[key].every((v,i)=>Math.abs(v-input.operation[key][i])<1e-8)));assert(match,JSON.stringify(input.operation));response=match.result}else if(input.operation.kind==='translate'){const match=fixture.bodyCases.find(c=>['origin','delta'].every(key=>c.operation[key].every((v,i)=>Math.abs(v-input.operation[key][i])<1e-8)));assert(match,JSON.stringify(input.operation));response=match.result}else if(input.operation.kind==='falloff'){const match=fixture.falloffCases.find(c=>c.operation.parameter===input.operation.parameter&&c.operation.delta.every((v,i)=>Math.abs(v-input.operation.delta[i])<1e-8));assert(match,JSON.stringify(input.operation));response=match.result}else if(input.operation.kind==='move')response=fixture.edited;else if(input.operation.kind==='penDraft'){const match=fixture.penCases.find(c=>penKey(c.operation)===penKey(input.operation));assert(match,`unknown pen operation ${JSON.stringify(input.operation)}`);response=match.result}else{assert.equal(input.operation.kind,'insertNear');assert.deepEqual(input.operation,fixture.insertOperation);response=fixture.insertResult}await route.fulfill({json:response})})
 await page.route('**/api/compositions/filmcraft/preview-frame',route=>{const input=route.request().postDataJSON();frameRequests.push(input.document);return route.fulfill({json:snapshot(input.document).frame})})
 await page.goto(`${url}/${htmlName}`);await page.getByText(newMask?'Создать маску в мониторе Filmcraft':'Маска в мониторе Filmcraft',{exact:true}).click()
 const svg=page.getByLabel('Контур маски Filmcraft',{exact:true});await svg.waitFor();await page.locator('canvas').waitFor();await page.screenshot({path:fileURLToPath(new URL('before.png',output))})
 let traceCases,history
 if(newMask){
 const draw=async(x,y,hx=x,hy=y)=>{const box=await svg.boundingBox();assert(box);const screen=(x,y)=>[box.x+x/320*box.width,box.y+y/180*box.height];await page.mouse.move(...screen(x,y));await page.mouse.down();if(hx!==x||hy!==y)await page.mouse.move(...screen(hx,hy));await page.mouse.up()}
 assert.equal(await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).inputValue(),'pen')
 await draw(40,30);await svg.focus();await page.keyboard.press('Escape');assert.equal(await page.evaluate(()=>window.accepted.length),0)
 const count=frameRequests.length
 await draw(40,30,70,30);await page.waitForFunction(()=>document.querySelector('[aria-label="Маска 1 out Filmcraft"]')?.getAttribute('cx')==='70')
 await draw(280,30);await page.waitForFunction(()=>document.querySelectorAll('circle[aria-label$="vertex Filmcraft"]').length===2)
 await draw(160,150,130,150);await page.waitForFunction(()=>document.querySelector('[aria-label="Маска 3 out Filmcraft"]')?.getAttribute('cx')==='130')
 assert.equal(await page.evaluate(()=>window.accepted.length),0);assert.equal(frameRequests.length,count)
 await page.screenshot({path:fileURLToPath(new URL('new-mask-draft.png',output))})
 await svg.focus();await page.keyboard.press('Enter');await page.waitForFunction(()=>window.accepted.length===1)
 assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.penResult.project.documentText)
 await page.getByText('Маска в мониторе Filmcraft',{exact:true}).waitFor();await svg.waitFor()
 await page.waitForFunction(()=>document.querySelector('[aria-label="Инструмент маски в мониторе Filmcraft"]').value==='move')
 await page.screenshot({path:fileURLToPath(new URL('new-mask-complete.png',output))})
 const pen=requests.filter(r=>r.operation.kind==='penDraft'&&r.operation.closed).at(-1);assert(pen.createMask);assert.equal(pen.address.maskIndex,undefined)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.document)
 await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.penResult.project.documentText)
 traceCases=[{request:pen,expectedProject:fixture.penResult.project,expectedOverlay:fixture.penResult.overlay}]
 history={accepted:1,undo:true,redo:true,penCancel:true,newMask:true}
 }else{
 const handle=page.getByLabel('Маска 1 vertex Filmcraft',{exact:true}),rect=await svg.boundingBox(),center=await handle.boundingBox();assert(rect&&center)
 await page.mouse.move(center.x+center.width/2,center.y+center.height/2);await page.mouse.down();await page.mouse.move(rect.x+80/320*rect.width,rect.y+60/180*rect.height)
 await page.waitForFunction(()=>window.accepted.length===0);for(let i=0;i<50&&!requests.some(r=>r.operation.kind==='move');i++)await new Promise(resolve=>setTimeout(resolve,20));assert(requests.some(r=>r.operation.kind==='move'))
 await page.mouse.up();await page.waitForFunction(()=>window.accepted.length===1);assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.edited.project.documentText)
 const move=requests.filter(r=>r.operation.kind==='move').at(-1);assert.equal(move.operation.index,0);assert.equal(move.operation.part,'vertex');assert(Math.abs(move.operation.point[0]-80)<1e-8&&Math.abs(move.operation.point[1]-60)<1e-8);assert.equal(move.timeTicks,'0')
 await page.getByLabel('Маска 1 vertex Filmcraft',{exact:true}).waitFor();await page.screenshot({path:fileURLToPath(new URL('edited.png',output))})
 await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.document)
 await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.edited.project.documentText)
 await svg.waitFor();await page.locator('canvas').waitFor()
 const frameCount=frameRequests.length
 await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).selectOption('pen')
 const draw=async(x,y,hx=x,hy=y)=>{const box=await svg.boundingBox();assert(box);const screen=(x,y)=>[box.x+x/320*box.width,box.y+y/180*box.height];await page.mouse.move(...screen(x,y));await page.mouse.down();if(hx!==x||hy!==y)await page.mouse.move(...screen(hx,hy));await page.mouse.up()}
 await draw(40,30,70,30);await page.waitForFunction(()=>document.querySelector('[aria-label="Маска 1 out Filmcraft"]')?.getAttribute('cx')==='70')
 await draw(280,30);await page.waitForFunction(()=>document.querySelectorAll('circle[aria-label$="vertex Filmcraft"]').length===2)
 await draw(160,150,130,150);await page.waitForFunction(()=>document.querySelector('[aria-label="Маска 3 out Filmcraft"]')?.getAttribute('cx')==='130')
 assert.equal(await page.evaluate(()=>window.accepted.length),1,'pen drafts must not publish');assert.equal(frameRequests.length,frameCount,'open pen draft must leave the accepted picture unchanged')
 await svg.focus();await page.keyboard.press('Enter');await page.waitForFunction(()=>window.accepted.length===2);assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.penResult.project.documentText)
 await svg.waitFor();await page.locator('canvas').waitFor();await page.screenshot({path:fileURLToPath(new URL('pen.png',output))})
 const pen=requests.filter(r=>r.operation.kind==='penDraft'&&r.operation.closed).at(-1);assert(pen);assert.deepEqual(pen.operation,fixture.penCases.at(-1).operation)
 const insertBox=await svg.boundingBox();assert(insertBox);await page.keyboard.down('Control');await page.mouse.click(insertBox.x+160/320*insertBox.width,insertBox.y+30/180*insertBox.height);await page.keyboard.up('Control');await page.waitForFunction(()=>window.accepted.length===3);assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.insertResult.project.documentText)
 await svg.waitFor();await page.locator('canvas').waitFor();await page.screenshot({path:fileURLToPath(new URL('inserted.png',output))})
 const insert=requests.filter(r=>r.operation.kind==='insertNear').at(-1);assert(insert)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.penResult.project.documentText)
 await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.edited.project.documentText)
 await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.penResult.project.documentText)
 await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.insertResult.project.documentText)
 await svg.waitFor();await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).selectOption('pen');await draw(40,30);await svg.focus();await page.keyboard.press('Escape');assert.equal(await page.evaluate(()=>window.accepted.length),3)
 const falloffTraces=[]
 for(const c of fixture.falloffCases??[]){
   while(await page.evaluate(()=>window.currentDocument())!==fixture.document)await page.getByRole('button',{name:'Undo',exact:true}).click()
   await svg.waitFor();await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).selectOption('move')
   const control=page.getByLabel(c.operation.parameter==='feather'?'Растушёвка маски Filmcraft':'Расширение маски Filmcraft',{exact:true});await control.waitFor()
   const b=await svg.boundingBox(),h=await control.boundingBox();assert(b&&h)
   const sx=b.width/320,sy=b.height/180,x=b.x+Math.round((h.x+h.width/2-b.x)/sx)*sx,y=b.y+Math.round((h.y+h.height/2-b.y)/sy)*sy
   const accepted=await page.evaluate(()=>window.accepted.length)
   await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+c.operation.delta[0]*sx,y+c.operation.delta[1]*sy)
   for(let i=0;i<100&&!requests.some(r=>r.operation.kind==='falloff'&&r.operation.parameter===c.operation.parameter);i++)await new Promise(resolve=>setTimeout(resolve,20))
   assert.equal(await page.evaluate(()=>window.accepted.length),accepted)
   await page.mouse.up();await page.waitForFunction(count=>window.accepted.length===count,accepted+1)
   assert.equal(await page.evaluate(()=>window.currentDocument()),c.result.project.documentText)
   await page.screenshot({path:fileURLToPath(new URL(`${c.operation.parameter}.png`,output))})
   const request=requests.filter(r=>r.operation.kind==='falloff'&&r.operation.parameter===c.operation.parameter).at(-1)
   assert(request);falloffTraces.push({request,expectedProject:c.result.project,expectedOverlay:c.result.overlay})
   await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.document)
   await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,c.result.project.documentText)
 }
 const bodyTraces=[]
 for(const c of fixture.bodyCases??[]){
   while(await page.evaluate(()=>window.currentDocument())!==fixture.document)await page.getByRole('button',{name:'Undo',exact:true}).click()
   await svg.waitFor();await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).selectOption('move')
   const b=await svg.boundingBox();assert(b);const screen=p=>[b.x+p[0]/320*b.width,b.y+p[1]/180*b.height]
   const accepted=await page.evaluate(()=>window.accepted.length),count=requests.length
   await page.mouse.move(...screen(c.operation.origin));await page.mouse.down();await page.mouse.move(...screen(c.operation.origin.map((v,i)=>v+c.operation.delta[i])))
   for(let i=0;i<100&&!requests.slice(count).some(r=>r.operation.kind==='translate');i++)await new Promise(resolve=>setTimeout(resolve,20))
   assert(requests.slice(count).some(r=>r.operation.kind==='translate'));assert.equal(await page.evaluate(()=>window.accepted.length),accepted)
   await page.mouse.up();await page.waitForFunction(()=>!document.querySelector('[aria-label="Инструмент маски в мониторе Filmcraft"]').disabled)
   assert.equal(await page.evaluate(()=>window.accepted.length),accepted+Number(c.hit))
   assert.equal(await page.evaluate(()=>window.currentDocument()),c.hit?c.result.project.documentText:fixture.document)
   const request=requests.filter(r=>r.operation.kind==='translate').at(-1);assert(request)
   bodyTraces.push({request,expectedProject:c.result.project,expectedOverlay:c.result.overlay})
   await page.screenshot({path:fileURLToPath(new URL(c.hit?'body-inside.png':'body-outside.png',output))})
   if(c.hit){
     await page.getByRole('button',{name:'Undo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,fixture.document)
     await page.getByRole('button',{name:'Redo',exact:true}).click();await page.waitForFunction(document=>window.currentDocument()===document,c.result.project.documentText)
   }
 }
 const clickTraces=[]
 while(await page.evaluate(()=>window.currentDocument())!==fixture.document)await page.getByRole('button',{name:'Undo',exact:true}).click()
 for(const c of fixture.clickCases){
   await svg.waitFor();await page.getByLabel('Инструмент маски в мониторе Filmcraft',{exact:true}).selectOption('move')
   const b=await svg.boundingBox();assert(b);const accepted=await page.evaluate(()=>window.accepted.length),count=requests.length
   await page.mouse.click(b.x+c.operation.point[0]/320*b.width,b.y+c.operation.point[1]/180*b.height)
   for(let i=0;i<100&&!requests.slice(count).some(r=>r.operation.kind==='bodyClick');i++)await new Promise(resolve=>setTimeout(resolve,20))
   assert(requests.slice(count).some(r=>r.operation.kind==='bodyClick'))
   if(c.clear)await page.waitForFunction(()=>window.selectionCleared===1)
   else await page.waitForFunction(()=>!document.querySelector('[aria-label="Инструмент маски в мониторе Filmcraft"]').disabled)
   assert.equal(await page.evaluate(()=>window.accepted.length),accepted)
   assert.equal(await page.evaluate(()=>window.currentDocument()),fixture.document)
   assert.equal(await page.evaluate(()=>window.selectionCleared),Number(c.clear))
   assert.equal(await page.locator('details').evaluate(node=>node.open),!c.clear)
   clickTraces.push({request:requests.filter(r=>r.operation.kind==='bodyClick').at(-1),expectedProject:null,expectedOverlay:c.result.overlay})
 }
 await page.screenshot({path:fileURLToPath(new URL('selection-cleared.png',output))})
 traceCases=[{request:move,expectedProject:fixture.edited.project,expectedOverlay:fixture.edited.overlay},{request:pen,expectedProject:fixture.penResult.project,expectedOverlay:fixture.penResult.overlay},{request:insert,expectedProject:fixture.insertResult.project,expectedOverlay:fixture.insertResult.overlay},...falloffTraces,...bodyTraces,...clickTraces]
 history={accepted:3+(fixture.falloffCases?.length??0)+(fixture.bodyCases??[]).filter(c=>c.hit).length,undo:true,redo:true,penCommits:1,insertionCommits:1,penCancel:true,falloffDrags:fixture.falloffCases?.length??0,bodyInside:true,bodyOutsideNoop:true,selectionClear:true,selectionClearUndoEntries:0}
 }
 assert.deepEqual(errors,[]);const after=await hashes();assert.deepEqual(after,before,'source changed during browser qualification')
 await writeFile(new URL('trace.json',output),JSON.stringify({cases:traceCases},null,2))
 await writeFile(new URL('evidence.json',output),JSON.stringify({scope:'Real Chromium pointer capture + native prepared monitor and raster fixtures',api:'HTTP responses served from the Rust integration test fixture; not a live backend in this browser run',sourceHashes:after,fixtureSha256:createHash('sha256').update(await readFile(fixturePath)).digest('hex'),requests:requests.map(r=>({operation:r.operation,timeTicks:r.timeTicks})),history,errors},null,2))
 process.stdout.write(fileURLToPath(output)+'\n')
}finally{await writeFile(new URL('vite.log',output),log);await browser?.close();if(server?.pid&&server.exitCode===null){try{process.kill(-server.pid,'SIGTERM')}catch{}}await Promise.all([unlink(html).catch(()=>{}),unlink(component).catch(()=>{})])}
