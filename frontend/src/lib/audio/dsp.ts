import type { AudioGraphPort, AudioLevel, VoiceoverDspOptions } from './types.js'

export const DEFAULT_VOICEOVER_DSP: VoiceoverDspOptions = {
  inputGainDb: 0,
  highPassEnabled: true,
  highPassHz: 80,
  compressorEnabled: true,
  limiterEnabled: true,
}

export type VoiceoverDspStage = 'input-gain' | 'high-pass' | 'compressor' | 'limiter' | 'meter' | 'output'

function finiteOr(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback
}

export function normalizeVoiceoverDsp(options: VoiceoverDspOptions): VoiceoverDspOptions {
  return {
    inputGainDb: Math.min(18, Math.max(-24, finiteOr(options.inputGainDb, 0))),
    highPassEnabled: Boolean(options.highPassEnabled),
    highPassHz: Math.min(240, Math.max(40, finiteOr(options.highPassHz, 80))),
    compressorEnabled: Boolean(options.compressorEnabled),
    limiterEnabled: Boolean(options.limiterEnabled),
  }
}

export function dspStagePlan(options: VoiceoverDspOptions): VoiceoverDspStage[] {
  const normalized = normalizeVoiceoverDsp(options)
  return [
    'input-gain',
    ...(normalized.highPassEnabled ? ['high-pass' as const] : []),
    ...(normalized.compressorEnabled ? ['compressor' as const] : []),
    ...(normalized.limiterEnabled ? ['limiter' as const] : []),
    'meter',
    'output',
  ]
}

export function decibelsToGain(decibels: number): number {
  return 10 ** (finiteOr(decibels, 0) / 20)
}

function defaultAudioContext(): AudioContext {
  if (typeof window === 'undefined') throw new Error('Web Audio недоступен в этой среде.')
  const audioWindow = window as typeof window & { webkitAudioContext?: typeof AudioContext }
  const AudioContextConstructor = window.AudioContext ?? audioWindow.webkitAudioContext
  if (!AudioContextConstructor) {
    throw new Error('Этот браузер не поддерживает локальную обработку голоса через Web Audio.')
  }
  return new AudioContextConstructor()
}

function setCompressor(
  node: DynamicsCompressorNode,
  values: { threshold: number; knee: number; ratio: number; attack: number; release: number },
): void {
  node.threshold.value = values.threshold
  node.knee.value = values.knee
  node.ratio.value = values.ratio
  node.attack.value = values.attack
  node.release.value = values.release
}

function safeDisconnect(node: AudioNode): void {
  try {
    node.disconnect()
  } catch {
    // A partially constructed graph may already be disconnected.
  }
}

export async function createVoiceoverGraph(
  input: MediaStream,
  rawOptions: VoiceoverDspOptions,
  suppliedContext?: AudioContext,
): Promise<AudioGraphPort> {
  if (!input.getAudioTracks().length) throw new Error('Микрофон не предоставил аудиодорожку.')
  const options = normalizeVoiceoverDsp(rawOptions)
  const context = suppliedContext ?? defaultAudioContext()
  const nodes: AudioNode[] = []

  try {
    const source = context.createMediaStreamSource(input)
    const inputGain = context.createGain()
    inputGain.gain.value = decibelsToGain(options.inputGainDb)
    nodes.push(source, inputGain)

    let tail: AudioNode = inputGain
    source.connect(inputGain)

    if (options.highPassEnabled) {
      const highPass = context.createBiquadFilter()
      highPass.type = 'highpass'
      highPass.frequency.value = options.highPassHz
      highPass.Q.value = 0.707
      tail.connect(highPass)
      tail = highPass
      nodes.push(highPass)
    }

    if (options.compressorEnabled) {
      const compressor = context.createDynamicsCompressor()
      setCompressor(compressor, { threshold: -20, knee: 12, ratio: 4, attack: 0.005, release: 0.16 })
      tail.connect(compressor)
      tail = compressor
      nodes.push(compressor)
    }

    if (options.limiterEnabled) {
      const limiter = context.createDynamicsCompressor()
      setCompressor(limiter, { threshold: -1, knee: 0, ratio: 20, attack: 0.001, release: 0.05 })
      tail.connect(limiter)
      tail = limiter
      nodes.push(limiter)
    }

    const analyser = context.createAnalyser()
    analyser.fftSize = 1024
    analyser.smoothingTimeConstant = 0.65
    const destination = context.createMediaStreamDestination()
    tail.connect(analyser)
    analyser.connect(destination)
    nodes.push(analyser, destination)

    if (!destination.stream.getAudioTracks().length) {
      throw new Error('Web Audio не создал выходную аудиодорожку.')
    }
    if (context.state === 'suspended') await context.resume()

    const samples = new Float32Array(analyser.fftSize)
    let cleaned = false
    return {
      output: destination.stream,
      readLevel(): AudioLevel {
        if (cleaned) return { rms: 0, peak: 0 }
        analyser.getFloatTimeDomainData(samples)
        let sum = 0
        let peak = 0
        for (const sample of samples) {
          const absolute = Math.abs(sample)
          sum += sample * sample
          peak = Math.max(peak, absolute)
        }
        return {
          rms: Math.min(1, Math.sqrt(sum / samples.length)),
          peak: Math.min(1, peak),
        }
      },
      cleanup() {
        if (cleaned) return
        cleaned = true
        nodes.reverse().forEach(safeDisconnect)
        destination.stream.getTracks().forEach((track) => track.stop())
        void context.close().catch(() => undefined)
      },
    }
  } catch (error) {
    nodes.reverse().forEach(safeDisconnect)
    void context.close().catch(() => undefined)
    throw error
  }
}
