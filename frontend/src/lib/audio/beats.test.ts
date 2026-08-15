import { describe, expect, it } from 'vitest'
import type { WaveformSummary } from './waveform'
import { detectWaveformBeats } from './beats'

function pulseTrain(intervalBuckets = 10, count = 200): WaveformSummary {
  return {
    durationSeconds: 10,
    sampleRate: 48_000,
    buckets: Array.from({ length: count }, (_, index) => ({
      min: -0.1,
      max: 0.1,
      rms: index % intervalBuckets === 0 ? 0.95 : 0.02,
    })),
  }
}

describe('classical Auto Beat detector', () => {
  it('finds a deterministic 120 BPM pulse train', () => {
    const first = detectWaveformBeats(pulseTrain(), { sensitivity: 0.5 })
    const second = detectWaveformBeats(pulseTrain(), { sensitivity: 0.5 })
    expect(second).toEqual(first)
    expect(first.beats.length).toBeGreaterThanOrEqual(17)
    expect(first.estimatedBpm).toBeCloseTo(120, 0)
    expect(first.beats.slice(1).every((beat, index) => beat.timeSeconds > first.beats[index]!.timeSeconds)).toBe(true)
  })

  it('returns no invented beats for silence', () => {
    const silent = pulseTrain()
    const result = detectWaveformBeats({
      ...silent,
      buckets: silent.buckets.map(() => ({ min: 0, max: 0, rms: 0 })),
    })
    expect(result).toEqual({ beats: [], estimatedBpm: null })
  })

  it('applies refractory spacing and strongest-first output bounds', () => {
    const result = detectWaveformBeats(pulseTrain(4), {
      minimumSpacingSeconds: 0.3,
      maxBeats: 8,
      sensitivity: 1,
    })
    expect(result.beats).toHaveLength(8)
    expect(result.beats.every((beat, index) => index === 0 || beat.timeSeconds > result.beats[index - 1]!.timeSeconds)).toBe(true)
  })

  it('rejects malformed or unbounded requests', () => {
    expect(() => detectWaveformBeats({ ...pulseTrain(), durationSeconds: 0 })).toThrow('duration')
    expect(() => detectWaveformBeats(pulseTrain(), { maxBeats: 257 })).toThrow('256')
    expect(() => detectWaveformBeats(pulseTrain(), { minimumSpacingSeconds: 0 })).toThrow('spacing')
  })
})
