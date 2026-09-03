import { describe, expect, it } from 'vitest'
import { detectAudibleRanges, retainAudibleTimelineSegments } from './silence'
import type { WaveformSummary } from './waveform'

function summary(rms: readonly number[]): WaveformSummary {
  return { durationSeconds: rms.length, sampleRate: 48_000, buckets: rms.map((value) => ({ min: -value, max: value, rms: value })) }
}

describe('classical silence removal', () => {
  it('removes bounded leading, middle and trailing silence while retaining padding', () => {
    expect(detectAudibleRanges(summary([0, 0, 0.5, 0.5, 0, 0, 0, 0.5, 0, 0]), {
      thresholdDb: -40,
      minimumSilenceSeconds: 1.5,
      paddingSeconds: 0.25,
    })).toEqual([
      { start: 1.75, end: 4.25 },
      { start: 6.75, end: 8.25 },
    ])
  })

  it('does not treat short pauses as removable silence', () => {
    expect(detectAudibleRanges(summary([0.5, 0, 0.5]), {
      thresholdDb: -40,
      minimumSilenceSeconds: 1.1,
      paddingSeconds: 0,
    })).toEqual([{ start: 0, end: 3 }])
  })

  it('intersects every ordered duplicate with audible source ranges', () => {
    expect(retainAudibleTimelineSegments([
      { id: 'second', start: 4, end: 8 },
      { id: 'first', start: 0, end: 5 },
      { id: 'repeat', start: 4, end: 8 },
    ], [
      { start: 1, end: 3 },
      { start: 4.5, end: 6 },
    ])).toEqual([
      { id: 'segment-1', start: 4.5, end: 6 },
      { id: 'segment-2', start: 1, end: 3 },
      { id: 'segment-3', start: 4.5, end: 5 },
      { id: 'segment-4', start: 4.5, end: 6 },
    ])
  })

  it('fails closed when every timeline frame is silent', () => {
    expect(() => retainAudibleTimelineSegments([{ id: 'x', start: 0, end: 2 }], [])).toThrow(/тишина/)
  })
})
