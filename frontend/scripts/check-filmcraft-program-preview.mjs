import { chromium } from '@playwright/test'
import { readFile, writeFile, mkdir, mkdtemp, rm } from 'node:fs/promises'
import { execFileSync } from 'node:child_process'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import assert from 'node:assert/strict'
const base = process.env.FILMCRAFT_PREVIEW_URL ?? 'http://127.0.0.1:5198'
const fixture = JSON.parse(await readFile(new URL('../../backend/tests/fixtures/filmcraft-source-masks.json', import.meta.url), 'utf8'))
const sample = fixture.cases.find(c => c.scale === 1 && c.effect === 'invert' && c.mask_mode === 'Add' && c.profile === 1)
assert(sample)
assert.equal((await fetch(`${base}/api/health`)).status, 200, 'backend must be ready before browser validation')
const browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE } : {}) })
try {
 const page = await browser.newPage({ viewport: { width: 1200, height: 900 } })
 const errors = []; page.on('pageerror', e => errors.push(e.message))
 await page.goto(`${base}/e2e/fixtures/live-filmcraft-program.html`)
 await page.getByText('Проект Filmcraft', { exact: true }).click()
 await page.locator('input[type=file]').setInputFiles({ name: 'masked.filmcraft.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(sample.document)) })
 await page.getByLabel('Позиция правки, кадров', { exact: true }).waitFor()
 const responsePromise = page.waitForResponse(r => r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
 await page.getByLabel('Предпросмотр композиции', { exact: true }).check()
 const response = await responsePromise; assert.equal(response.status(), 200)
 const frame = await response.json()
 const canvas = page.getByLabel('Кадр композиции Filmcraft', { exact: true }); await canvas.waitFor()
 const rendered = await canvas.evaluate(c => ({ width: c.width, height: c.height, data: Array.from(c.getContext('2d').getImageData(0, 0, c.width, c.height).data) }))
 assert.equal(rendered.width, frame.width); assert.equal(rendered.height, frame.height)
 const expected = Buffer.from(frame.rgbaHex, 'hex')
 // Browser canvas internally premultiplies RGBA8, so RGB can round by <=1/alpha.
 for(let i = 0; i < expected.length; i += 4) {
  assert.equal(rendered.data[i+3], expected[i+3])
  const alpha=expected[i+3]; if(alpha) for(let c=0;c<3;c++) assert(Math.abs(rendered.data[i+c]-expected[i+c]) <= Math.ceil(255/alpha), `canvas pixel ${i+c}`)
 }
 const next = page.waitForResponse(r => r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
 await page.getByLabel('Позиция правки, кадров', { exact: true }).fill('12')
 assert.equal((await next).status(),200)
 await page.getByText('Кадр композиции · время: 127008000000', { exact: true }).waitFor()
 await page.getByLabel('Предпросмотр композиции', { exact: true }).uncheck()
 assert.equal(await canvas.count(),0)
 await mkdir('/tmp/video-kadr-filmcraft-preview-stage1930-artifacts', { recursive: true })
 await page.getByLabel('Предпросмотр композиции', { exact: true }).check(); await canvas.waitFor()
 await page.screenshot({ path: '/tmp/video-kadr-filmcraft-preview-stage1930-artifacts/program-preview.png', fullPage: true })
 const registered = await page.request.post(`${base}/api/auth/register`, { data: { username: `program-image-${Date.now()}`, password: 'program-image-test-password' } }); assert.equal(registered.status(),201)
 const { token } = await registered.json()
 const png = await page.evaluate(() => {
  const c=document.createElement('canvas');c.width=8;c.height=8;const ctx=c.getContext('2d');const pixels=ctx.createImageData(8,8)
  for(let i=0;i<64;i++)pixels.data.set([(i*37+11)%256,(i*73+51)%256,(i*19+83)%256,[0,1,64,128,255][i%5]],i*4)
  ctx.putImageData(pixels,0,0);return c.toDataURL('image/png').split(',')[1]
 })
 const uploaded=await page.request.post(`${base}/api/upload`, { headers: { Authorization: `Bearer ${token}` }, multipart: { file: { name: 'pattern.png', mimeType: 'image/png', buffer: Buffer.from(png,'base64') } } }); assert.equal(uploaded.status(),200)
 const entry=await uploaded.json();assert(entry.id)
 await page.evaluate(value=>localStorage.setItem('video-kadr.filmcraft-program-test',JSON.stringify(value)),{token,library:[{...entry,kind:'source'}]})
 await page.reload();await page.getByText('Проект Filmcraft', { exact: true }).click()
 const fileDocument=structuredClone(sample.document);fileDocument.project.items['1'].kind.Media.media={File:{path:'/original/unavailable/pattern.png'}}
 await page.locator('input[type=file]').setInputFiles({ name: 'file-image.filmcraft.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(fileDocument)) })
 await page.locator('select[aria-label^="Файл для "]').first().selectOption(entry.id)
 const imageResponse=page.waitForResponse(r=>r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
 await page.getByLabel('Предпросмотр композиции', { exact: true }).check();assert.equal((await imageResponse).status(),200)
 await canvas.waitFor();await page.screenshot({path:'/tmp/video-kadr-filmcraft-preview-stage1930-artifacts/program-image-preview.png',fullPage:true})
 const missingResponse=page.waitForResponse(r=>r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
 await page.locator('select[aria-label^="Файл для "]').first().selectOption('');assert.equal((await missingResponse).status(),400)
 await page.getByRole('alert').filter({hasText:'file source provider is not connected'}).waitFor();assert.equal(await canvas.count(),0)
 const videoDir=await mkdtemp(join(tmpdir(),'filmcraft-program-video-'))
 try {
  const raw=Buffer.alloc(8*8*4*3)
  for(let f=0;f<3;f++)for(let i=0;i<64;i++)raw.set([(i*37+f*29+11)%256,(i*73+f*17+51)%256,(i*19+f*43+83)%256,[0,1,64,128,255][(i+f)%5]],(f*64+i)*4)
  const rawPath=join(videoDir,'frames.rgba');const videoPath=join(videoDir,'pattern.mov');await writeFile(rawPath,raw)
  execFileSync('ffmpeg',['-nostdin','-v','error','-f','rawvideo','-pix_fmt','rgba','-s','8x8','-r','24','-i',rawPath,'-c:v','qtrle','-pix_fmt','argb','-video_track_timescale','24000','-threads','1',videoPath])
  const videoUpload=await page.request.post(`${base}/api/upload`,{headers:{Authorization:`Bearer ${token}`},multipart:{file:{name:'pattern.mov',mimeType:'video/quicktime',buffer:await readFile(videoPath)}}});assert.equal(videoUpload.status(),200)
  const videoEntry=await videoUpload.json();assert(videoEntry.id)
  await page.evaluate(value=>localStorage.setItem('video-kadr.filmcraft-program-test',JSON.stringify(value)),{token,library:[{...videoEntry,kind:'source',mediaType:'video'}]})
  await page.reload();await page.getByText('Проект Filmcraft',{exact:true}).click()
  const videoDocument=structuredClone(sample.document);videoDocument.project.items['1'].kind.Media.media={File:{path:'/original/unavailable/video.mov'}}
  await page.locator('input[type=file]').setInputFiles({name:'video.filmcraft.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(videoDocument))})
  await page.locator('select[aria-label^="Файл для "]').first().selectOption(videoEntry.id)
  const videoResponse=page.waitForResponse(r=>r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
  await page.getByLabel('Предпросмотр композиции',{exact:true}).check();const response0=await videoResponse;assert.equal(response0.status(),200);const frame0=await response0.json();await canvas.waitFor()
  const nextVideo=page.waitForResponse(r=>r.url().endsWith('/api/compositions/filmcraft/preview-frame'))
  await page.getByLabel('Позиция правки, кадров',{exact:true}).fill('1');const response1=await nextVideo;assert.equal(response1.status(),200);const frame1=await response1.json();assert.notEqual(frame1.rgbaHex,frame0.rgbaHex);assert.equal(frame1.sampleTicks,'10584000000')
  await page.getByText('Кадр композиции · время: 10584000000',{exact:true}).waitFor()
  await page.screenshot({path:'/tmp/video-kadr-filmcraft-preview-stage1930-artifacts/program-video-preview.png',fullPage:true})
 } finally {await rm(videoDir,{recursive:true,force:true})}
 assert.deepEqual(errors,[])
 console.log('PASS: real library video upload → composed mask frame → source time update → changed video pixels; no page errors')
 console.log('PASS: real Filmcraft import → composed mask frame → browser canvas → time update → hide/reopen; library PNG binding → masked frame → explicit missing-binding error; no page errors')
} finally { await browser.close() }
