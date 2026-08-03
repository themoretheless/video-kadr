import { describe, expect, it } from 'vitest'
import fixtureJson from '../../../fixtures/color-grade/primary-corrections-v1.json'
import {
  applyPrimaryCorrectionsLinearSrgb,
  applyPrimaryCorrectionsSrgb,
  PRIMARY_COLOR_PIPELINE_ORDER,
  type PrimaryCorrections,
  type Rgb,
} from './primary-color'

interface Fixture {
  schemaVersion: number
  workingSpace: string
  order: string[]
  samples: Array<PrimaryCorrections & { rgb: Rgb; expected: Rgb }>
  tolerances: { cpuAbsolute: number }
}

const fixture = fixtureJson as unknown as Fixture

describe('shared primary correction contract', () => {
  it('matches every linear-sRGB golden vector and the declared order', () => {
    expect(fixture.schemaVersion).toBe(1)
    expect(fixture.workingSpace).toBe('linear-srgb-d65')
    expect(PRIMARY_COLOR_PIPELINE_ORDER).toEqual(fixture.order)
    for (const sample of fixture.samples) {
      const actual = applyPrimaryCorrectionsLinearSrgb(sample.rgb, sample)
      actual.forEach((channel, index) => {
        expect(Math.abs(channel - sample.expected[index]!)).toBeLessThanOrEqual(fixture.tolerances.cpuAbsolute)
      })
    }
  })

  it('keeps neutral sRGB pixels unchanged through decode and encode', () => {
    const rgb: Rgb = [0.02, 0.5, 0.9]
    const actual = applyPrimaryCorrectionsSrgb(rgb, {
      temperature: 0, tint: 0, highlights: 0, shadows: 0,
    })
    actual.forEach((channel, index) => expect(channel).toBeCloseTo(rgb[index]!, 12))
  })
})
