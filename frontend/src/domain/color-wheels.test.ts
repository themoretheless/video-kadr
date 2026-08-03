import { describe, expect, it } from 'vitest'
import fixtureJson from '../../../fixtures/color-grade/lift-gamma-gain-v1.json'
import {
  applyLiftGammaGainLinearSrgb,
  applyPrimaryAndWheelsSrgb,
  applyPrimaryAndWheelsToImageData,
  LIFT_GAMMA_GAIN_PIPELINE_ORDER,
  type LiftGammaGain,
} from './color-wheels'

interface FixtureSample extends LiftGammaGain {
  rgb: [number, number, number]
  expected: [number, number, number]
}

const fixture = fixtureJson as unknown as {
  order: string[]
  samples: FixtureSample[]
  tolerances: { cpuAbsolute: number }
}

describe('shared Lift/Gamma/Gain contract', () => {
  it('uses the canonical stage order and exact fixture vectors', () => {
    expect(LIFT_GAMMA_GAIN_PIPELINE_ORDER).toEqual(fixture.order)
    for (const sample of fixture.samples) {
      const actual = applyLiftGammaGainLinearSrgb(sample.rgb, sample)
      actual.forEach((value, channel) => {
        expect(Math.abs(value - sample.expected[channel]!)).toBeLessThanOrEqual(fixture.tolerances.cpuAbsolute)
      })
    }
  })

  it('uses one exact primary → wheels preview pass and preserves alpha', () => {
    const primary = { temperature: 0.4, tint: -0.2, highlights: 0.3, shadows: -0.1 }
    const wheels = fixture.samples[4]!
    const input = [0.2, 0.4, 0.8] as const
    const expected = applyPrimaryAndWheelsSrgb(input, primary, wheels)
    const image = new ImageData(new Uint8ClampedArray([51, 102, 204, 77]), 1, 1)
    applyPrimaryAndWheelsToImageData(image, primary, wheels)
    expect([...image.data.slice(0, 3)]).toEqual(expected.map(value => Math.round(value * 255)))
    expect(image.data[3]).toBe(77)
  })
})
