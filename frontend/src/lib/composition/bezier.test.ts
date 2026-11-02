import { expect, it } from 'vitest'
import { bezierBounds, isBezierInterpolation, sampleBezier } from './bezier'
import { sampleAnimatableValue } from './keyframes'
import { scaleTracksTwins } from './scaleLink'
import type { CompositionAnimatableValue, CompositionBezierInterpolation } from './types'

it('evaluates arbitrary Bézier, exact keys and source Continue tangents', () => {
  const mode: CompositionBezierInterpolation = { kind: 'bezier', p1: [0.1, -2], p2: [0.9, 3] }
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: mode, keyframes: [{ tick: 0, value: 0.2 }, { tick: 1000, value: 0.4 }], extrapolation: { before: 'continue', after: 'continue' } } }
  expect(sampleAnimatableValue(value, 0, 1000)).toBe(0.2)
  expect(sampleAnimatableValue(value, 1000, 1000)).toBe(0.4)
  expect(sampleAnimatableValue(value, -1000, 1000)).toBeCloseTo(4.2, 12)
  expect(sampleAnimatableValue(value, 2000, 1000)).toBeCloseTo(-3.6, 12)
  expect(scaleTracksTwins(value, structuredClone(value))).toBe(true)
  expect(scaleTracksTwins(value, { ...value, track: { ...value.track, interpolation: { ...mode, p1: [0.1, -1] } } })).toBe(false)
  const [lo, hi] = bezierBounds(mode)
  for (let tick = 0; tick <= 1000; tick++) {
    const sample = sampleBezier(mode, tick / 1000)
    expect(sample).toBeGreaterThanOrEqual(lo - 1e-12)
    expect(sample).toBeLessThanOrEqual(hi + 1e-12)
  }
  expect(sampleBezier({ kind: 'bezier', p1: [1 / 3, 0], p2: [2 / 3, 1 / 3] }, 0.25)).toBeCloseTo(0.0625, 12)
  expect(sampleBezier({ kind: 'bezier', p1: [0.25, 0.1], p2: [0.25, 1] }, 0.5)).toBeCloseTo(0.802403387695, 6)
})

it('rejects invalid coordinates and unknown fields', () => {
  expect(isBezierInterpolation({ kind: 'bezier', p1: [0.1, -2], p2: [0.9, 3] })).toBe(true)
  for (const p1 of [[-0.1, 0], [1.1, 0], [0, NaN], [0], [0, 0, 0]]) expect(isBezierInterpolation({ kind: 'bezier', p1, p2: [1, 1] })).toBe(false)
  expect(isBezierInterpolation({ kind: 'bezier', p1: [0, 0], p2: [1, 1], extra: 1 })).toBe(false)
})
