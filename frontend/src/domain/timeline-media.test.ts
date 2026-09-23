import { describe, expect, it } from 'vitest'

import { computePeakBuckets, peakBucketRange, thumbnailSliceTicks } from './timeline-media'

describe('timeline media helpers', () => {
  it('computes min/max envelope buckets', () => {
    const samples = new Float32Array([0.5, -0.25, 0.9, -0.9, 0.1, -0.4, 0.2, 0.8])
    const peaks = computePeakBuckets(samples, 2)
    expect(peaks.min[0]).toBeCloseTo(-0.9)
    expect(peaks.max[0]).toBeCloseTo(0.9)
    expect(peaks.min[1]).toBeCloseTo(-0.4)
    expect(peaks.max[1]).toBeCloseTo(0.8)
  })

  it('handles empty and short inputs without crashing', () => {
    expect(Array.from(computePeakBuckets(new Float32Array(0), 4).min)).toEqual([0, 0, 0, 0])
    const one = computePeakBuckets(new Float32Array([0.3]), 4)
    expect(one.max[0]).toBeCloseTo(0.3)
    expect(one.max[1]).toBe(0)
  })

  it('produces evenly spaced clip-relative thumbnail ticks inside the clip', () => {
    const ticks = thumbnailSliceTicks(10_000_000, 4)
    expect(ticks).toHaveLength(4)
    expect(ticks[0]).toBeGreaterThan(0)
    expect(ticks[3]).toBeLessThan(10_000_000)
    expect(thumbnailSliceTicks(0, 4)).toEqual([])
  })

  it('maps a clip source range onto peak bucket indices', () => {
    const range = peakBucketRange(10_000_000, 5_000_000, 120, 1_000_000, 512)
    expect(range.start).toBe(Math.floor((10 / 120) * 512))
    expect(range.end).toBe(Math.ceil((15 / 120) * 512))
    const clamped = peakBucketRange(119_000_000, 5_000_000, 120, 1_000_000, 512)
    expect(clamped.start).toBe(Math.floor((119 / 120) * 512))
    expect(clamped.end).toBe(512)
    expect(peakBucketRange(0, 1, 0, 1_000_000, 512)).toEqual({ start: 0, end: 0 })
  })
})
