import { describe, expect, it, vi } from 'vitest'
import {
  buildWaveformBuckets,
  decodeWaveform,
  sliceWaveformBuckets,
  waveformPath,
  type AudioDecoderPort,
  type DecodedAudioLike,
} from './waveform'

describe('waveform aggregation', () => {
  it('assigns every stereo frame to deterministic min/max/RMS buckets', () => {
    const buckets = buildWaveformBuckets(
      [new Float32Array([-1, -0.5, 0.25, 1]), new Float32Array([0.5, 0, -0.25, -1])],
      2,
    )

    expect(buckets).toHaveLength(2)
    expect(buckets[0]).toEqual({ min: -1, max: 0.5, rms: Math.sqrt(1.5 / 4) })
    expect(buckets[1]).toEqual({ min: -1, max: 1, rms: Math.sqrt(2.125 / 4) })
  })

  it('bounds hostile inputs and sanitizes non-finite PCM samples', () => {
    const buckets = buildWaveformBuckets(
      [new Float32Array([Number.NaN, Number.POSITIVE_INFINITY, -4, 4])],
      99_999,
      -50,
      99_999,
    )

    expect(buckets).toEqual([
      { min: 0, max: 0, rms: 0 },
      { min: 0, max: 0, rms: 0 },
      { min: -1, max: -1, rms: 1 },
      { min: 1, max: 1, rms: 1 },
    ])
  })

  it('slices and downsamples a clip range without mutating the source summary', () => {
    const source = Array.from({ length: 8 }, (_, index) => ({
      min: -index / 10,
      max: index / 10,
      rms: index / 20,
    }))
    const sliced = sliceWaveformBuckets(source, 0.25, 0.75, 2)

    expect(sliced).toEqual([
      { min: -0.3, max: 0.3, rms: Math.sqrt((0.1 ** 2 + 0.15 ** 2) / 2) },
      { min: -0.5, max: 0.5, rms: Math.sqrt((0.2 ** 2 + 0.25 ** 2) / 2) },
    ])
    expect(source[2]).toEqual({ min: -0.2, max: 0.2, rms: 0.1 })
  })

  it('builds a stable closed SVG polygon', () => {
    expect(
      waveformPath(
        [
          { min: -1, max: 1, rms: 1 },
          { min: -0.5, max: 0.5, rms: 0.5 },
        ],
        100,
        20,
      ),
    ).toBe('M25,1 L75,5.5 L75,14.5 L25,19 Z')
    expect(waveformPath([], 100, 20)).toBe('')
  })
})

describe('waveform decoder lifecycle', () => {
  it('copies the buffer, closes the decoder, and returns a bounded summary', async () => {
    const decoded: DecodedAudioLike = {
      duration: 1,
      sampleRate: 4,
      numberOfChannels: 1,
      length: 4,
      getChannelData: () => new Float32Array([-1, 0, 0.5, 1]),
    }
    const close = vi.fn(async () => undefined)
    const decodeAudioData = vi.fn(async (data: ArrayBuffer): Promise<DecodedAudioLike> => {
      void data
      return decoded
    })
    const decoder: AudioDecoderPort = { close, decodeAudioData }
    const input = new Uint8Array([1, 2, 3]).buffer

    const summary = await decodeWaveform(input, () => decoder, 2)

    expect(summary).toEqual({
      durationSeconds: 1,
      sampleRate: 4,
      buckets: [
        { min: -1, max: 0, rms: Math.sqrt(0.5) },
        { min: 0.5, max: 1, rms: Math.sqrt(0.625) },
      ],
    })
    expect(decodeAudioData).toHaveBeenCalledTimes(1)
    expect(decodeAudioData.mock.calls[0]![0]).not.toBe(input)
    expect(close).toHaveBeenCalledTimes(1)
  })

  it('closes the decoder when decoding fails', async () => {
    const close = vi.fn(async () => undefined)
    const decoder: AudioDecoderPort = {
      close,
      decodeAudioData: vi.fn(async () => { throw new Error('unsupported codec') }),
    }

    await expect(decodeWaveform(new ArrayBuffer(1), () => decoder)).rejects.toThrow('unsupported codec')
    expect(close).toHaveBeenCalledTimes(1)
  })
})
