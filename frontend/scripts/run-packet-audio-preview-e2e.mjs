import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { execFileSync } from 'node:child_process'
import { readFileSync } from 'node:fs'
import { createServer } from 'vite'
import { chromium } from '@playwright/test'
const backend = process.env.NATIVE_AUDIO_BACKEND_URL
assert.ok(backend && process.env.NATIVE_AUDIO_TEST_TOKEN && process.env.NATIVE_AUDIO_TEST_SOURCE)
const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const server = await createServer({ root, plugins: [{ name: 'packet-audio-qa', resolveId(id) { if (id === '/audio-qa-runtime') return '\0audio-qa-runtime' }, load(id) { if (id === '\0audio-qa-runtime') return `export { mount, unmount } from 'svelte'` } }], server: { host: '127.0.0.1', port: 0, watch: null, hmr: false, proxy: { '/api': { target: backend, changeOrigin: true }, '/files': { target: backend, changeOrigin: true } } } })
let browser
try {
  await server.listen()
  browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) })
  const page = await browser.newPage()
  const errors = []
  page.on('pageerror', error => errors.push(error.message))
  await page.route('**/packet-audio-qa', route => route.fulfill({ contentType: 'text/html', body: '<button>Audio QA</button>' }))
  await page.goto(`http://127.0.0.1:${server.httpServer.address().port}/packet-audio-qa`)
  await page.getByRole('button', { name: 'Audio QA', exact: true }).click()
  await page.evaluate(async token => {
    const { mount, unmount } = await import('/audio-qa-runtime')
    const { default: Preview } = await import('/src/lib/components/composition/CompositionPreview.svelte')
    const { default: Chain } = await import('/src/lib/components/composition/AudioDspChain.svelte')
    const state = await import('/src/lib/state/composition.svelte.ts')
    const { authState } = await import('/src/lib/state/auth.svelte.ts')
    state.resetCompositionForTests(); authState.token = token
    const clip = state.addMediaInfoToComposition({ id: 'packet', url: '/files/sources/packet.wav', filename: 'packet.wav', mediaType: 'audio', duration: 3, width: 0, height: 0, acodec: 'pcm_f64le' })
    const previewTarget = document.createElement('div'), chainTarget = document.createElement('div')
    document.body.append(previewTarget, chainTarget)
    const preview = mount(Preview, { target: previewTarget })
    let chain
    const onchange = async effects => {
      window.__packetQa.effects = effects
      state.updateCompositionAudioMix(clip, { dspEffects: effects })
      await unmount(chain)
      chain = mount(Chain, { target: chainTarget, props: { effects, onchange } })
    }
    chain = mount(Chain, { target: chainTarget, props: { effects: [], onchange } })
    window.__packetQa = { state, effects: [], cleanup: async () => { await unmount(chain); await unmount(preview) } }
  }, process.env.NATIVE_AUDIO_TEST_TOKEN)
  const sweet = JSON.parse(readFileSync(resolve(root, '../backend/tests/fixtures/concat-sweet-plans.json'), 'utf8')).cases.find(c => c.amount === 75 && c.pitch === 0)
  assert.ok(sweet)
  const checks = []
  for (const [id, key, label, initial, value, pitch, suffix] of [
    ['concat.pitch', 'semitones', 'Semitones', 0, 7, 7, ''],
    ['concat.chipmunk', 'pitch', 'Pitch', 7, 6, 6, ',highpass=f=120,alimiter=limit=0.94'],
    ['concat.deep', 'body', 'Body', 3, 4, -3, ',highpass=f=55,equalizer=f=140:t=q:w=1.0:g=4.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60'],
    ['concat.sweet', 'amount', 'Amount', 65, 75, 4.05, ''],
    ['concat.loudness', 'target', 'Target', -23, -18, 0, ''],
  ]) {
    await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption(id)
    const control = page.getByLabel(`DSP 1 ${label}`, { exact: true })
    await control.waitFor()
    assert.equal(await control.inputValue(), String(initial))
    assert.equal(await control.getAttribute('step'), ['concat.sweet', 'concat.loudness'].includes(id) ? '1' : '0.5')
    assert.equal(await page.getByLabel(`Ключ DSP 1 ${label}`, { exact: true }).isDisabled(), true)
    await control.fill(String(value)); await control.press('Tab')
    await page.waitForFunction(({ key, value }) => window.__packetQa.effects[0]?.params?.[key] === value, { key, value })
    await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
    const responsePromise = page.waitForResponse(response => {
      if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
      const body = response.request().postDataJSON()
      return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.[0]?.id === id && c.dspEffects[0].params?.[key] === value)))
    })
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    const response = await responsePromise
    const pcm = await response.body()
    const ratio = 2 ** (pitch / 12)
    const filter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS,' + (id === 'concat.loudness' ? 'loudnorm=I=-18.0:TP=-1.5:LRA=11,aresample=48000:osf=flt' : id === 'concat.sweet' ? sweet.chain : `aresample=48000,asetrate=${(48000 * ratio).toFixed(6)},aresample=48000,atempo=${(1 / ratio).toFixed(8)}${suffix}`)
    const original = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', filter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
    const frames = response.request().postDataJSON().frames
    assert.ok(Number.isInteger(frames) && frames > 0 && frames <= 96000)
    assert.equal(pcm.length, frames * 8)
    let maxError = 0
    for (let i = 0; i < frames * 2; i++) {
      const error = Math.abs(pcm.readFloatLE(i * 4) - original.readFloatLE(i * 4))
      assert.ok(error <= 2e-7, `${id} browser sample ${i}: ${error}`)
      maxError = Math.max(maxError, error)
    }
    await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
    await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
    await page.getByLabel('Удалить DSP 1', { exact: true }).click()
    await page.waitForFunction(() => window.__packetQa.effects.length === 0)
    checks.push({ id, values: frames * 2, maxError })
  }
  // The actual editor reorders authored stages; request and PCM must follow that order.
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.pitch')
  await page.getByLabel('DSP 1 Semitones', { exact: true }).fill('7')
  await page.getByLabel('DSP 1 Semitones', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[0]?.params?.semitones === 7)
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.pitch')
  await page.getByLabel('DSP 2 Semitones', { exact: true }).fill('-7')
  await page.getByLabel('DSP 2 Semitones', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[1]?.params?.semitones === -7)
  await page.getByLabel('DSP 2 вверх', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects[0]?.params?.semitones === -7 && window.__packetQa.effects[1]?.params?.semitones === 7)
  await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
  const orderedResponsePromise = page.waitForResponse(response => {
    if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
    const body = response.request().postDataJSON()
    return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.length === 2 && c.dspEffects[0].params?.semitones === -7 && c.dspEffects[1].params?.semitones === 7)))
  })
  await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
  const orderedResponse = await orderedResponsePromise
  const orderedPcm = await orderedResponse.body()
  let orderedFilter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS'
  for (const pitch of [-7, 7]) {
    const ratio = 2 ** (pitch / 12)
    orderedFilter += `,aresample=48000,asetrate=${(48000 * ratio).toFixed(6)},aresample=48000,atempo=${(1 / ratio).toFixed(8)}`
  }
  const orderedOriginal = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', orderedFilter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
  assert.equal(orderedPcm.length, orderedResponse.request().postDataJSON().frames * 8)
  let orderedError = 0
  for (let i = 0; i < orderedPcm.length / 4; i++) {
    const error = Math.abs(orderedPcm.readFloatLE(i * 4) - orderedOriginal.readFloatLE(i * 4))
    assert.ok(error <= 2e-7, `reordered Pitch sample ${i}: ${error}`)
    orderedError = Math.max(orderedError, error)
  }
  await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
  await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
  checks.push({ id: 'pitch(-7) -> pitch(7)', values: orderedPcm.length / 4, maxError: orderedError })
  await page.getByLabel('Удалить DSP 2', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 1)
  await page.getByLabel('Удалить DSP 1', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 0)
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.chipmunk')
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.deep')
  await page.getByLabel('DSP 2 вверх', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects[0]?.id === 'concat.deep' && window.__packetQa.effects[1]?.id === 'concat.chipmunk')
  await page.getByLabel('DSP 1 Body', { exact: true }).fill('4')
  await page.getByLabel('DSP 1 Body', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[0]?.params?.body === 4)
  await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
  const voiceResponsePromise = page.waitForResponse(response => {
    if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
    const body = response.request().postDataJSON()
    return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.length === 2 && c.dspEffects[0].id === 'concat.deep' && c.dspEffects[0].params?.body === 4 && c.dspEffects[1].id === 'concat.chipmunk')))
  })
  await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
  const voiceResponse = await voiceResponsePromise
  const voicePcm = await voiceResponse.body()
  let voiceFilter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS'
  for (const [pitch, suffix] of [[-3, ',highpass=f=55,equalizer=f=140:t=q:w=1.0:g=4.00,equalizer=f=2600:t=q:w=0.9:g=-1.2,alimiter=limit=0.94:attack=5:release=60'], [7, ',highpass=f=120,alimiter=limit=0.94']]) {
    const ratio = 2 ** (pitch / 12)
    voiceFilter += `,aresample=48000,asetrate=${(48000 * ratio).toFixed(6)},aresample=48000,atempo=${(1 / ratio).toFixed(8)}${suffix}`
  }
  const voiceOriginal = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', voiceFilter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
  assert.equal(voicePcm.length, voiceResponse.request().postDataJSON().frames * 8)
  let voiceError = 0
  for (let i = 0; i < voicePcm.length / 4; i++) {
    const error = Math.abs(voicePcm.readFloatLE(i * 4) - voiceOriginal.readFloatLE(i * 4))
    assert.ok(error <= 2e-7, `Deep -> Chipmunk sample ${i}: ${error}`)
    voiceError = Math.max(voiceError, error)
  }
  await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
  await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
  checks.push({ id: 'deep -> chipmunk', values: voicePcm.length / 4, maxError: voiceError })
  await page.getByLabel('Удалить DSP 2', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 1)
  await page.getByLabel('Удалить DSP 1', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 0)
  for (const [index, target] of [[1, -30], [2, -12]]) {
    await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.loudness')
    const control = page.getByLabel(`DSP ${index} Target`, { exact: true })
    await control.fill(String(target)); await control.press('Tab')
    await page.waitForFunction(({ index, target }) => window.__packetQa.effects[index - 1]?.params?.target === target, { index, target })
  }
  await page.getByLabel('DSP 2 вверх', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects[0]?.params?.target === -12 && window.__packetQa.effects[1]?.params?.target === -30)
  await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
  const loudnessPromise = page.waitForResponse(response => {
    if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
    const body = response.request().postDataJSON()
    return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.length === 2 && c.dspEffects.every(e => e.id === 'concat.loudness') && c.dspEffects[0].params?.target === -12 && c.dspEffects[1].params?.target === -30)))
  })
  await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
  const loudnessResponse = await loudnessPromise
  const loudnessPcm = await loudnessResponse.body()
  const loudnessFilter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS,loudnorm=I=-12.0:TP=-1.5:LRA=11,loudnorm=I=-30.0:TP=-1.5:LRA=11,aresample=48000:osf=flt'
  const loudnessOriginal = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', loudnessFilter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
  assert.equal(loudnessPcm.length, loudnessResponse.request().postDataJSON().frames * 8)
  let loudnessError = 0
  for (let i = 0; i < loudnessPcm.length / 4; i++) {
    const error = Math.abs(loudnessPcm.readFloatLE(i * 4) - loudnessOriginal.readFloatLE(i * 4))
    assert.ok(error <= 2e-7, `reordered Loudness sample ${i}: ${error}`)
    loudnessError = Math.max(loudnessError, error)
  }
  await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
  await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
  checks.push({ id: 'loudness(-12) -> loudness(-30)', values: loudnessPcm.length / 4, maxError: loudnessError })
  await page.getByLabel('Удалить DSP 2', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 1)
  await page.getByLabel('Удалить DSP 1', { exact: true }).click()
  await page.waitForFunction(() => window.__packetQa.effects.length === 0)
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.loudness')
  await page.getByLabel('DSP 1 Target', { exact: true }).fill('-18')
  await page.getByLabel('DSP 1 Target', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[0]?.params?.target === -18)
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.pitch')
  await page.getByLabel('DSP 2 Semitones', { exact: true }).fill('7')
  await page.getByLabel('DSP 2 Semitones', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[1]?.params?.semitones === 7)
  for (const reversed of [false, true]) {
    if (reversed) {
      await page.getByLabel('DSP 2 вверх', { exact: true }).click()
      await page.waitForFunction(() => window.__packetQa.effects[0]?.id === 'concat.pitch' && window.__packetQa.effects[1]?.id === 'concat.loudness')
    }
    await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
    const mixedPromise = page.waitForResponse(response => {
      if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
      const body = response.request().postDataJSON()
      const ids = reversed ? ['concat.pitch', 'concat.loudness'] : ['concat.loudness', 'concat.pitch']
      return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.length === 2 && c.dspEffects.every((e, i) => e.id === ids[i]) && c.dspEffects.find(e => e.id === 'concat.pitch')?.params?.semitones === 7 && c.dspEffects.find(e => e.id === 'concat.loudness')?.params?.target === -18)))
    })
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    const response = await mixedPromise
    const pcm = await response.body()
    const ratio = 2 ** (7 / 12)
    const pitch = `aresample=48000,asetrate=${(48000 * ratio).toFixed(6)},aresample=48000,atempo=${(1 / ratio).toFixed(8)}`
    const loudness = 'loudnorm=I=-18.0:TP=-1.5:LRA=11'
    const filter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS,' + (reversed ? `${pitch},${loudness}` : `${loudness},${pitch}`) + ',aresample=48000:osf=flt'
    const original = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', filter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
    assert.equal(pcm.length, response.request().postDataJSON().frames * 8)
    let maxError = 0
    for (let i = 0; i < pcm.length / 4; i++) {
      const error = Math.abs(pcm.readFloatLE(i * 4) - original.readFloatLE(i * 4))
      assert.ok(error <= 2e-7, `mixed Pitch/Loudness order${reversed} sample${i}: ${error}`)
      maxError = Math.max(maxError, error)
    }
    await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
    await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
    checks.push({ id: reversed ? 'pitch -> loudness' : 'loudness -> pitch', values: pcm.length / 4, maxError })
  }
  await page.getByLabel('Добавить DSP эффект', { exact: true }).selectOption('concat.pitch')
  await page.getByLabel('DSP 3 Semitones', { exact: true }).fill('-3')
  await page.getByLabel('DSP 3 Semitones', { exact: true }).press('Tab')
  await page.waitForFunction(() => window.__packetQa.effects[2]?.params?.semitones === -3)
  for (const reordered of [false, true]) {
    if (reordered) {
      await page.getByLabel('DSP 3 вверх', { exact: true }).click()
      await page.waitForFunction(() => window.__packetQa.effects[1]?.id === 'concat.pitch' && window.__packetQa.effects[1]?.params?.semitones === -3 && window.__packetQa.effects[2]?.id === 'concat.loudness')
    }
    await page.evaluate(() => window.__packetQa.state.setCompositionPlayhead(0))
    const responsePromise = page.waitForResponse(response => {
      if (!response.url().endsWith('/api/compositions/audio-preview') || response.status() !== 200) return false
      const body = response.request().postDataJSON()
      const ids = reordered ? ['concat.pitch', 'concat.pitch', 'concat.loudness'] : ['concat.pitch', 'concat.loudness', 'concat.pitch']
      return body.startSample === 0 && Object.values(body.project.compositions).some(doc => doc.content.tracks.some(t => t.kind === 'audio' && t.clips.some(c => c.dspEffects?.length === 3 && c.dspEffects.every((e, i) => e.id === ids[i]) && c.dspEffects[0].params?.semitones === 7 && c.dspEffects[reordered ? 1 : 2].params?.semitones === -3 && c.dspEffects[reordered ? 2 : 1].params?.target === -18)))
    })
    await page.getByRole('button', { name: '▶ Играть', exact: true }).click()
    const response = await responsePromise
    const pcm = await response.body()
    const pitch = value => {
      const ratio = 2 ** (value / 12)
      return `aresample=48000,asetrate=${(48000 * ratio).toFixed(6)},aresample=48000,atempo=${(1 / ratio).toFixed(8)}`
    }
    const loudness = 'loudnorm=I=-18.0:TP=-1.5:LRA=11'
    const stages = reordered ? [pitch(7), pitch(-3), loudness] : [pitch(7), loudness, pitch(-3)]
    const filter = 'atrim=start_sample=0:end_sample=132300,asetpts=PTS-STARTPTS,' + stages.join(',') + ',aresample=48000:osf=flt'
    const original = execFileSync('ffmpeg', ['-nostdin', '-v', 'error', '-i', process.env.NATIVE_AUDIO_TEST_SOURCE, '-af', filter, '-c:a', 'pcm_f32le', '-f', 'f32le', 'pipe:1'], { maxBuffer: 4 * 1024 * 1024 })
    assert.equal(pcm.length, response.request().postDataJSON().frames * 8)
    let maxError = 0
    for (let i = 0; i < pcm.length / 4; i++) {
      const error = Math.abs(pcm.readFloatLE(i * 4) - original.readFloatLE(i * 4))
      assert.ok(error <= 2e-7, `three-stage reordered${reordered} sample${i}: ${error}`)
      maxError = Math.max(maxError, error)
    }
    await page.waitForFunction(() => window.__packetQa.state.compositionState.transport.playheadTicks > 100000)
    await page.getByRole('button', { name: '⏸ Пауза', exact: true }).click()
    checks.push({ id: reordered ? 'pitch(7) -> pitch(-3) -> loudness' : 'pitch(7) -> loudness -> pitch(-3)', values: pcm.length / 4, maxError })
  }
  await page.evaluate(() => window.__packetQa.cleanup())
  assert.deepEqual(errors, [])
  console.log(JSON.stringify({ packetAudio: checks }))
} finally {
  await browser?.close()
  await server.close()
}
