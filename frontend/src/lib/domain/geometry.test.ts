import { describe, expect, it } from 'vitest'
import corpus from '../../../../fixtures/geometry/transform-cases.json'
import {
  clampRect,
  GeometryError,
  point,
  rect,
  Transform2D,
  type PreviewSpace,
  type SourceSpace,
} from './geometry'

describe('geometry corpus', () => {
  it('matches the same transform and clamp fixtures as Rust', () => {
    expect(corpus.schemaVersion).toBe(1)
    for (const fixture of corpus.transformCases) {
      const matrix = Transform2D.fromMatrix<SourceSpace, PreviewSpace>(
        fixture.matrix as [number, number, number, number, number, number],
      )
      const transformed = matrix.apply(point<SourceSpace>(fixture.point[0], fixture.point[1]))
      expect([transformed.x, transformed.y]).toEqual(fixture.expected)
      const original = matrix.inverse().apply(transformed)
      expect(original.x).toBeCloseTo(fixture.point[0], 10)
      expect(original.y).toBeCloseTo(fixture.point[1], 10)
    }
    for (const fixture of corpus.clampCases) {
      const value = rect<SourceSpace>(...(fixture.rect as [number, number, number, number]))
      const bounds = rect<SourceSpace>(...(fixture.bounds as [number, number, number, number]))
      const result = clampRect(value, bounds, fixture.minimum as [number, number])
      expect([result.x, result.y, result.width, result.height]).toEqual(fixture.expected)
    }
  })

  it('keeps fuzz-like values finite, contained, and round-trippable', () => {
    let seed = 0x5eed
    const bounds = rect<SourceSpace>(0, 0, 1920, 1080)
    const transform = Transform2D.fromMatrix<SourceSpace, PreviewSpace>([0.4, 0, 0, 0.4, 12, 8])
    for (let index = 0; index < 2000; index++) {
      seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0
      const random = (shift: number) => ((seed >>> shift) % 100000) / 10 - 5000
      const value = rect<SourceSpace>(random(0), random(4), Math.abs(random(8)), Math.abs(random(12)))
      const clamped = clampRect(value, bounds, [2, 2])
      expect([clamped.x, clamped.y, clamped.width, clamped.height].every(Number.isFinite)).toBe(true)
      expect(clamped.x).toBeGreaterThanOrEqual(0)
      expect(clamped.y).toBeGreaterThanOrEqual(0)
      expect(clamped.x + clamped.width).toBeLessThanOrEqual(1920)
      expect(clamped.y + clamped.height).toBeLessThanOrEqual(1080)
      const source = point<SourceSpace>(clamped.x, clamped.y)
      const roundTrip = transform.inverse().apply(transform.apply(source))
      expect(roundTrip.x).toBeCloseTo(source.x, 9)
      expect(roundTrip.y).toBeCloseTo(source.y, 9)
    }
  })

  it('rejects non-finite and singular geometry', () => {
    expect(() => point<SourceSpace>(Number.NaN, 0)).toThrow(GeometryError)
    expect(() => Transform2D.scale<SourceSpace, PreviewSpace>(0, 1).inverse()).toThrow(GeometryError)
  })
})
