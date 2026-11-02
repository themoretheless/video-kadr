// Actual model-free region preview/authoring against a disposable authenticated backend.
import {chromium} from '@playwright/test'
import {readFile,writeFile,unlink} from 'node:fs/promises'
import {dirname,join} from 'node:path'
import {fileURLToPath} from 'node:url'
import {createHash} from 'node:crypto'
import {execFileSync} from 'node:child_process'
import assert from 'node:assert/strict'
const data=JSON.parse(await readFile(process.env.REGION_BROWSER_FIXTURE,'utf8'))
const api=process.env.SPEECH_TEST_API||'http://127.0.0.1:3017',ui=process.env.SPEECH_TEST_UI||'http://127.0.0.1:5189'
const output=process.env.REGION_BROWSER_OUTPUT||dirname(process.env.REGION_BROWSER_FIXTURE),fixture=fileURLToPath(new URL('../video-region-frame-live.html',import.meta.url)),ffmpeg=process.env.VIDEO_KADR_TEST_FFMPEG||'/opt/homebrew/bin/ffmpeg'
const headers={authorization:'Bearer '+data.token},errors=[]
const hash=bytes=>createHash('sha256').update(bytes).digest('hex')
const oracle=JSON.parse(await readFile(join(dirname(data.expectedMasks),'valid.json'),'utf8')),planes=await readFile(oracle.regions[0].input)
const browser=await chromium.launch({executablePath:process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH||'/Users/themoretheless/Library/Caches/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell'})
let page
try{
 const context=await browser.newContext();page=await context.newPage();page.setDefaultTimeout(30000);page.on('pageerror',error=>errors.push(String(error)))
 await writeFile(fixture,`<html lang="ru"><meta charset="utf-8"><body><main id="app"></main><script type="module">
 import {mount} from 'svelte';import Editor from '/src/lib/components/composition/VideoMaskBrushEditor.svelte';import '/src/app.css';
 const data=JSON.parse(sessionStorage.getItem('video-region-frame'));window.regionFrameTest={ready:false,dirty:false,result:null};mount(Editor,{target:document.getElementById('app'),props:{token:data.token,jobId:data.jobId,width:37,height:23,frames:4,frameRate:{numerator:4,denominator:1},onready:id=>window.regionFrameTest.result=id,onchanged:value=>window.regionFrameTest.dirty=value}});window.regionFrameTest.ready=true;
 </script></body></html>`)
 await page.addInitScript(value=>sessionStorage.setItem('video-region-frame',JSON.stringify(value)),{token:data.token,jobId:data.jobId})
 await page.setViewportSize({width:900,height:850});await page.goto(ui+'/video-region-frame-live.html');await page.waitForFunction(()=>window.regionFrameTest?.ready)
 await page.getByText('Правка видеомаски',{exact:true}).click();await page.getByRole('button',{name:'Открыть видеомаски',exact:true}).click();await page.getByLabel('Полотно видеомаски').waitFor({state:'visible'})
 async function frame(n){await page.getByLabel('Кадр видеомаски',{exact:true}).fill(String(n));await page.getByLabel('Кадр видеомаски',{exact:true}).dispatchEvent('change');await page.getByRole('button',{name:'Показать область кадра',exact:true}).waitFor({state:'visible'});await page.waitForFunction(()=>!document.querySelector('image[data-video-region-preview]'))}
 async function preview(n){
  const waiting=page.waitForResponse(r=>new URL(r.url()).pathname===`/api/person/video-jobs/${data.jobId}/regions/${data.regionId}/${n}`);await page.getByRole('button',{name:'Показать область кадра',exact:true}).click();const response=await waiting;assert.equal(response.status(),200);assert.equal(response.headers()['cache-control'],'no-store');assert.equal(response.headers()['x-source-frame'],String(n));assert.equal(response.headers()['x-mask-sha256'],hash(planes.subarray(n*65536,(n+1)*65536)))
  await page.locator('image[data-video-region-preview]').waitFor();const actual=await page.evaluate(async()=>{const href=document.querySelector('image[data-video-region-preview]').getAttribute('href'),blob=await(await fetch(href)).blob(),bitmap=await createImageBitmap(blob),canvas=new OffscreenCanvas(bitmap.width,bitmap.height),ctx=canvas.getContext('2d');ctx.drawImage(bitmap,0,0);const rgba=ctx.getImageData(0,0,bitmap.width,bitmap.height).data,gray=new Uint8Array(bitmap.width*bitmap.height);for(let i=0;i<gray.length;i++){if(rgba[i*4+3]!==255||rgba[i*4]!==rgba[i*4+1]||rgba[i*4]!==rgba[i*4+2])throw new Error('region is not opaque gray');gray[i]=rgba[i*4]}const digest=await crypto.subtle.digest('SHA-256',gray);bitmap.close();return {width:canvas.width,height:canvas.height,hash:Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('')}})
  assert.deepEqual(actual,{width:256,height:256,hash:hash(planes.subarray(n*65536,(n+1)*65536))});assert.equal(await page.evaluate(()=>window.regionFrameTest.dirty),n!==1)
 }
 await frame(1);await page.getByLabel('Основа полотна видеомаски').selectOption('overlay');await preview(1)
 await page.screenshot({path:join(output,'region-desktop.png'),fullPage:true});await page.setViewportSize({width:390,height:844});await page.screenshot({path:join(output,'region-mobile.png'),fullPage:true});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await page.setViewportSize({width:900,height:850})
 await page.getByRole('button',{name:'Сохранить область кадра',exact:true}).click();await page.waitForFunction(()=>window.regionFrameTest.dirty)
 await frame(2);await preview(2);await page.getByRole('button',{name:'Удалить область кадра',exact:true}).click();await page.getByRole('button',{name:'Отменить правку кадра',exact:true}).click();await page.getByRole('button',{name:'Повторить правку кадра',exact:true}).click()
 const admission=page.waitForResponse(r=>r.request().method()==='POST'&&new URL(r.url()).pathname===`/api/person/video-jobs/${data.jobId}/corrections`);await page.getByRole('button',{name:'Пересчитать видеомаски',exact:true}).click();const response=await admission;assert.equal(response.status(),202,await response.text());const payload=response.request().postDataJSON();assert.deepEqual(payload.corrections.map(e=>e.frame),[1,2]);for(const edit of payload.corrections){assert.equal(edit.correction.strokes.length,1);assert.equal(edit.correction.strokes[0].region.maskSourceId,data.regionId);assert.equal(edit.correction.strokes[0].region.maskHash,hash(planes.subarray(edit.frame*65536,(edit.frame+1)*65536)))}
 const {jobId}=await response.json();await page.waitForFunction(id=>window.regionFrameTest.result===id,jobId);let job;const deadline=Date.now()+90000;while(Date.now()<deadline){const result=await context.request.get(api+'/api/jobs/'+jobId,{headers});assert.equal(result.status(),200);job=await result.json();if(!['pending','running'].includes(job.status))break;await new Promise(resolve=>setTimeout(resolve,100))}assert.equal(job?.status,'done',JSON.stringify(job))
 const masks=await context.request.get(api+'/api/person/video-jobs/'+jobId+'/masks',{headers});assert.equal(masks.status(),200);assert.deepEqual(await masks.body(),await readFile(data.expectedMasks))
 const movie=await context.request.get(api+'/api/person/video-jobs/'+jobId+'/video',{headers});assert.equal(movie.status(),200);const target=join(output,'browser-result.mov');await writeFile(target,await movie.body(),{mode:0o600});const rgba=execFileSync(ffmpeg,['-nostdin','-v','error','-i',target,'-map','0:v:0','-an','-fps_mode','passthrough','-pix_fmt','rgba','-f','rawvideo','pipe:1']);assert.deepEqual(rgba,await readFile(data.expectedRgba))
 const recipe=await context.request.get(api+'/api/person/video-jobs/'+jobId+'/corrections',{headers});assert.equal(recipe.status(),200);assert.deepEqual((await recipe.json()).corrections,payload.corrections);assert.deepEqual(errors,[])
 console.log('PASS real browser/private PNG plane hashes, actual SVG Blob pixels, per-frame cached smart keep/erase, Undo/Redo, durable model-free API correction, exact independent masks/RGBA and persisted annotations; no route mocks. Desktop/mobile inspected separately.')
}catch(error){if(page){await page.screenshot({path:join(output,'region-failure.png'),fullPage:true});console.error(await page.locator('main').innerText())}throw error}finally{await browser.close();await unlink(fixture).catch(()=>{})}
