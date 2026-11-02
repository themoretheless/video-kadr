import { expect, it } from 'vitest'
import { elasticBounds, isElasticInterpolation, sampleElastic } from './elastic'
import { animatableExtents, normalizeInterpolation, sampleAnimatableValue } from './keyframes'
import type { CompositionElasticInterpolation, CompositionAnimatableValue } from './types'

it('matches pinned WeftCut Elastic values and exact authored keys', () => {
  const out: CompositionElasticInterpolation = { kind: 'elastic', dir: 'out', amplitude: 1, period: 0.3 }
  expect(sampleElastic(out, 0.1)).toBeCloseTo(1.25, 12)
  expect(sampleElastic({ ...out, dir: 'in' }, 0.25)).toBeCloseTo(-0.005524271728019903, 12)
  for (const dir of ['in', 'out', 'in_out'] as const) {
    const interpolation = { ...out, amplitude: 2, period: 0.17, dir }
    const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0.2, interpolation }, { tick: 1000, value: 0.4 }] } }
    expect(sampleAnimatableValue(value, 0, 1000)).toBe(0.2)
    expect(sampleAnimatableValue(value, 1000, 1000)).toBe(0.4)
    expect(sampleAnimatableValue(value, 250, 1000)).toBeCloseTo(0.2 + 0.2 * sampleElastic(interpolation, 0.25), 12)
    const bounds = animatableExtents(value)
    for (let tick = 0; tick <= 1000; tick++) {
      const sampled = sampleAnimatableValue(value, tick, 1000)
      expect(sampled).toBeGreaterThanOrEqual(bounds.minimum - 1e-12)
      expect(sampled).toBeLessThanOrEqual(bounds.maximum + 1e-12)
    }
    expect(normalizeInterpolation(interpolation)).toEqual(interpolation)
  }
})

it('validates all Elastic parameters and bounds partial intervals', () => {
  const value: CompositionElasticInterpolation = { kind: 'elastic', dir: 'in_out', amplitude: 2, period: 0.17 }
  expect(isElasticInterpolation(value)).toBe(true)
  for (const patch of [{ amplitude: 0.5 }, { period: 0 }, { period: NaN }, { dir: 'invalid' }, { extra: 1 }]) expect(isElasticInterpolation({ ...value, ...patch })).toBe(false)
  expect(() => normalizeInterpolation({ ...value, period: 0 })).toThrow()
  const [lo, hi] = elasticBounds(value, 0.3, 0.8)
  for (let n = 0; n <= 10000; n++) {
    const sampled = sampleElastic(value, 0.3 + 0.5 * n / 10000)
    expect(sampled).toBeGreaterThanOrEqual(lo - 1e-12)
    expect(sampled).toBeLessThanOrEqual(hi + 1e-12)
  }
})
