import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { readFileSync } from 'node:fs'
import { gunzipSync } from 'node:zlib'
import { chromium } from '@playwright/test'
// Optional real backend mode is owned by the ignored composition_audio_preview
// Rust integration test, with an ephemeral private source/session and HTTP server.
const backendUrl = process.env.NATIVE_AUDIO_BACKEND_URL
const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const server = await createServer({ root, plugins: [{ name: 'audio-qa-runtime', resolveId(id) { if (id === '/audio-qa-runtime') return '\0audio-qa-runtime' }, load(id) { if (id === '\0audio-qa-runtime') return `export { mount, unmount } from 'svelte'` } }], server: { host: '127.0.0.1', port: 0, watch: null, hmr: false, ...(backendUrl ? { proxy: { '/api': { target: backendUrl, changeOrigin: true } } } : {}) } })
let browser
try {
  await server.listen()
  browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) })
  const page = await browser.newPage()
  await page.route('**/native-audio-qa', route => route.fulfill({ contentType: 'text/html', body: '<button>Audio QA</button>' }))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/native-audio-qa`)
  await page.getByRole('button').click()
  const result = await page.evaluate(async () => {
    const { NativeAudioPreview } = await import('/src/lib/audio/nativePreview.ts')
    const { readAudioMasterMeter, setPreviewMasterMuted } = await import('/src/lib/audio/mediaGain.ts')
    const sleep = ms => new Promise(resolve => setTimeout(resolve, ms))
    const requests = []
    const fetch = async (_project, start, frames, _token, signal) => {
      requests.push({ start, frames, signal })
      const pcm = new Float32Array(frames * 2)
      for (let i = 0; i < frames; i++) for (let ch = 0; ch < 2; ch++) pcm[i * 2 + ch] = 0.1 * Math.sin(2 * Math.PI * (ch ? 700 : 440) * (start + i) / 48000)
      return pcm
    }
    const engine = new NativeAudioPreview(fetch)
    const project = { schemaVersion: 1, root: 'editor', compositions: {} }
    try {
      engine.start(project, 0, 8e6)
      await sleep(500)
      const playing = { ready: engine.ready, time: engine.timeUs(), meter: readAudioMasterMeter(), count: requests.length }
      setPreviewMasterMuted(true); await sleep(150)
      const muted = readAudioMasterMeter()
      setPreviewMasterMuted(false); await sleep(150)
      const unmuted = readAudioMasterMeter()
      engine.start(project, 3e6, 8e6); await sleep(300)
      const seek = { time: engine.timeUs(), firstAborted: requests[0].signal.aborted, start: requests[2].start }
      engine.stop(); await sleep(150)
      const stopped = { ready: engine.ready, meter: readAudioMasterMeter(), aborted: requests.at(-1).signal.aborted }
      return { playing, muted, unmuted, seek, stopped }
    } finally { engine.stop(); setPreviewMasterMuted(false) }
  })
  assert.equal(result.playing.ready, true)
  assert.equal(result.playing.count, 2)
  assert.ok(result.playing.time > 100000 && result.playing.time < 1000000, JSON.stringify(result))
  assert.ok(result.playing.meter.rmsDb > -35, JSON.stringify(result))
  assert.ok(result.muted.peakDb < -80, JSON.stringify(result))
  assert.ok(result.unmuted.rmsDb > -35, JSON.stringify(result))
  assert.equal(result.seek.firstAborted, true)
  assert.equal(result.seek.start, 144000)
  assert.ok(result.seek.time > 3100000 && result.seek.time < 3800000, JSON.stringify(result))
  assert.equal(result.stopped.ready, false)
  assert.equal(result.stopped.aborted, true)
  assert.ok(result.stopped.meter.peakDb < -80, JSON.stringify(result))
  const requests = []
  page.on('request', request => { if (request.url().endsWith('/api/compositions/audio-preview')) requests.push(request.postDataJSON()) })
  const originalResponses = []
  page.on('response', response => {
    if (!backendUrl || !response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return
    const request = response.request().postDataJSON()
    if (request.startSample === 0 && request.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.params.delay === 0.25)) originalResponses.push(response.body())
  })
  if (!backendUrl) await page.route('**/api/compositions/audio-preview', async route => {
    const body = route.request().postDataJSON()
    const pcm = Buffer.alloc(body.frames * 8)
    for (let i = 0; i < body.frames; i++) for (let ch = 0; ch < 2; ch++) pcm.writeFloatLE(0.08 * Math.sin(2 * Math.PI * 440 * (body.startSample + i) / 48000), (i * 2 + ch) * 4)
    await route.fulfill({ contentType: 'application/octet-stream', headers: {
      'x-audio-rate': '48000', 'x-audio-channels': '2', 'x-audio-frames': String(body.frames), 'x-audio-start-sample': String(body.startSample),
    }, body: pcm })
  })
  await page.evaluate(async config => {
    const { mount, unmount } = await import('/audio-qa-runtime')
    const { default: Preview } = await import('/src/lib/components/composition/CompositionPreview.svelte')
    const state = await import('/src/lib/state/composition.svelte.ts')
    state.resetCompositionForTests()
    const { authState } = await import('/src/lib/state/auth.svelte.ts')
    authState.token = config.token ?? null
    const clip = state.addMediaInfoToComposition({ id: config.real ? 'echo' : 'qa', url: '/files/sources/echo.wav', filename: 'echo.wav', mediaType: 'audio', duration: 8, width: 0, height: 0, acodec: 'pcm_f32le' })
    state.updateCompositionAudioMix(clip, { dspEffects: [{ id: 'concat.echo', params: { delay: 0.25 } }] })
    const target = document.createElement('div'); document.body.append(target)
    const component = mount(Preview, { target })
    window.__audioQa = { state, clip, authState, cleanup: () => unmount(component) }
  }, { real: !!backendUrl, token: process.env.NATIVE_AUDIO_TEST_TOKEN })
  await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
  await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 200000)
  assert.equal(await page.locator('audio').count(), 0)
  assert.equal(requests.length, 2)
  const seekRequest = page.waitForRequest(request => request.url().endsWith('/api/compositions/audio-preview') && request.postDataJSON().startSample === 48000)
  await page.evaluate(() => window.__audioQa.state.setCompositionPlayhead(1000000))
  await seekRequest
  await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 1100000)
  const editRequest = page.waitForRequest(request => request.url().endsWith('/api/compositions/audio-preview') && request.postDataJSON().project.compositions.editor.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.[0]?.params.delay === 0.5)))
  await page.evaluate(() => window.__audioQa.state.updateCompositionAudioMix(window.__audioQa.clip, { dspEffects: [{ id: 'concat.echo', params: { delay: 0.5 } }] }))
  await editRequest
  await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
  const paused = await page.evaluate(() => window.__audioQa.state.compositionState.transport.playheadTicks)
  await page.waitForTimeout(180)
  assert.equal(await page.evaluate(() => window.__audioQa.state.compositionState.transport.playheadTicks), paused)
  await page.evaluate(() => window.__audioQa.state.updateCompositionAudioMix(window.__audioQa.clip, { dspEffects: [{ id: 'concat.echo', bypass: true }] }))
  await page.waitForFunction(() => !!document.querySelector('audio'))
  let originalValues = 0
  let originalMaxError = 0
  let distortedValues = 0, distortedMaxError = 0
  const tones = []
  const voices = []
  const denoise = []
  const enhance = []
  const crunch = []
  if (backendUrl) {
    assert.equal(originalResponses.length, 1)
    const response = await originalResponses[0]
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-echo-pcm.json'), 'utf8'))
    const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2)
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-echo-pcm.f32le.gz')))
    assert.ok(response.length >= 48000 * 8)
    for (let i = 0; i < 48000 * 2; i++) {
      const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
      assert.ok(error <= 2e-7, `browser PCM value ${i}: error ${error}`)
      originalMaxError = Math.max(originalMaxError, error); originalValues++
    }
    // Missing and outsider sessions must stop the actual editor without dry fallback.
    for (const [token, status] of [[null, 401], [process.env.NATIVE_AUDIO_OUTSIDER_TOKEN, 403]]) {
      await page.evaluate(token => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id: 'concat.echo', params: { delay: 0.5 } }] })
        qa.authState.token = token
      }, token)
      const denied = page.waitForResponse(response => response.url().endsWith('/api/compositions/audio-preview') && response.status() === status)
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      await denied
      await page.waitForFunction(() => !window.__audioQa.state.compositionState.transport.playing && document.querySelector('[role="alert"]')?.textContent.includes('Аудиопредпросмотр'))
      assert.equal(await page.locator('audio').count(), 0)
    }
  }
  if (backendUrl) {
    await page.evaluate(token => {
      const qa = window.__audioQa
      qa.authState.token = token
      qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id: 'concat.distorted', params: { drive: 50 } }] })
      qa.state.setCompositionPlayhead(0)
    }, process.env.NATIVE_AUDIO_TEST_TOKEN)
    const nativeDistorted = page.waitForResponse(response => {
      if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
      const body = response.request().postDataJSON()
      return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === 'concat.distorted')
    })
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    const response = await (await nativeDistorted).body()
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-distorted-pcm.json'), 'utf8'))
    const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings[0].params.drive === 50)
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-distorted-pcm.f32le.gz')))
    for (let i = 0; i < 96000; i++) {
      const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
      assert.ok(error <= 2e-7, `Distorted browser PCM value ${i}: error ${error}`)
      distortedValues++; distortedMaxError = Math.max(distortedMaxError, error)
    }
    await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
    await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
  }
  if (backendUrl) {
    for (const [id, gain] of [['concat.bass', 5], ['concat.treble', 4], ['concat.bright', 5]]) {
      await page.evaluate(({ id, gain }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { gain } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, gain })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-tones-pcm.json'), 'utf8'))
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params.gain === gain)
      const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-tones-pcm.f32le.gz')))
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      tones.push({ id, values: 96000, maxError })
    }
  }
  if (backendUrl) {
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-voices-pcm.json'), 'utf8'))
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-voices-pcm.f32le.gz')))
    for (const [id, key, value] of [['concat.underwater','depth',5],['concat.telephone','drive',3],['concat.megaphone','drive',5],['concat.vinyl','warmth',5],['concat.whisper','air',5]]) {
      await page.evaluate(({ id, key, value }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { [key]: value } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, key, value })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params[key] === value)
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      voices.push({ id, values: 96000, maxError })
    }
  }
  if (backendUrl) {
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-crunch-pcm.json'), 'utf8'))
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-crunch-pcm.f32le.gz')))
    for (const [id, key, value] of [['concat.radio','noise',30],['concat.lofi','bits',8]]) {
      await page.evaluate(({ id, key, value }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { [key]: value } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, key, value })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params[key] === value)
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      crunch.push({ id, values: 96000, maxError })
    }
  }
  if (backendUrl) {
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-robot-pcm.json'), 'utf8'))
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-robot-pcm.f32le.gz')))
    for (const [id, key, value] of [['concat.robot','depth',5]]) {
      await page.evaluate(({ id, key, value }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { [key]: value } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, key, value })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params[key] === value)
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      crunch.push({ id, values: 96000, maxError })
    }
  }
  if (backendUrl) {
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-enhance-pcm.json'), 'utf8'))
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-enhance-pcm.f32le.gz')))
    for (const [id, key, value] of [['concat.enhance-voice','amount',60]]) {
      await page.evaluate(({ id, key, value }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { [key]: value } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, key, value })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params[key] === value)
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      enhance.push({ id, values: 96000, maxError })
    }
  }
  if (backendUrl) {
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-denoise-pcm.json'), 'utf8'))
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-denoise-pcm.f32le.gz')))
    for (const [id, key, value] of [['concat.denoise','strength',50]]) {
      await page.evaluate(({ id, key, value }) => {
        const qa = window.__audioQa
        qa.state.updateCompositionAudioMix(qa.clip, { dspEffects: [{ id, params: { [key]: value } }] })
        qa.state.setCompositionPlayhead(0)
      }, { id, key, value })
      const pcmResponse = page.waitForResponse(response => {
        if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
        const body = response.request().postDataJSON()
        return body.startSample === 0 && body.project.compositions.editor?.content.tracks.some(t => t.kind === 'audio' && t.clips[0]?.dspEffects?.[0]?.id === id)
      })
      await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
      const response = await (await pcmResponse).body()
      const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2 && c.signal === 'noise' && c.settings.length === 1 && c.settings[0].id === id && c.settings[0].params[key] === value)
      let maxError = 0
      for (let i = 0; i < 96000; i++) {
        const error = Math.abs(response.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
        assert.ok(error <= 2e-7, `${id} browser PCM value ${i}: error ${error}`)
        maxError = Math.max(maxError, error)
      }
      await page.waitForFunction(() => window.__audioQa.state.compositionState.transport.playheadTicks > 100000)
      await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
      denoise.push({ id, values: 96000, maxError })
    }
  }
  await page.evaluate(() => window.__audioQa.cleanup())
  let nested = null
  if (backendUrl) {
    await page.evaluate(async token => {
      const { mount, unmount } = await import('/audio-qa-runtime')
      const { default: Controls } = await import('/src/lib/components/composition/fixtures/NestedPlaybackHarness.svelte')
      const state = await import('/src/lib/state/nestedComposition.svelte.ts')
      const { buildCompositionWireDocument } = await import('/src/lib/composition/payload.ts')
      const flat = window.__audioQa
      flat.state.updateCompositionAudioMix(flat.clip, { dspEffects: [{ id: 'concat.echo', params: { delay: 0.25 } }] })
      flat.state.setCompositionPlayhead(0)
      flat.state.updateCompositionCanvas({ width: 64, height: 36 })
      const color = flat.state.addColorToComposition()
      flat.state.updateCompositionColorParams(color, { color: '#ff0000' })
      const content = buildCompositionWireDocument(flat.state.compositionState.document)
      state.closeNestedComposition()
      const constant = value => ({ mode: 'constant', value })
      state.nestedComposition.document = { schemaVersion: 1, root: 'root', compositions: {
        root: { durationUs: 4000000, content: { ...content, tracks: [] }, instances: [{ id: 'placed-child',
          timing: { composition: 'child', timelineStartUs: 1000000, sourceInUs: 0, sourceOutUs: 3000000 },
          transform: { x: constant(0), y: constant(0), scaleX: constant(1), scaleY: constant(1), rotationDegrees: constant(0), anchorX: 0.5, anchorY: 0.5 },
          opacity: constant(1), blendMode: 'normal' }] },
        child: { durationUs: 8000000, content, instances: [] },
      } }
      state.nestedComposition.activeCompositionId = 'root'
      const target = document.createElement('div'); document.body.append(target)
      const component = mount(Controls, { target, props: { token } })
      window.__nestedAudioQa = { state, cleanup: () => unmount(component) }
    }, process.env.NATIVE_AUDIO_TEST_TOKEN)
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    await page.waitForFunction(() => window.__nestedAudioQa.state.nestedComposition.timeUs > 200000)
    const scoped = page.waitForResponse(response => {
      if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
      const body = response.request().postDataJSON()
      return body.compositionId === 'root' && body.startSample === 48000
    })
    await page.evaluate(() => { window.__nestedAudioQa.state.nestedComposition.timeUs = 1000000 })
    const placedPcm = await (await scoped).body()
    await page.waitForFunction(() => {
      const canvas = document.querySelector('canvas')
      if (!canvas || canvas.width !== 64 || canvas.height !== 36) return false
      return canvas.getContext('2d').getImageData(32, 18, 1, 1).data[0] > 240
    })
    const manifest = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-echo-pcm.json'), 'utf8'))
    const fixture = manifest.cases.find(c => c.rate === 48000 && c.channels === 2)
    const oracle = gunzipSync(readFileSync(resolve(root, '../backend/tests/fixtures/concat-echo-pcm.f32le.gz')))
    let maxError = 0
    for (let i = 0; i < 96000; i++) {
      const error = Math.abs(placedPcm.readFloatLE(i * 4) - oracle.readFloatLE(fixture.offsetBytes + i * 4))
      assert.ok(error <= 2e-7, `nested PCM value ${i}: error ${error}`); maxError = Math.max(maxError, error)
    }
    await page.evaluate(() => window.__nestedAudioQa.state.navigateNestedComposition('child'))
    await page.waitForFunction(() => !window.__nestedAudioQa.state.nestedPlayback.playing)
    assert.equal(await page.evaluate(() => window.__nestedAudioQa.state.nestedComposition.timeUs), 0)
    const focused = page.waitForResponse(response => response.url().endsWith('/api/compositions/audio-preview') && response.status() === 200 && response.request().postDataJSON().compositionId === 'child')
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    await focused
    await page.waitForFunction(() => window.__nestedAudioQa.state.nestedComposition.timeUs > 100000)
    await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
    const paused = await page.evaluate(() => window.__nestedAudioQa.state.nestedComposition.timeUs)
    await page.waitForTimeout(150)
    assert.equal(await page.evaluate(() => window.__nestedAudioQa.state.nestedComposition.timeUs), paused)
    await page.evaluate(() => window.__nestedAudioQa.cleanup())
    nested = { realNativeFrames: true, parentPlacement: true, originalValues: 96000, originalMaxError: maxError, focusedChild: true, navigationPause: true, pause: true }
  }
  console.log(JSON.stringify({ qualified: true, ...result, nested, editor: { denoise, enhance, crunch, voices, tones, distortedValues, distortedMaxError, realBackend: !!backendUrl, originalValues, originalMaxError, networkRequests: requests.length, seek: true, edit: true, pause: true, drySuppression: true, bypassRestoresMedia: true } }, null, 2))
} finally { await browser?.close(); await server.close() }
