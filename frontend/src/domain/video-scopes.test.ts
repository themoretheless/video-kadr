import { describe, expect, it, vi } from 'vitest'
import fixture from '../../../fixtures/video-scopes/video-scopes-v1.json'
import {
  analyzeVideoScopes,
  VIDEO_SCOPES_LIVE_SAMPLE_CAP,
  VideoScopesCancelledError,
} from './video-scopes'

function nonZero(plane: Uint32Array): Array<[number, number]> {
  return [...plane.entries()].filter(([, value]) => value !== 0)
}

describe('video scopes v1 analyzer', () => {
  it('matches the shared encoded-sRGB, alpha-weighted fixture', async () => {
    const result = await analyzeVideoScopes({
      rgba: new Uint8ClampedArray(fixture.rgba),
      width: fixture.width,
      height: fixture.height,
      rate: 'paused',
      scopes: ['histogram', 'waveform', 'parade', 'vectorscope'],
    })
    expect(result.schemaVersion).toBe(fixture.schemaVersion)
    expect(result.descriptor).toBe(fixture.descriptor)
    expect(result.sampledPixels).toBe(fixture.expected.sampledPixels)
    expect(result.alphaWeight).toBe(fixture.expected.alphaWeight)
    expect(nonZero(result.histogram!.subarray(0, 256))).toEqual(fixture.expected.histogram.y)
    expect(nonZero(result.histogram!.subarray(256, 512))).toEqual(fixture.expected.histogram.r)
    expect(nonZero(result.histogram!.subarray(512, 768))).toEqual(fixture.expected.histogram.g)
    expect(nonZero(result.histogram!.subarray(768, 1024))).toEqual(fixture.expected.histogram.b)
    expect(result.waveform).toHaveLength(512 * 256)
    expect(nonZero(result.waveform!)).toEqual(fixture.expected.waveform.map(([x, y, weight]) => [x * 256 + y, weight]))

    const paradePlane = 512 * 256
    for (const [planeIndex, channel] of ['r', 'g', 'b'].entries()) {
      const expected = fixture.expected.parade[channel as keyof typeof fixture.expected.parade]
        .map(([x, invertedBin, weight]) => [x * 256 + invertedBin, weight])
      expect(nonZero(result.parade!.subarray(planeIndex * paradePlane, (planeIndex + 1) * paradePlane))).toEqual(expected)
    }
    expect(nonZero(result.vectorscope!)).toEqual(
      fixture.expected.vectorscope
        .map(([x, y, weight]) => [y * 256 + x, weight])
        .sort((left, right) => left[0] - right[0]),
    )
  })

  it('allocates only selected result planes and is byte-for-byte deterministic', async () => {
    const request = {
      rgba: new Uint8ClampedArray(fixture.rgba), width: 2, height: 2,
      rate: 'live' as const, scopes: ['histogram'] as const,
    }
    const first = await analyzeVideoScopes(request)
    const second = await analyzeVideoScopes(request)
    expect(first.histogram).toEqual(second.histogram)
    expect(first.waveform).toBeUndefined()
    expect(first.parade).toBeUndefined()
    expect(first.vectorscope).toBeUndefined()
  })

  it('uses the deterministic square-root stride and respects the live cap', async () => {
    const width = 1024
    const height = 1024
    const result = await analyzeVideoScopes({
      rgba: new Uint8ClampedArray(width * height * 4), width, height,
      rate: 'live', scopes: [],
    })
    expect(result.stride).toBe(4)
    expect(result.sampledPixels).toBe(256 * 256)
    expect(result.sampledPixels).toBeLessThanOrEqual(VIDEO_SCOPES_LIVE_SAMPLE_CAP)

    const skinnyWidth = 53
    const skinnyHeight = 4855
    const skinny = await analyzeVideoScopes({
      rgba: new Uint8ClampedArray(skinnyWidth * skinnyHeight * 4),
      width: skinnyWidth, height: skinnyHeight, rate: 'live', scopes: [],
    })
    expect(skinny.stride).toBe(2)
    expect(skinny.sampledPixels).toBeLessThanOrEqual(VIDEO_SCOPES_LIVE_SAMPLE_CAP)
  })

  it('starts an odd-sized strided fixture at the stride-cell center', async () => {
    const sample = fixture.oddStrideCase
    const rgba = new Uint8ClampedArray(sample.width * sample.height * 4)
    rgba[3] = 9
    rgba[(sample.expectedOffset * sample.width + sample.expectedOffset) * 4 + 3] = 7
    const result = await analyzeVideoScopes({
      rgba, width: sample.width, height: sample.height,
      rate: sample.rate as 'paused', scopes: ['histogram'],
    })
    expect(result.stride).toBe(sample.expectedStride)
    expect(result.sampledColumns).toBe(sample.expectedColumns)
    expect(result.sampledRows).toBe(sample.expectedRows)
    expect(result.sampledPixels).toBe(sample.expectedColumns * sample.expectedRows)
    expect(result.alphaWeight).toBe(7)
  })

  it('rejects malformed frames and observes cancellation every 32 sampled rows', async () => {
    await expect(analyzeVideoScopes({
      rgba: new Uint8ClampedArray(3), width: 1, height: 1, rate: 'paused', scopes: [],
    })).rejects.toThrow('RGBA byte length')

    let cancelled = false
    const yielded = vi.fn(async () => { cancelled = true })
    await expect(analyzeVideoScopes({
      rgba: new Uint8ClampedArray(4 * 64), width: 1, height: 64,
      rate: 'paused', scopes: [],
    }, { isCancelled: () => cancelled, yieldEvery32Rows: yielded }))
      .rejects.toBeInstanceOf(VideoScopesCancelledError)
    expect(yielded).toHaveBeenCalledTimes(1)
  })
})
