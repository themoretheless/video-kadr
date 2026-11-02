import type { CompositionAudioRole } from '../composition/types'
let sharedContext: AudioContext | undefined

export interface MediaGain {
  setGain(value: number): void
  setPan(value: number): void
  setRole(role?: CompositionAudioRole): void
  resume(): void
  dispose(): void
}

/** One graph per media element; URLs may change without recreating its source. */
export function createMediaGain(element: HTMLMediaElement, context?: AudioContext, role?: CompositionAudioRole): MediaGain | undefined {
  if (!context) {
    if (typeof AudioContext === 'undefined') return undefined
    context = sharedContext ??= new AudioContext()
  }
  const source = context.createMediaElementSource(element)
  const gain = context.createGain()
  source.connect(gain)
  const master = acquireMasterBus(context)
  const output = master?.input ?? context.destination
  const pan = typeof context.createStereoPanner === 'function' ? context.createStereoPanner() : undefined
  if (pan) gain.connect(pan)
  const routed = pan ?? gain
  let currentRole = role
  let bus = role ? acquireRoleBus(context, role, output) : undefined
  routed.connect(bus?.analyser ?? output)
  element.volume = 1
  let disposed = false
  return {
    setGain(value) {
      if (disposed) return
      gain.gain.setValueAtTime(Number.isFinite(value) ? Math.max(0, value) : 0, context.currentTime)
    },
    setPan(value) {
      if (disposed) return
      const safe = Number.isFinite(value) ? Math.max(-1, Math.min(1, value)) : 0
      if (pan) pan.pan.setValueAtTime(safe, context.currentTime)
      else if (safe !== 0) throw new Error('WebAudio pan unavailable')
    },
    setRole(nextRole) {
      if (disposed || nextRole === currentRole) return
      routed.disconnect()
      if (bus) releaseRoleBus(context, currentRole!, bus)
      currentRole = nextRole
      bus = nextRole ? acquireRoleBus(context, nextRole, output) : undefined
      routed.connect(bus?.analyser ?? output)
    },
    resume() {
      if (!disposed && context.state === 'suspended') void context.resume().catch(() => undefined)
    },
    dispose() {
      if (disposed) return
      disposed = true
      source.disconnect()
      gain.disconnect()
      pan?.disconnect()
      if (bus) releaseRoleBus(context, currentRole!, bus)
      if (master && --master.users === 0) { master.input.disconnect(); master.analyser.disconnect(); master.compressor?.disconnect(); masterBuses.delete(context) }
    },
  }
}

interface RoleBus { analyser: AnalyserNode; samples: Float32Array<ArrayBuffer>; users: number }
const roleBuses = new WeakMap<AudioContext, Map<CompositionAudioRole, RoleBus>>()
function acquireRoleBus(context: AudioContext, role: CompositionAudioRole, output: AudioNode): RoleBus {
  let buses = roleBuses.get(context)
  if (!buses) { buses = new Map(); roleBuses.set(context, buses) }
  let bus = buses.get(role)
  if (!bus) {
    const analyser = context.createAnalyser()
    analyser.fftSize = 2048
    analyser.connect(output)
    bus = { analyser, samples: new Float32Array(analyser.fftSize), users: 0 }
    buses.set(role, bus)
  }
  bus.users++
  return bus
}
function releaseRoleBus(context: AudioContext, role: CompositionAudioRole, bus: RoleBus): void {
  if (--bus.users === 0) { bus.analyser.disconnect(); roleBuses.get(context)?.delete(role) }
}
/** Measures the summed, post-gain signal of a role, rather than adding clip RMS values. */
export function readAudioRoleMeter(role: CompositionAudioRole, context = sharedContext): { rmsDb: number; peakDb: number } {
  const bus = context && roleBuses.get(context)?.get(role)
  return readBusMeter(bus)
}
function readBusMeter(bus?: RoleBus): { rmsDb: number; peakDb: number } {
  if (!bus) return { rmsDb: -Infinity, peakDb: -Infinity }
  bus.analyser.getFloatTimeDomainData(bus.samples)
  let energy = 0
  let peak = 0
  for (const sample of bus.samples) { energy += sample * sample; peak = Math.max(peak, Math.abs(sample)) }
  return { rmsDb: 20 * Math.log10(Math.sqrt(energy / bus.samples.length)), peakDb: 20 * Math.log10(peak) }
}

interface MasterBus extends RoleBus { input: GainNode; compressor?: DynamicsCompressorNode }
let previewMasterMuted = false
const masterBuses = new WeakMap<AudioContext, MasterBus>()
const masterPeakHolds = new WeakMap<AudioContext, number>()
function acquireMasterBus(context: AudioContext): MasterBus | undefined {
  if (typeof context.createAnalyser !== 'function') return undefined
  let bus = masterBuses.get(context)
  if (!bus) {
    const analyser = context.createAnalyser()
    analyser.fftSize = 2048
    const compressor = typeof context.createDynamicsCompressor === 'function' ? context.createDynamicsCompressor() : undefined
    if (compressor) {
      compressor.threshold.value = -1
      compressor.ratio.value = 20
      compressor.attack.value = 0.001
      compressor.release.value = 0.25
      compressor.knee.value = 0
      analyser.connect(compressor)
      compressor.connect(context.destination)
    } else analyser.connect(context.destination)
    const input = context.createGain()
    input.gain.setValueAtTime(previewMasterMuted ? 0 : 1, context.currentTime)
    input.connect(analyser)
    bus = { input, analyser, compressor, samples: new Float32Array(analyser.fftSize), users: 0 }
    masterBuses.set(context, bus)
  }
  bus.users++
  return bus
}
/** Includes role buses and embedded video audio in the actual summed output. */
export function readAudioMasterMeter(context = sharedContext): { rmsDb: number; peakDb: number } {
  const bus = context && masterBuses.get(context)
  const sample = readBusMeter(bus)
  if (context && bus) masterPeakHolds.set(context, Math.max(masterPeakHolds.get(context) ?? -Infinity, sample.peakDb))
  return sample
}
export function readMasterPeakHold(context = sharedContext): number {
  return context ? masterPeakHolds.get(context) ?? -Infinity : -Infinity
}
export function resetMasterPeakHold(context = sharedContext): void {
  if (context) masterPeakHolds.delete(context)
}

/** Preview preference; never written to the composition or export gain. */
export function setPreviewMasterMuted(muted: boolean, context?: AudioContext): void {
  if (!context) { previewMasterMuted = muted; context = sharedContext }
  const bus = context && masterBuses.get(context)
  if (bus && context) bus.input.gain.setValueAtTime(muted ? 0 : 1, context.currentTime)
}

/** Native mixed PCM shares preview mute, master limiting and metering with media. */
export function createPcmOutput(): { context: AudioContext; input: GainNode; dispose(): void } {
  if (typeof AudioContext === 'undefined') throw new Error('WebAudio недоступен')
  const context = sharedContext ??= new AudioContext()
  const master = acquireMasterBus(context)
  const input = context.createGain()
  input.connect(master?.input ?? context.destination)
  let disposed = false
  return { context, input, dispose() {
    if (disposed) return
    disposed = true
    input.disconnect()
    if (master && --master.users === 0) {
      master.input.disconnect(); master.analyser.disconnect(); master.compressor?.disconnect(); masterBuses.delete(context)
    }
  } }
}
