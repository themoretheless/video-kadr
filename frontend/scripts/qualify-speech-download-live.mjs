// Real native API integration. Run against a fresh disposable backend and Vite.
import { chromium } from '@playwright/test'
import { readFile, writeFile, unlink } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import assert from 'node:assert/strict'
const root = fileURLToPath(new URL('../', import.meta.url))
const fixture = root + 'speech-download-live.html'
const api = process.env.SPEECH_TEST_API || 'http://127.0.0.1:3017'
const ui = process.env.SPEECH_TEST_UI || 'http://127.0.0.1:5189'
const hash = '818710568da3ca15689e31a743197b520007872ff9576237bda97bd1b469c3d7'
const browser = await chromium.launch({ executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH || '/Users/themoretheless/Library/Caches/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell' })
try {
 const context = await browser.newContext()
 const response = await context.request.post(api + '/api/auth/register', { data: { username: 'download-live-' + Date.now(), password: 'public fixture integration password' } })
 assert.equal(response.ok(), true); const {token} = await response.json()
 const headers = {authorization: `Bearer ${token}`}
 const upload = await context.request.post(api + '/api/upload', { headers, multipart: { file: { name: 'public-jfk.wav', mimeType: 'audio/wav', buffer: await readFile(process.env.SPEECH_TEST_AUDIO || '/opt/homebrew/Cellar/whisper.cpp/1.9.5/share/whisper.cpp/jfk.wav') } } })
 assert.equal(upload.ok(), true); const source = await upload.json()
 await writeFile(fixture, `<html lang="ru"><meta charset="utf-8"><body><main id="app"></main><script type="module">import {mount} from 'svelte';import Panel from '/src/lib/components/composition/NativeSpeechPanel.svelte';import {state} from '/src/lib/state/store.svelte';import '/src/app.css';const data=JSON.parse(sessionStorage.getItem('speech-test'));if(data){state.library=[{...data.source,kind:'source'}];mount(Panel,{target:document.getElementById('app'),props:{token:data.token}})}</script></body></html>`)
 const page = await context.newPage(); const errors = []; const downloads = []
 page.on('pageerror', e=>errors.push(String(e)))
 page.on('response', r=>{if(new URL(r.url()).pathname==='/api/speech/model-downloads' && r.request().method()==='POST')downloads.push(r.json())})
 await page.addInitScript(data=>sessionStorage.setItem('speech-test',JSON.stringify(data)), {token,source})
 await page.setViewportSize({width:900,height:900}); await page.goto(ui + '/speech-download-live.html')
 await page.getByText('Распознать речь локально',{exact:true}).click(); await page.getByText('Модели распознавания речи',{exact:true}).click()
 await page.getByRole('button',{name:'Загрузить каталог моделей'}).click(); await page.getByLabel('Модель для скачивания').selectOption('tiny-q5_1')
 const admission = page.waitForResponse(r=>new URL(r.url()).pathname==='/api/speech/model-downloads' && r.request().method()==='POST')
 await page.getByRole('button',{name:'Скачать модель',exact:true}).click(); await admission
 // Detach the observer with a real reload; reconnect through durable owner history.
 await page.reload();await page.getByText('Распознать речь локально',{exact:true}).click();await page.getByText('Модели распознавания речи',{exact:true}).click();await page.getByText('История скачивания',{exact:true}).click()
 const history=page.locator('li button').filter({hasText:'tiny-q5_1'});await history.first().waitFor();await history.first().click()
 await page.getByLabel('Модель распознавания речи').locator(`option[value="${hash}"]`).waitFor({state:'attached',timeout:180000})
 assert.equal(await page.getByLabel('Модель распознавания речи').inputValue(),'configured')
 await page.getByLabel('Модель распознавания речи').selectOption(hash)
 await page.getByRole('status').filter({hasText:'Модель: ggml-tiny-q5_1.bin'}).waitFor()
 await page.getByLabel('Источник распознавания речи').selectOption(source.id);await page.getByLabel('Конец распознавания речи').fill('11');await page.getByLabel('Язык распознавания речи',{exact:true}).selectOption('en')
 await page.getByRole('button',{name:'Распознать речь',exact:true}).click()
 await page.getByLabel('Текст распознанного фрагмента 1',{exact:true}).waitFor({timeout:60000})
 const texts = await page.locator('textarea').evaluateAll(nodes=>nodes.map(n=>n.value).join(' ')); assert.match(texts,/country/i)
 const historyResponse=await context.request.get(api+'/api/speech/jobs',{headers});const speechHistory=await historyResponse.json();const speechId=speechHistory.jobs[0].jobId
 const speech=await (await context.request.get(api+'/api/jobs/'+speechId,{headers})).json();assert.equal(speech.status,'done');assert.equal(speech.result.modelHash,hash)
 assert.equal(downloads.length,1);const admitted=await downloads[0];assert.ok(admitted.jobId)
 await page.screenshot({path:'/tmp/video-kadr-speech-download-live-desktop.png'});await page.setViewportSize({width:390,height:844});await page.screenshot({path:'/tmp/video-kadr-speech-download-live-mobile.png'})
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);assert.deepEqual(errors,[])
 console.log('PASS real catalog download, page reload/history reconnect, explicit model selection, learned JFK recognition/model hash; desktop/mobile')
} finally { await browser.close(); await unlink(fixture).catch(()=>{}) }
