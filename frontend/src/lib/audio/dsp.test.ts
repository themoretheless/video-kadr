import { describe, expect, it } from 'vitest'
import {
  createVoiceoverGraph,
  decibelsToGain,
  dspStagePlan,
  normalizeVoiceoverDsp,
} from './dsp.js'
import type { VoiceoverDspOptions } from './types.js'

class FakeAudioNode {
  readonly name: string
  readonly connections: FakeAudioNode[] = []
  disconnected = false

  constructor(name: string) {
    this.name = name
  }

  connect(destination: FakeAudioNode): FakeAudioNode {
    this.connections.push(destination)
    return destination
  }

  disconnect(): void {
    this.disconnected = true
  }
}

const param = (): AudioParam => ({ value: 0 } as AudioParam)

class FakeGain extends FakeAudioNode {
  gain = param()
}

class FakeFilter extends FakeAudioNode {
  type: BiquadFilterType = 'lowpass'
  frequency = param()
  Q = param()
}

class FakeCompressor extends FakeAudioNode {
  threshold = param()
  knee = param()
  ratio = param()
  attack = param()
  release = param()
}

class FakeAnalyser extends FakeAudioNode {
  fftSize = 32
  smoothingTimeConstant = 0

  getFloatTimeDomainData(samples: Float32Array): void {
    samples.fill(0.5)
  }
}

class FakeTrack {
  stopped = false
  stop(): void { this.stopped = true }
}

function audioStream(track = new FakeTrack()): { stream: MediaStream; track: FakeTrack } {
  return {
    stream: {
      getAudioTracks: () => [track],
      getTracks: () => [track],
    } as unknown as MediaStream,
    track,
  }
}

class FakeAudioContext {
  state: AudioContextState = 'suspended'
  readonly source = new FakeAudioNode('source')
  readonly gain = new FakeGain('gain')
  readonly filter = new FakeFilter('high-pass')
  readonly compressors = [new FakeCompressor('compressor'), new FakeCompressor('limiter')]
  readonly analyser = new FakeAnalyser('analyser')
  readonly output = audioStream()
  readonly destination = Object.assign(new FakeAudioNode('destination'), { stream: this.output.stream })
  resumed = false
  closed = false

  createMediaStreamSource(): MediaStreamAudioSourceNode { return this.source as unknown as MediaStreamAudioSourceNode }
  createGain(): GainNode { return this.gain as unknown as GainNode }
  createBiquadFilter(): BiquadFilterNode { return this.filter as unknown as BiquadFilterNode }
  createDynamicsCompressor(): DynamicsCompressorNode {
    const node = this.compressors.shift()
    if (!node) throw new Error('Unexpected compressor')
    return node as unknown as DynamicsCompressorNode
  }
  createAnalyser(): AnalyserNode { return this.analyser as unknown as AnalyserNode }
  createMediaStreamDestination(): MediaStreamAudioDestinationNode {
    return this.destination as unknown as MediaStreamAudioDestinationNode
  }
  async resume(): Promise<void> { this.resumed = true; this.state = 'running' }
  async close(): Promise<void> { this.closed = true; this.state = 'closed' }
}

const FULL_DSP: VoiceoverDspOptions = {
  inputGainDb: 6,
  highPassEnabled: true,
  highPassHz: 90,
  compressorEnabled: true,
  limiterEnabled: true,
}

describe('voiceover DSP model', () => {
  it('normalizes bounds and produces a deterministic node plan', () => {
    expect(normalizeVoiceoverDsp({
      inputGainDb: 99,
      highPassEnabled: true,
      highPassHz: 5,
      compressorEnabled: false,
      limiterEnabled: true,
    })).toEqual({
      inputGainDb: 18,
      highPassEnabled: true,
      highPassHz: 40,
      compressorEnabled: false,
      limiterEnabled: true,
    })
    expect(dspStagePlan(FULL_DSP)).toEqual([
      'input-gain', 'high-pass', 'compressor', 'limiter', 'meter', 'output',
    ])
    expect(decibelsToGain(6)).toBeCloseTo(1.995, 3)
  })

  it('builds the selected Web Audio chain, meters it, and cleans every owned output', async () => {
    const input = audioStream()
    const context = new FakeAudioContext()
    const compressor = context.compressors[0]
    const limiter = context.compressors[1]

    const graph = await createVoiceoverGraph(input.stream, FULL_DSP, context as unknown as AudioContext)

    expect(context.source.connections[0]?.name).toBe('gain')
    expect(context.gain.connections[0]?.name).toBe('high-pass')
    expect(context.filter.connections[0]?.name).toBe('compressor')
    expect(compressor?.connections[0]?.name).toBe('limiter')
    expect(limiter?.connections[0]?.name).toBe('analyser')
    expect(context.analyser.connections[0]?.name).toBe('destination')
    expect(context.gain.gain.value).toBeCloseTo(decibelsToGain(6))
    expect(context.filter).toMatchObject({ type: 'highpass' })
    expect(context.filter.frequency.value).toBe(90)
    expect(compressor?.threshold.value).toBe(-20)
    expect(limiter?.threshold.value).toBe(-1)
    expect(context.resumed).toBe(true)
    expect(graph.readLevel()).toEqual({ rms: 0.5, peak: 0.5 })

    graph.cleanup()
    expect(context.output.track.stopped).toBe(true)
    expect(context.closed).toBe(true)
    expect(graph.readLevel()).toEqual({ rms: 0, peak: 0 })
  })

  it('fails instead of pretending to process a stream without audio', async () => {
    const context = new FakeAudioContext()
    const empty = { getAudioTracks: () => [], getTracks: () => [] } as unknown as MediaStream
    await expect(createVoiceoverGraph(empty, FULL_DSP, context as unknown as AudioContext))
      .rejects.toThrow('не предоставил аудиодорожку')
    expect(context.closed).toBe(false)
  })
})
