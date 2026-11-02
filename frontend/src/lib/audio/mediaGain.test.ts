import { expect, it, vi } from 'vitest'
import { createMediaGain } from './mediaGain'
it('routes amplification through WebAudio, resumes playback and releases both nodes exactly once', () => {
  const source = { connect: vi.fn(), disconnect: vi.fn() }
  const gain = { connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }
  const context = { createMediaElementSource: vi.fn(() => source), createGain: vi.fn(() => gain), destination: {}, currentTime: 3, state: 'suspended', resume: vi.fn().mockResolvedValue(undefined) }
  const element = document.createElement('audio')
  element.volume = 0.5
  const controller = createMediaGain(element, context as unknown as AudioContext)!
  expect(source.connect).toHaveBeenCalledWith(gain)
  expect(gain.connect).toHaveBeenCalledWith(context.destination)
  expect(element.volume).toBe(1)
  controller.setGain(4)
  expect(gain.gain.setValueAtTime).toHaveBeenLastCalledWith(4, 3)
  controller.setGain(0)
  expect(gain.gain.setValueAtTime).toHaveBeenLastCalledWith(0, 3)
  controller.resume()
  expect(context.resume).toHaveBeenCalledOnce()
  controller.dispose()
  controller.dispose()
  controller.setGain(8)
  controller.resume()
  expect(source.disconnect).toHaveBeenCalledOnce()
  expect(gain.disconnect).toHaveBeenCalledOnce()
  expect(context.resume).toHaveBeenCalledOnce()
  expect(gain.gain.setValueAtTime).toHaveBeenCalledTimes(2)
})

it('shares a post-gain role bus and releases or reroutes it without recreating media sources', async () => {
  const { readAudioRoleMeter, readAudioMasterMeter } = await import('./mediaGain')
  const analysers: { connect: ReturnType<typeof vi.fn>; disconnect: ReturnType<typeof vi.fn>; fftSize: number; getFloatTimeDomainData: ReturnType<typeof vi.fn> }[] = []
  const context = {
    destination: {}, currentTime: 0,
    createMediaElementSource: vi.fn(() => ({ connect: vi.fn(), disconnect: vi.fn() })),
    createGain: () => ({ connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }),
    createAnalyser: () => {
      const node = { connect: vi.fn(), disconnect: vi.fn(), fftSize: 0, getFloatTimeDomainData: vi.fn((samples: Float32Array) => samples.fill(0.5)) }
      analysers.push(node)
      return node
    },
  } as unknown as AudioContext
  const first = createMediaGain(document.createElement('audio'), context, 'music')!
  const second = createMediaGain(document.createElement('audio'), context, 'music')!
  expect(analysers).toHaveLength(2)
  expect(readAudioRoleMeter('music', context).rmsDb).toBeCloseTo(-6.0206)
  expect(readAudioRoleMeter('music', context).peakDb).toBeCloseTo(-6.0206)
  expect(readAudioMasterMeter(context).rmsDb).toBeCloseTo(-6.0206)
  first.setRole('sfx')
  expect(analysers).toHaveLength(3)
  expect(analysers[1]!.disconnect).not.toHaveBeenCalled()
  expect(context.createMediaElementSource).toHaveBeenCalledTimes(2)
  second.dispose()
  expect(analysers[1]!.disconnect).toHaveBeenCalledOnce()
  expect(readAudioRoleMeter('music', context).rmsDb).toBe(-Infinity)
  first.dispose()
  expect(analysers[2]!.disconnect).toHaveBeenCalledOnce()
  expect(analysers[0]!.disconnect).toHaveBeenCalledOnce()
  expect(readAudioMasterMeter(context).rmsDb).toBe(-Infinity)
  expect(readAudioRoleMeter('sfx', context).rmsDb).toBe(-Infinity)
})

it('keeps master mute when the first media graph is created after the preference changes', async () => {
  const { setPreviewMasterMuted } = await import('./mediaGain')
  const inputs: ReturnType<typeof vi.fn>[] = []
  const context = {
    destination: {}, currentTime: 7,
    createMediaElementSource: () => ({ connect: vi.fn(), disconnect: vi.fn() }),
    createGain: () => {
      const setValueAtTime = vi.fn()
      inputs.push(setValueAtTime)
      return { connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime } }
    },
    createAnalyser: () => ({ fftSize: 0, connect: vi.fn(), disconnect: vi.fn() }),
  } as unknown as AudioContext
  setPreviewMasterMuted(true)
  try {
    const controller = createMediaGain(document.createElement('audio'), context)!
    expect(inputs[1]).toHaveBeenLastCalledWith(0, 7)
    setPreviewMasterMuted(false, context)
    expect(inputs[1]).toHaveBeenLastCalledWith(1, 7)
    controller.dispose()
  } finally { setPreviewMasterMuted(false) }
})

it('uses upstream overload metering and the source compressor contract with lifecycle cleanup', () => {
  const compressor = {
    threshold: { value: 0 }, ratio: { value: 0 }, attack: { value: 0 }, release: { value: 0 }, knee: { value: 0 },
    connect: vi.fn(), disconnect: vi.fn(),
  }
  const analyser = { fftSize: 0, connect: vi.fn(), disconnect: vi.fn() }
  const context = {
    destination: {}, currentTime: 0,
    createMediaElementSource: () => ({ connect: vi.fn(), disconnect: vi.fn() }),
    createGain: () => ({ connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }),
    createAnalyser: () => analyser,
    createDynamicsCompressor: () => compressor,
  } as unknown as AudioContext
  const controller = createMediaGain(document.createElement('audio'), context)!
  expect(compressor.threshold.value).toBe(-1)
  expect(compressor.ratio.value).toBe(20)
  expect(compressor.attack.value).toBe(0.001)
  expect(compressor.release.value).toBe(0.25)
  expect(compressor.knee.value).toBe(0)
  expect(analyser.connect).toHaveBeenCalledWith(compressor)
  expect(compressor.connect).toHaveBeenCalledWith(context.destination)
  controller.dispose()
  controller.dispose()
  expect(compressor.disconnect).toHaveBeenCalledOnce()
})


it('holds the pass maximum across silence and media gaps until explicit reset', async () => {
  const { readAudioMasterMeter, readMasterPeakHold, resetMasterPeakHold } = await import('./mediaGain')
  let amplitude = 0.5
  const context = {
    destination: {}, currentTime: 0,
    createMediaElementSource: () => ({ connect: vi.fn(), disconnect: vi.fn() }),
    createGain: () => ({ connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }),
    createAnalyser: () => ({ fftSize: 0, connect: vi.fn(), disconnect: vi.fn(), getFloatTimeDomainData: (samples: Float32Array) => samples.fill(amplitude) }),
  } as unknown as AudioContext
  const controller = createMediaGain(document.createElement('audio'), context)!
  readAudioMasterMeter(context)
  expect(readMasterPeakHold(context)).toBeCloseTo(-6.0206)
  amplitude = 0.1
  readAudioMasterMeter(context)
  expect(readMasterPeakHold(context)).toBeCloseTo(-6.0206)
  amplitude = 0
  expect(readAudioMasterMeter(context).peakDb).toBe(-Infinity)
  expect(readMasterPeakHold(context)).toBeCloseTo(-6.0206)
  resetMasterPeakHold(context)
  expect(readMasterPeakHold(context)).toBe(-Infinity)
  amplitude = 0.1
  readAudioMasterMeter(context)
  expect(readMasterPeakHold(context)).toBeCloseTo(-20)
  controller.dispose()
  expect(readMasterPeakHold(context)).toBeCloseTo(-20)
  resetMasterPeakHold(context)
  expect(readMasterPeakHold(context)).toBe(-Infinity)
})

it('applies pan through StereoPanner without replacing the source and cleans up routing', () => {
  const source = { connect: vi.fn(), disconnect: vi.fn() }
  const gain = { connect: vi.fn(), disconnect: vi.fn(), gain: { setValueAtTime: vi.fn() } }
  const pan = { connect: vi.fn(), disconnect: vi.fn(), pan: { setValueAtTime: vi.fn() } }
  const context = { currentTime: 4, destination: {}, createMediaElementSource: vi.fn(() => source), createGain: () => gain, createStereoPanner: () => pan } as unknown as AudioContext
  const controller = createMediaGain(document.createElement('audio'), context)!
  controller.setPan(-1)
  expect(pan.pan.setValueAtTime).toHaveBeenLastCalledWith(-1, 4)
  controller.setPan(0.5)
  expect(pan.pan.setValueAtTime).toHaveBeenLastCalledWith(0.5, 4)
  expect(gain.connect).toHaveBeenCalledWith(pan)
  expect(pan.connect).toHaveBeenCalledWith(context.destination)
  expect(context.createMediaElementSource).toHaveBeenCalledOnce()
  controller.dispose()
  expect(pan.disconnect).toHaveBeenCalledOnce()
})
