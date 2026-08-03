import { describe, expect, it } from 'vitest'
import fixtureJson from '../../../fixtures/color-grade/hsl-selective-v1.json'
import {
  applyHslSelectiveEncodedSrgb,
  applyHslSelectiveToImageData,
  encodedSrgbToHsl,
  HSL_SELECTIVE_DEFAULTS,
  HSL_SELECTIVE_PIPELINE_ORDER,
  hslSelectionMask,
  hslSelectiveActive,
  hslToEncodedSrgb,
  sanitizeHslSelective,
  type HslSelective,
  type Rgb,
} from './hsl-selective'

interface Sample extends HslSelective {
  rgb: Rgb
  expectedHsl: readonly [number, number, number]
  expectedMask: number
  expectedRgb: Rgb
}

const fixture = fixtureJson as unknown as {
  defaults: HslSelective
  order: string[]
  samples: Sample[]
  tolerances: { cpuAbsolute: number }
}

describe('shared HSL selective v1 contract', () => {
  it('matches every encoded-sRGB golden vector, circular mask, and pipeline order', () => {
    expect(HSL_SELECTIVE_DEFAULTS).toEqual(fixture.defaults)
    expect(HSL_SELECTIVE_PIPELINE_ORDER).toEqual(fixture.order)
    for (const sample of fixture.samples) {
      const hsl = encodedSrgbToHsl(sample.rgb)
      const mask = hslSelectionMask(hsl, sample.selection)
      const output = applyHslSelectiveEncodedSrgb(sample.rgb, sample)
      hsl.forEach((value, index) => expect(Math.abs(value - sample.expectedHsl[index]!)).toBeLessThanOrEqual(fixture.tolerances.cpuAbsolute))
      expect(Math.abs(mask - sample.expectedMask)).toBeLessThanOrEqual(fixture.tolerances.cpuAbsolute)
      output.forEach((value, index) => expect(Math.abs(value - sample.expectedRgb[index]!)).toBeLessThanOrEqual(fixture.tolerances.cpuAbsolute))
    }
  })

  it('round-trips all six HSL sectors and sanitizes hostile draft state', () => {
    for (const rgb of fixture.samples.map(sample => sample.rgb)) {
      const roundTrip = hslToEncodedSrgb(encodedSrgbToHsl(rgb))
      roundTrip.forEach((value, index) => expect(value).toBeCloseTo(rgb[index]!, 12))
    }
    expect(sanitizeHslSelective({
      selection: { centerDegrees: -1, halfWidthDegrees: 170, featherDegrees: 90 },
      adjustment: { hueDegrees: 999, saturation: Number.NaN, lightness: -9 },
    })).toEqual({
      selection: { centerDegrees: 359, halfWidthDegrees: 170, featherDegrees: 10 },
      adjustment: { hueDegrees: 180, saturation: 0, lightness: -1 },
    })
    expect(hslSelectiveActive(fixture.defaults)).toBe(false)
    expect(hslSelectiveActive({ ...fixture.defaults, adjustment: { ...fixture.defaults.adjustment, saturation: 1e-12 } })).toBe(true)
  })

  it('renders grade and UI-only grayscale mask while preserving alpha', () => {
    const sample = fixture.samples[2]!
    const bytes = sample.rgb.map(value => Math.round(value * 255))
    const grade = new ImageData(new Uint8ClampedArray([...bytes, 77]), 1, 1)
    applyHslSelectiveToImageData(grade, sample, 'grade')
    const quantizedInput = bytes.map(value => value / 255) as [number, number, number]
    const expectedGrade = applyHslSelectiveEncodedSrgb(quantizedInput, sample)
      .map(value => Math.round(value * 255))
    expect([...grade.data.slice(0, 3)]).toEqual(expectedGrade)
    expect(grade.data[3]).toBe(77)

    const mask = new ImageData(new Uint8ClampedArray([...bytes, 91]), 1, 1)
    applyHslSelectiveToImageData(mask, sample, 'mask')
    const expectedMask = Math.round(hslSelectionMask(encodedSrgbToHsl(quantizedInput), sample.selection) * 255)
    expect([...mask.data]).toEqual([expectedMask, expectedMask, expectedMask, 91])
  })
})
