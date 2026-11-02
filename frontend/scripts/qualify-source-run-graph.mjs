import assert from 'node:assert/strict'
import {chromium} from 'playwright'
import {spawn} from 'node:child_process'
import {createServer} from 'node:net'
import {randomUUID,createHash} from 'node:crypto'
import {readFile,writeFile,mkdir,unlink} from 'node:fs/promises'
import {fileURLToPath} from 'node:url'
import {once} from 'node:events'
const root=fileURLToPath(new URL('../',import.meta.url)),repository=fileURLToPath(new URL('../../',import.meta.url))
const id=randomUUID(),name=`.source-run-browser-${id}.html`,html=new URL(`../${name}`,import.meta.url)
const output=new URL(`../../backend/target/source-run-browser-${id}/`,import.meta.url)
const sources=['frontend/src/lib/composition/nativeEffectScalar.ts','frontend/src/lib/composition/nativeScalarTimeline.ts','frontend/src/lib/components/composition/NestedScalarTimeline.svelte','frontend/src/lib/components/composition/NestedScalarGraph.svelte','frontend/src/lib/composition/sourceRunCutTransaction.ts','frontend/src/lib/composition/sourceRunClipboard.ts','frontend/src/lib/state/sourceRunClipboard.svelte.ts','frontend/src/lib/composition/sourceRunSelection.ts','frontend/src/app.css','frontend/src/base.css','frontend/src/lib/ui/tokens.css','frontend/src/lib/composition/fixtures/nativeEffectScalarProject.ts','frontend/src/lib/composition/concatExposureSourceKeyEditing.ts','frontend/src/lib/composition/concatRadiusSourceKeys.ts','frontend/src/lib/components/composition/SourceKeyRunGraph.svelte','frontend/src/lib/components/composition/ConcatExposureSourceKeyEditor.svelte','frontend/src/lib/components/composition/NestedConcatExposureInspector.svelte','frontend/src/lib/composition/concatSourceWindow.ts','frontend/src/lib/state/nestedComposition.svelte.ts','frontend/scripts/qualify-source-run-graph.mjs']
const hashes=async()=>Object.fromEntries(await Promise.all(sources.map(async path=>[path,createHash('sha256').update(await readFile(new URL(path,`file://${repository}`))).digest('hex')])) )
const initialHashes=await hashes()
await mkdir(output,{recursive:true})
await writeFile(html,`<!doctype html><meta charset="utf-8"><title>Source key graph qualification</title><main id="app"></main><script type="module">
import '/src/app.css';
import {mount,unmount} from 'svelte';
import {sourceRunClipboard} from '/src/lib/state/sourceRunClipboard.svelte.ts';
import Inspector from '/src/lib/components/composition/NestedConcatExposureInspector.svelte';
import {effectScalarProject} from '/src/lib/composition/fixtures/nativeEffectScalarProject.ts';
import {nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition,closeNestedComposition} from '/src/lib/state/nestedComposition.svelte.ts';
import {authState} from '/src/lib/state/auth.svelte.ts';
authState.token='browser-qualification';nestedComposition.document=effectScalarProject();nestedComposition.activeCompositionId='root';nestedComposition.projectId='browser-source';nestedComposition.revision=3;nestedComposition.timeUs=1500000;nestedComposition.renderedOutputs={root:'old-render'};
Object.assign(nestedComposition.document.compositions.root.content.tracks[0].clips[0],{effects:[{kind:'concat_exposure',parameters:{stops:0},concatKeys:[{at:0,value:-2,ease:'linear'},{at:.5,value:-1,derived:true,ease:[.42,-.5,.58,1]},{at:1,value:0,ease:'out'}]}]});
window.sourceRunQualification={nestedComposition,nestedHistory,undoNestedComposition,redoNestedComposition,original:JSON.stringify(nestedComposition.document)};
document.addEventListener('pointerdown',event=>{window.sourceRunPointer=event.pointerId});
const inspector=mount(Inspector,{target:document.querySelector('#app')});window.sourceRunQualification.closeEditor=()=>unmount(inspector);window.sourceRunQualification.sourceRunClipboard=sourceRunClipboard;
window.sourceRunQualification.showLumetri=async()=>{
 const {default:Timeline}=await import('/src/lib/components/composition/NestedScalarTimeline.svelte');const {effectLumetriScalarProject}=await import('/src/lib/composition/fixtures/nativeEffectScalarProject.ts');
 closeNestedComposition();authState.token='browser-qualification';nestedComposition.document=effectLumetriScalarProject();nestedComposition.activeCompositionId='root';nestedComposition.projectId='browser-lumetri';nestedComposition.revision=3;nestedComposition.timeUs=1500000;window.sourceRunQualification.lumetriOriginal=JSON.stringify(nestedComposition.document);
 const timeline=mount(Timeline,{target:document.querySelector('#app')});window.sourceRunQualification.closeEditor=()=>unmount(timeline);
};
</script>`,{flag:'wx'})
const reservation=createServer();await new Promise(resolve=>reservation.listen(0,'127.0.0.1',resolve));const port=reservation.address().port;await new Promise(resolve=>reservation.close(resolve))
let logs='',browser
const server=spawn(process.execPath,[fileURLToPath(new URL('../node_modules/vite/bin/vite.js',import.meta.url)),'--host','127.0.0.1','--port',String(port),'--strictPort'],{cwd:root,stdio:['ignore','pipe','pipe']})
server.stdout.on('data',chunk=>{logs=(logs+chunk).slice(-8000)});server.stderr.on('data',chunk=>{logs=(logs+chunk).slice(-8000)})
const url=`http://127.0.0.1:${port}/${name}`
try{
 let ready=false
 for(let attempt=0;attempt<100;attempt++){if(server.exitCode!==null)throw new Error(`Vite exited: ${logs}`);try{const response=await fetch(url);if(response.ok&&(await response.text()).includes('Source key graph qualification')){ready=true;break}}catch{/* Vite may not be listening yet; retry within the startup deadline. */}await new Promise(resolve=>setTimeout(resolve,100))}
 assert.ok(ready,'Vite readiness deadline')
 browser=await chromium.launch({headless:true,...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH?{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH}:{})})
 const page=await browser.newPage({viewport:{width:1100,height:900}}),errors=[],validations=[];let deferValidation=false,releaseValidation
 page.on('pageerror',error=>errors.push(error.message))
 await page.route('**/api/compositions/nested-frame',async route=>{validations.push(route.request().postDataJSON());if(deferValidation)await new Promise(resolve=>{releaseValidation=resolve});await route.fulfill({contentType:'application/json',body:JSON.stringify({entries:[]})})})
 await page.goto(url);await page.getByText('Ключи Concat',{exact:true}).click()
 const graph=page.getByRole('group',{name:'shape concat_exposure график исходных ключей',exact:true}),point=page.getByRole('button',{name:'shape concat_exposure исходный ключ 1',exact:true})
 await graph.waitFor({state:'visible'})
 const box=await graph.boundingBox(),bounds=await point.boundingBox();assert.ok(box&&bounds)
 const origin={x:bounds.x+bounds.width/2,y:bounds.y+bounds.height/2},delta=box.width*600/640*.25
 await page.mouse.move(origin.x,origin.y);await page.mouse.down()
 assert.equal(await point.evaluate(element=>element.hasPointerCapture(window.sourceRunPointer)),true,'real pointer capture')
 await page.mouse.move(origin.x-delta,origin.y,{steps:8})
 assert.equal(await page.evaluate(()=>JSON.stringify(window.sourceRunQualification.nestedComposition.document)===window.sourceRunQualification.original),true)
 assert.equal(validations.length,0)
 await page.mouse.up();await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===1)
 const accepted=await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document)))
 assert.equal(accepted.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[1].at,.25)
 assert.equal(accepted.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[1].derived,true)
 assert.deepEqual(accepted.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[1].ease,[.42,-.5,.58,1])
 assert.equal(validations.length,1)
 await page.screenshot({path:fileURLToPath(new URL('accepted.png',output)),fullPage:true});await graph.screenshot({path:fileURLToPath(new URL('graph.png',output))})
 await page.evaluate(()=>window.sourceRunQualification.undoNestedComposition());await page.waitForFunction(()=>JSON.stringify(window.sourceRunQualification.nestedComposition.document)===window.sourceRunQualification.original)
 await page.evaluate(()=>window.sourceRunQualification.redoNestedComposition());await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===1)
 assert.deepEqual(await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document))),accepted)
 const next=await point.boundingBox();assert.ok(next)
 await page.mouse.move(next.x+next.width/2,next.y+next.height/2);await page.mouse.down();await page.mouse.move(next.x+60,next.y)
 await page.keyboard.press('Escape');await page.mouse.up()
 assert.deepEqual(await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document))),accepted)
 assert.equal(validations.length,1)
 await point.focus();await page.keyboard.press('Alt+ArrowLeft');await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===2)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[1].at),.249)
 assert.equal(validations.length,2)
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 0',exact:true}).click({modifiers:['Shift']})
 assert.equal(await page.getByRole('button',{name:'shape concat_exposure исходный ключ 0',exact:true}).getAttribute('aria-pressed'),'true');assert.equal(await point.getAttribute('aria-pressed'),'true')
 await point.focus();await page.keyboard.press('Alt+ArrowRight');await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===3)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[0].at),.001)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[1].at),.25)
 assert.equal(validations.length,3);await graph.screenshot({path:fileURLToPath(new URL('group.png',output))})
 const beforeInsertion=await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document)))
 await page.getByRole('button',{name:'shape concat_exposure добавить исходный ключ',exact:true}).dblclick({position:{x:450,y:80}})
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===4)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.length),4)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys[2].at),.75)
 await graph.screenshot({path:fileURLToPath(new URL('inserted.png',output))})
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 2',exact:true}).focus();await page.keyboard.press('Delete')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===5)
 assert.deepEqual(await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document))),beforeInsertion)
 assert.equal(validations.length,5)
 await page.evaluate(()=>window.sourceRunQualification.undoNestedComposition());await page.waitForFunction(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.length===4)
 await page.evaluate(()=>window.sourceRunQualification.redoNestedComposition());await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===5)
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 0',exact:true}).focus();await page.keyboard.press('ControlOrMeta+a');await page.keyboard.press('Delete')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===6)
 assert.equal(await graph.isVisible(),true);assert.equal(await graph.locator('circle').count(),0)
 await graph.screenshot({path:fileURLToPath(new URL('empty.png',output))})
 await page.getByRole('button',{name:'shape concat_exposure добавить исходный ключ',exact:true}).focus();await page.keyboard.press('Enter')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===7)
 assert.deepEqual(await page.evaluate(()=>JSON.parse(JSON.stringify(window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys))),[{at:.5,value:0,ease:[0,0,1,1]}])
 assert.equal(validations.length,7)
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 0',exact:true}).click();await page.keyboard.press('ControlOrMeta+c')
 await page.getByLabel('shape concat_exposure позиция вставки исходных ключей',{exact:true}).fill('.75')
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 0',exact:true}).focus();await page.keyboard.press('ControlOrMeta+v')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===8)
 assert.deepEqual(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.map(key=>key.at)),[.5,.75])
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 1',exact:true}).click();await page.keyboard.press('ControlOrMeta+x')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===9)
 await page.getByLabel('shape concat_exposure позиция вставки исходных ключей',{exact:true}).fill('.875')
 await page.getByRole('button',{name:'shape concat_exposure добавить исходный ключ',exact:true}).focus();await page.keyboard.press('ControlOrMeta+v')
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===10)
 assert.deepEqual(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.map(key=>key.at)),[.5,.875]);assert.equal(validations.length,10)
 await graph.screenshot({path:fileURLToPath(new URL('clipboard.png',output))})

 await graph.screenshot({path:fileURLToPath(new URL('first-key.png',output))})
 await page.getByLabel('shape Concat source 1 value',{exact:true}).fill('-1');await page.getByLabel('shape Concat source 1 value',{exact:true}).press('Tab');await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===11)
 deferValidation=true
 await page.getByRole('button',{name:'shape concat_exposure исходный ключ 1',exact:true}).click();await page.keyboard.press('ControlOrMeta+x')
 for(let attempt=0;attempt<100&&!releaseValidation;attempt++)await new Promise(resolve=>setTimeout(resolve,20))
 assert.ok(releaseValidation,'cut validation request was not issued')
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.sourceRunClipboard.value.keys[0].value),0)
 await page.evaluate(()=>window.sourceRunQualification.closeEditor());assert.equal(await graph.count(),0)
 releaseValidation()
 await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===12&&window.sourceRunQualification.sourceRunClipboard.value.keys[0].value===-1)
 assert.equal(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.length),1);assert.equal(validations.length,12)
 deferValidation=false
 await page.evaluate(()=>window.sourceRunQualification.showLumetri())
 await page.getByRole('button',{name:'Выбрать все ключи',exact:true}).click();await page.getByText('График значений и кривых',{exact:true}).click()
 const lumetri=page.getByRole('group',{name:'График нативных скалярных ключей',exact:true});await lumetri.waitFor({state:'visible'})
 assert.equal(await lumetri.getByRole('button',{name:'Точка графика shape lumetri #1 lumetri.basic.exposure 250',exact:true}).count(),1)
 await page.getByLabel('Число для значений нативных ключей',{exact:true}).fill('-1');await page.getByRole('button',{name:'Предпросмотр значений ключей',exact:true}).click()
 assert.equal(await page.evaluate(()=>JSON.stringify(window.sourceRunQualification.nestedComposition.document)===window.sourceRunQualification.lumetriOriginal),true)
 await page.getByRole('button',{name:'Применить значения ключей',exact:true}).click();await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===1)
 assert.deepEqual(await page.evaluate(()=>window.sourceRunQualification.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].parameters.animation.basic.exposure.track.keyframes.map(key=>key.value)),[-3,-2,-1]);assert.equal(validations.length,13)
 await lumetri.screenshot({path:fileURLToPath(new URL('lumetri.png',output))})
 await page.evaluate(()=>window.sourceRunQualification.undoNestedComposition());await page.waitForFunction(()=>JSON.stringify(window.sourceRunQualification.nestedComposition.document)===window.sourceRunQualification.lumetriOriginal)
 await page.evaluate(()=>window.sourceRunQualification.redoNestedComposition());await page.waitForFunction(()=>window.sourceRunQualification.nestedHistory.past.length===1)
 assert.deepEqual(errors,[])
 assert.deepEqual(await hashes(),initialHashes,'sources changed during browser qualification')
 await writeFile(new URL('evidence.json',output),JSON.stringify({status:'passed',browser:browser.version(),checks:['visible inspector graph','real mouse pointer capture','local draft isolation','one accepted native-validation request','normalized position','derived/easing retention','Undo/Redo','Escape cancellation','Alt keyboard nudge','double-click insertion','Delete exact source index','insert/delete Undo/Redo','last-key deletion retains graph','first key from empty graph','Shift multiselection','one-transaction group nudge','Ctrl+A group deletion','local source-key copy/paste','atomic cut buffer acceptance','native cut acceptance after editor destruction','nested Lumetri shared graph preview/apply','nested Lumetri Undo/Redo','no browser errors'],validation:'HTTP validation stubbed; real Rust raster is qualified separately by qualify-native-scalar-tangents.py',inputs:initialHashes},null,2)+'\n')
 console.log(`Browser evidence: ${fileURLToPath(output)}`)
}finally{
 await browser?.close()
 if(server.exitCode===null){const stopped=once(server,'exit');server.kill('SIGTERM');const timer=setTimeout(()=>server.kill('SIGKILL'),5000);timer.unref();await stopped;clearTimeout(timer)}
 await unlink(html)
}
