import { describe, expect, it } from 'vitest'
import { estimateWaveformOffset, waveformEnergy } from './sync'

describe('deterministic waveform synchronization', () => {
  it('returns the candidate timeline shift with an explicit sign convention', () => {
    const reference = Float32Array.from([0, 0, 0.1, 1, 0.4, 0.1, 0, 0, 0, 0, 0, 0])
    const candidate = Float32Array.from([0, 0, 0, 0, 0.1, 1, 0.4, 0.1, 0, 0, 0, 0])

    const result = estimateWaveformOffset(reference, candidate, {
      sampleRateHz: 10,
      maxOffsetSeconds: 0.5,
      minOverlapSeconds: 0.8,
    })

    expect(result).not.toBeNull()
    expect(result!.candidateStartOffsetSeconds).toBe(-0.2)
    expect(result!.correlation).toBeGreaterThan(0.99)
    expect(result!.overlapSeconds).toBe(1)
  })

  it('is invariant to positive gain and tolerates bounded noise', () => {
    const reference = Float32Array.from({ length: 80 }, (_, index) => (
      index === 16 ? 1 : index === 17 ? 0.6 : index === 52 ? 0.8 : 0.02 * Math.sin(index)
    ))
    const candidate = Float32Array.from({ length: 80 }, (_, index) => {
      const source = index + 4
      const value = reference[source] ?? 0
      return value * 0.35 + 0.003 * Math.cos(index * 2)
    })

    const result = estimateWaveformOffset(reference, candidate, {
      sampleRateHz: 20,
      maxOffsetSeconds: 1,
      minOverlapSeconds: 2,
    })

    expect(result?.candidateStartOffsetSeconds).toBe(0.2)
    expect(result?.confidence).toBeGreaterThan(0.8)
  })

  it('rejects silence and low-correlation material instead of inventing sync', () => {
    expect(estimateWaveformOffset(new Float32Array(32), new Float32Array(32), {
      sampleRateHz: 10,
      maxOffsetSeconds: 1,
    })).toBeNull()

    const rising = Float32Array.from({ length: 32 }, (_, index) => index / 31)
    const alternating = Float32Array.from({ length: 32 }, (_, index) => index % 2)
    expect(estimateWaveformOffset(rising, alternating, {
      sampleRateHz: 10,
      maxOffsetSeconds: 0,
      minOverlapSeconds: 2,
    })).toBeNull()
  })

  it('builds a finite activity envelope from malformed buckets', () => {
    expect([...waveformEnergy([
      { min: -1, max: 0.5, rms: 0.25 },
      { min: Number.NaN, max: Number.POSITIVE_INFINITY, rms: 4 },
    ])]).toEqual([0.5, 1])
  })

  it('bounds work and validates time units', () => {
    expect(() => estimateWaveformOffset(new Float32Array(1), new Float32Array(1), {
      sampleRateHz: 0,
      maxOffsetSeconds: 1,
    })).toThrow('частота')
    expect(() => estimateWaveformOffset(new Float32Array(16_385), new Float32Array(1), {
      sampleRateHz: 10,
      maxOffsetSeconds: 1,
    })).toThrow('Слишком много')
  })
})
