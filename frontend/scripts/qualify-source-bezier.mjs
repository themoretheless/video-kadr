import assert from 'node:assert/strict'
import {resolve} from 'node:path'
import {fileURLToPath} from 'node:url'
import {mkdirSync,writeFileSync} from 'node:fs'
import {createServer} from 'vite'
import {chromium} from '@playwright/test'
const root=resolve(fileURLToPath(new URL('..',import.meta.url)))
const output=resolve(root,'../backend/target/qualification-logs')
mkdirSync(output,{recursive:true})
const server=await createServer({root,cacheDir:resolve(output,'source-bezier-vite-cache'),plugins:[{name:'source-bezier-qa',resolveId(id){if(id==='/source-bezier-runtime')return '\0source-bezier-runtime'},load(id){if(id==='\0source-bezier-runtime')return `export {mount,unmount,tick} from 'svelte'`}}],server:{host:'127.0.0.1',port:0,watch:null,hmr:false}})
let browser
try {
 await server.listen()
 browser=await chromium.launch({headless:true,...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH?{executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH}:{})})
 const page=await browser.newPage({viewport:{width:900,height:1100}}),errors=[]
 page.on('pageerror',error=>errors.push(error.message))
 await page.route('**/source-bezier-qa',route=>route.fulfill({contentType:'text/html',body:'<meta charset="utf-8"><div id="app"></div>'}))
 await page.route('**/api/compositions/nested-frame',route=>route.fulfill({contentType:'application/json',body:JSON.stringify({entries:[]})}))
 await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/source-bezier-qa`)
 await page.evaluate(async()=>{
  const {mount}=await import('/source-bezier-runtime')
  const {default:Inspector}=await import('/src/lib/components/composition/NestedConcatChromaKeyInspector.svelte')
  const state=await import('/src/lib/state/nestedComposition.svelte.ts');state.closeNestedComposition()
  state.nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:0},effects:[{kind:'concat_chroma_key',parameters:{color:16711935,similarity:1,softness:0,spill:0},concatKeys:{similarity:[{at:0,value:60},{at:1,value:1,ease:[0.42,0,0.58,1]}]}}]}]}]},instances:[]}}}
  window.__sourceBezierMount=mount(Inspector,{target:document.querySelector('#app')});window.__sourceBezierQa=state
 })
 await page.locator('details').evaluateAll(nodes=>nodes.forEach(node=>{node.open=true}))
 const label='clip Concat chroma similarity 1'
 const graph=page.getByRole('group',{name:`${label} Безье`,exact:true})
 const p1=page.getByRole('button',{name:`${label} P1`,exact:true}),p2=page.getByRole('button',{name:`${label} P2`,exact:true})
 const history=async count=>page.waitForFunction(n=>window.__sourceBezierQa.nestedHistory.past.length===n&&!window.__sourceBezierQa.nestedHistory.busy,count)
 const points=()=>page.evaluate(()=>window.__sourceBezierQa.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.similarity[1].ease)
 const target=async(x,y,low=-0.25,high=1.25)=>{const b=await graph.boundingBox();assert.ok(b);const scale=Math.min(b.width/280,b.height/180);return{x:b.x+(b.width-280*scale)/2+(24+x*232)*scale,y:b.y+(b.height-180*scale)/2+(16+(high-y)/(high-low)*148)*scale}}
 await p1.scrollIntoViewIfNeeded()
 let b=await p1.boundingBox();assert.ok(b);await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down()
 let to=await target(0.42,-0.5);await page.mouse.move(to.x,to.y,{steps:8})
 assert.equal(await page.evaluate(()=>window.__sourceBezierQa.nestedHistory.past.length),0)
 assert.deepEqual(await points(),[0.42,0,0.58,1])
 await page.mouse.up();await history(1)
 let actual=await points();assert.ok(Math.abs(actual[0]-0.42)<1e-6&&Math.abs(actual[1]+0.5)<1e-6,JSON.stringify(actual))
 await p1.focus();await p1.press('ArrowUp');await history(2)
 actual=await points();assert.ok(Math.abs(actual[1]+0.49)<1e-6)
 await p1.press('Shift+ArrowRight');await history(3);await p1.press('Alt+ArrowLeft');await history(4)
 actual=await points();assert.ok(Math.abs(actual[0]-0.519)<1e-6)
 await page.evaluate(()=>{const s=window.__sourceBezierQa;s.undoNestedComposition();s.undoNestedComposition();s.undoNestedComposition()});await history(1)
 const before=await points();b=await p2.boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+40,b.y-30,{steps:4});await p2.press('Escape');await page.mouse.up();assert.deepEqual(await points(),before);await history(1)
 b=await p1.boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await page.mouse.down();await page.mouse.move(b.x+30,b.y-20,{steps:3})
 await page.evaluate(()=>{const s=window.__sourceBezierQa;const doc=structuredClone(JSON.parse(JSON.stringify(s.nestedComposition.document)));doc.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.similarity[0].value=59;s.nestedComposition.document=doc})
 await page.mouse.up();await history(1);assert.deepEqual(await points(),before)
 await page.evaluate(()=>{const s=window.__sourceBezierQa;const doc=JSON.parse(JSON.stringify(s.nestedComposition.document));doc.compositions.root.content.tracks[0].clips[0].effects[0].concatKeys.similarity[0].value=60;s.nestedComposition.document=doc})
 const exactY=page.locator('[aria-label="clip Concat chroma similarity 1 ease 1"]');await exactY.fill('-0.5');await exactY.dispatchEvent('change');await history(2)
 await page.setViewportSize({width:380,height:1100});await graph.scrollIntoViewIfNeeded()
 const narrow=await graph.boundingBox();assert.ok(narrow.x>=0&&narrow.x+narrow.width<=382,'graph fits narrow panel')
 await graph.screenshot({path:resolve(output,'concat-source-bezier-chromium.png')})
 const effect=await page.evaluate(()=>JSON.parse(JSON.stringify(window.__sourceBezierQa.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0])))
 writeFileSync(resolve(output,'concat-chroma-browser-bezier-effect.json'),JSON.stringify(effect))
 await page.evaluate(async()=>{
  const {mount,unmount}=await import('/source-bezier-runtime');await unmount(window.__sourceBezierMount)
  const {default:Inspector}=await import('/src/lib/components/composition/NestedConcatGaussianBlurInspector.svelte')
  const state=window.__sourceBezierQa;state.closeNestedComposition()
  state.nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:{tracks:[{kind:'video',id:'video',name:'Video',locked:false,clips:[{id:'clip',placement:{timelineStartTick:0},effects:[{kind:'concat_gaussian_blur',parameters:{radius:10},concatKeys:[{at:0,value:1},{at:1,value:10,ease:[0.42,0,0.58,1]}]}]}]}]},instances:[]}}}
  window.__sourceBezierMount=mount(Inspector,{target:document.querySelector('#app')})
 })
 await page.locator('details').evaluateAll(nodes=>nodes.forEach(node=>{node.open=true}))
 const gaussHandle=page.getByRole('button',{name:'clip Concat source 1 P1',exact:true}),gaussGraph=page.getByRole('group',{name:'clip Concat source 1 Безье',exact:true})
 await gaussHandle.scrollIntoViewIfNeeded();const gb=await gaussGraph.boundingBox(),hb=await gaussHandle.boundingBox();assert.ok(gb&&hb)
 await page.mouse.move(hb.x+hb.width/2,hb.y+hb.height/2);await page.mouse.down()
 const scale=Math.min(gb.width/280,gb.height/180)
 await page.mouse.move(gb.x+(gb.width-280*scale)/2+(24+0.42*232)*scale,gb.y+(gb.height-180*scale)/2+(16+(1.25-0.25)/1.5*148)*scale,{steps:6});await page.mouse.up();await history(1)
 const gy=page.locator('[aria-label="clip Concat source 1 ease 1"]');await gy.fill('0.25');await gy.dispatchEvent('change');await history(2)
 const gaussEffect=await page.evaluate(()=>JSON.parse(JSON.stringify(window.__sourceBezierQa.nestedComposition.document.compositions.root.content.tracks[0].clips[0].effects[0])))
 assert.ok(Math.abs(gaussEffect.concatKeys[1].ease[0]-0.42)<1e-6);assert.equal(gaussEffect.concatKeys[1].ease[1],0.25)
 writeFileSync(resolve(output,'concat-gaussian-browser-bezier-effect.json'),JSON.stringify(gaussEffect))
 assert.deepEqual(errors,[])
 console.log(JSON.stringify({dragSingleUndo:true,keyboard:true,escapeNoCommit:true,staleGestureDiscarded:true,numericFineTune:true,narrowViewport:true,chromaEase:effect.concatKeys.similarity[1].ease,gaussianEase:gaussEffect.concatKeys[1].ease,screenshot:resolve(output,'concat-source-bezier-chromium.png')}))
} finally {await browser?.close();await server.close()}
