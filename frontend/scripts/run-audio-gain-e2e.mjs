import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createServer } from 'vite'
import { chromium } from '@playwright/test'
const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const server = await createServer({ root, server: { host: '127.0.0.1', port: 0, watch: null, hmr: false } })
let browser
try {
  await server.listen()
  const port = server.httpServer.address().port
  browser = await chromium.launch({ headless: true, ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH } : {}) })
  const page = await browser.newPage()
  await page.route('**/gain-qa', route => route.fulfill({ contentType: 'text/html', body: '<button>Audio QA</button>' }))
  await page.goto(`http://127.0.0.1:${port}/gain-qa`)
  await page.getByRole('button').click()
  const samples = await page.evaluate(async () => {
    const { createMediaGain, readAudioRoleMeter, readAudioMasterMeter, setPreviewMasterMuted, readMasterPeakHold, resetMasterPeakHold } = await import('/src/lib/audio/mediaGain.ts')
    const { audioRoleGain } = await import('/src/lib/composition/audioRoles.ts')
    const context = new AudioContext()
    const analyser = context.createAnalyser()
    analyser.fftSize = 4096
    analyser.connect(context.destination)
    const preCompressorTap = context.createAnalyser()
    preCompressorTap.fftSize = 4096
    let tapCount = 0
    const channelTaps = [context.createAnalyser(), context.createAnalyser()]
    channelTaps.forEach(tap => { tap.fftSize = 4096 })
    const splitter = context.createChannelSplitter(2)
    splitter.connect(channelTaps[0], 0)
    splitter.connect(channelTaps[1], 1)
    let panCount = 0
    let observedPan
    const wrapped = {
      createMediaElementSource: context.createMediaElementSource.bind(context),
      createGain: context.createGain.bind(context),
      createStereoPanner: () => {
        const pan = context.createStereoPanner()
        if (panCount++ === 0) observedPan = pan
        return pan
      },
      destination: analyser,
      get currentTime() { return context.currentTime },
      get state() { return context.state },
      resume: context.resume.bind(context),
      createAnalyser: () => {
        const tap = context.createAnalyser()
        if (tapCount++ === 0) tap.connect(preCompressorTap)
        return tap
      },
      createDynamicsCompressor: context.createDynamicsCompressor.bind(context),
    }
    const urls = []
    function wav(amplitude, stereo = false) {
      const rate = 48000, length = rate * 2, channels = stereo ? 2 : 1
      const bytes = new ArrayBuffer(44 + length * 2 * channels), view = new DataView(bytes)
      const text = (at, value) => [...value].forEach((char, index) => view.setUint8(at + index, char.charCodeAt(0)))
      text(0, 'RIFF'); view.setUint32(4, 36 + length * 2 * channels, true); text(8, 'WAVE'); text(12, 'fmt ')
      view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, channels, true)
      view.setUint32(24, rate, true); view.setUint32(28, rate * 2 * channels, true); view.setUint16(32, 2 * channels, true); view.setUint16(34, 16, true)
      text(36, 'data'); view.setUint32(40, length * 2 * channels, true)
      for (let index = 0; index < length; index++) for (let channel = 0; channel < channels; channel++) view.setInt16(44 + (index * channels + channel) * 2, Math.round(Math.sin(2 * Math.PI * (channel === 0 ? 1000 : 2000) * index / rate) * amplitude * 32767), true)
      const url = URL.createObjectURL(new Blob([bytes], { type: 'audio/wav' }))
      urls.push(url)
      return url
    }
    const audio = document.createElement('audio')
    audio.loop = true
    audio.src = wav(0.02)
    document.body.append(audio)
    const gain = createMediaGain(audio, wrapped, 'music')
    const pause = ms => new Promise(done => setTimeout(done, ms))
    async function rms() {
      await pause(180)
      return readRms(analyser)
    }
    function readRms(tap) {
      const values = new Float32Array(tap.fftSize)
      tap.getFloatTimeDomainData(values)
      return Math.sqrt(values.reduce((sum, value) => sum + value * value, 0) / values.length)
    }
    try {
      gain.resume()
      await pause(50)
      if (context.state !== 'running') throw new Error(`Audio context did not resume: ${context.state}`)
      gain.setGain(1)
      await audio.play()
      await pause(600)
      const unity = await rms()
      gain.setGain(4)
      const boosted = await rms()
      gain.setGain(0)
      const muted = await rms()
      gain.setGain(audioRoleGain({ audioRoles: { music: { gainDb: 6, muted: false, solo: false } } }, 'music'))
      const roleBoosted = await rms()
      const masterMeter = readAudioMasterMeter(wrapped)
      const rolePreRms = readRms(preCompressorTap)
      const musicMeter = readAudioRoleMeter('music', wrapped)
      setPreviewMasterMuted(true, wrapped)
      const masterMutedRms = await rms()
      const masterMutedMeterSilent = readAudioMasterMeter(wrapped).rmsDb === -Infinity
      const roleStillActive = Number.isFinite(readAudioRoleMeter('music', wrapped).rmsDb)
      setPreviewMasterMuted(false, wrapped)
      const masterRestoredRms = await rms()
      gain.setRole('sfx')
      const rerouted = await rms()
      const reroutedPreRms = readRms(preCompressorTap)
      const sfxMeter = readAudioRoleMeter('sfx', wrapped)
      const releasedMusic = readAudioRoleMeter('music', wrapped).rmsDb === -Infinity
      const secondAudio = document.createElement('audio')
      secondAudio.loop = true
      secondAudio.src = wav(0.02)
      const secondGain = createMediaGain(secondAudio, wrapped, 'sfx')
      let mixedRms, mixedMeter, mixedPreRms
      try {
        secondGain.setGain(2)
        await secondAudio.play()
        await pause(600)
        mixedRms = await rms()
        mixedMeter = readAudioRoleMeter('sfx', wrapped)
        mixedPreRms = readRms(preCompressorTap)
      } finally { secondGain.dispose(); secondAudio.pause(); secondAudio.remove() }
      gain.setGain(audioRoleGain({ audioRoles: { voiceover: { gainDb: 0, muted: false, solo: true } } }, 'music'))
      const roleSoloBlocked = await rms()
      const gatedMeterSilent = readAudioRoleMeter('sfx', wrapped).rmsDb === -Infinity
      gain.setGain(audioRoleGain({ audioRoles: { music: { gainDb: 6, muted: true, solo: true } } }, 'music'))
      const roleMutedSolo = await rms()
      audio.src = wav(0.01)
      gain.setGain(4)
      await audio.play()
      const replaced = await rms()
      observedPan.connect(splitter)
      gain.setPan(-1)
      await pause(200)
      const leftPan = channelTaps.map(readRms)
      gain.setPan(1)
      await pause(200)
      const rightPan = channelTaps.map(readRms)
      gain.setPan(0.5)
      await pause(200)
      const monoHalfPan = channelTaps.map(readRms)
      audio.src = wav(0.01, true)
      await audio.play()
      await pause(600)
      gain.setPan(0)
      await pause(200)
      const stereoCenter = channelTaps.map(readRms)
      gain.setPan(-1)
      await pause(200)
      const stereoLeft = channelTaps.map(readRms)
      gain.setPan(1)
      await pause(200)
      const stereoRight = channelTaps.map(readRms)
      gain.setPan(0)
      gain.setGain(800)
      const overloaded = await rms()
      const overloadMeter = readAudioMasterMeter(wrapped)
      const heldOverload = readMasterPeakHold(wrapped)
      gain.setGain(0)
      await rms()
      readAudioMasterMeter(wrapped)
      const heldAfterSilence = readMasterPeakHold(wrapped)
      resetMasterPeakHold(wrapped)
      const resetHoldSilent = readMasterPeakHold(wrapped) === -Infinity
      gain.dispose()
      const disposed = await rms()
      const disposedMeterSilent = readAudioRoleMeter('sfx', wrapped).rmsDb === -Infinity && readAudioMasterMeter(wrapped).rmsDb === -Infinity
      return { monoHalfPan, stereoCenter, stereoLeft, stereoRight, leftPan, rightPan, heldOverload, heldAfterSilence, resetHoldSilent, rolePreRms, reroutedPreRms, mixedPreRms, overloaded, overloadMeter, masterMutedRms, masterMutedMeterSilent, roleStillActive, masterRestoredRms, masterMeter, mixedRms, mixedMeter, musicMeter, sfxMeter, rerouted, releasedMusic, gatedMeterSilent, disposedMeterSilent, unity, boosted, muted, roleBoosted, roleSoloBlocked, roleMutedSolo, replaced, disposed, elementVolume: audio.volume }
    } finally {
      gain.dispose(); audio.pause(); audio.remove(); urls.forEach(url => URL.revokeObjectURL(url)); analyser.disconnect(); preCompressorTap.disconnect(); splitter.disconnect(); channelTaps.forEach(tap => tap.disconnect()); await context.close()
    }
  })
  assert.ok(samples.unity > 0.01 && samples.unity < 0.02, JSON.stringify(samples))
  assert.ok(Math.abs(samples.boosted / samples.unity - 4) < 0.1, JSON.stringify(samples))
  assert.ok(samples.muted < 1e-7, JSON.stringify(samples))
  assert.ok(Math.abs(samples.replaced / samples.unity - 2) < 0.1, JSON.stringify(samples))
  assert.ok(samples.disposed < 1e-7, JSON.stringify(samples))
  assert.ok(Math.abs(samples.roleBoosted / samples.unity - 10 ** (6 / 20)) < 0.06, JSON.stringify(samples))
  assert.ok(samples.roleSoloBlocked < 1e-7, JSON.stringify(samples))
  assert.ok(samples.roleMutedSolo < 1e-7, JSON.stringify(samples))
  assert.ok(Math.abs(samples.mixedMeter.rmsDb - 20 * Math.log10(samples.mixedPreRms)) < 0.1, JSON.stringify(samples))
  assert.ok(Math.abs(samples.masterMeter.rmsDb - 20 * Math.log10(samples.rolePreRms)) < 0.1, JSON.stringify(samples))
  assert.ok(Math.abs(samples.masterMeter.peakDb - samples.musicMeter.peakDb) < 0.1, JSON.stringify(samples))
  assert.ok(Math.abs(samples.musicMeter.rmsDb - 20 * Math.log10(samples.rolePreRms)) < 0.1, JSON.stringify(samples))
  assert.ok(Math.abs(samples.sfxMeter.rmsDb - 20 * Math.log10(samples.reroutedPreRms)) < 0.1, JSON.stringify(samples))
  assert.ok(samples.releasedMusic && samples.gatedMeterSilent && samples.disposedMeterSilent, JSON.stringify(samples))
  assert.ok(samples.masterMutedRms < 1e-7 && samples.masterMutedMeterSilent && samples.roleStillActive, JSON.stringify(samples))
  assert.ok(Math.abs(samples.masterRestoredRms / samples.roleBoosted - 1) < 0.02, JSON.stringify(samples))
  assert.ok(samples.overloadMeter.peakDb > 10, JSON.stringify(samples))
  assert.ok(samples.overloaded > 0.01 && samples.overloaded < 1 && samples.overloaded < 10 ** (samples.overloadMeter.rmsDb / 20) / 2, JSON.stringify(samples))
  assert.equal(samples.heldOverload, samples.overloadMeter.peakDb)
  assert.equal(samples.heldAfterSilence, samples.heldOverload)
  assert.ok(samples.resetHoldSilent)
  assert.ok(samples.leftPan[0] > 0.02 && samples.leftPan[1] < 1e-7, JSON.stringify(samples))
  assert.ok(samples.rightPan[1] > 0.02 && samples.rightPan[0] < 1e-7, JSON.stringify(samples))
  assert.ok(Math.abs(samples.monoHalfPan[1] / samples.monoHalfPan[0] - Math.tan(3 * Math.PI / 8)) < 0.02, JSON.stringify(samples))
  assert.ok(Math.abs(Math.hypot(...samples.monoHalfPan) / samples.leftPan[0] - 1) < 0.02, JSON.stringify(samples))
  assert.ok(samples.stereoCenter.every(value => value > 0.02), JSON.stringify(samples))
  assert.ok(samples.stereoLeft[1] < 1e-7 && samples.stereoRight[0] < 1e-7, JSON.stringify(samples))
  assert.ok(Math.abs(samples.stereoLeft[0] / Math.hypot(...samples.stereoCenter) - 1) < 0.02, JSON.stringify(samples))
  assert.ok(Math.abs(samples.stereoRight[1] / Math.hypot(...samples.stereoCenter) - 1) < 0.02, JSON.stringify(samples))
  assert.equal(samples.elementVolume, 1)
  console.log(`Chromium media gain RMS: ${JSON.stringify(samples)}`)
} finally { await browser?.close(); await server.close() }
