import { beforeEach, describe, expect, it } from 'vitest'
import {
  colorAdvancedPayload,
  colorAdvancedState,
  resetColorAdvanced,
} from '../../store/colorAdvanced'
import {
  clampChannel,
  formatChannel,
  isNeutralWheel,
  masterOf,
  neutralRgb,
  nudgeWheel,
  pointFromRgb,
  rgbFromPoint,
  withMaster,
  WHEEL_CONFIG,
  WHEEL_MODES,
} from './wheel'

describe('wheel geometry', () => {
  it('leaves the centre neutral for every wheel', () => {
    for (const mode of WHEEL_MODES) {
      expect(rgbFromPoint(0, 0, mode)).toEqual(neutralRgb(mode))
      expect(isNeutralWheel(neutralRgb(mode), mode)).toBe(true)
    }
  })

  it('pushes one primary up and the other two down by half', () => {
    const lift = rgbFromPoint(0, 1, 'lift')
    expect(lift.r).toBeCloseTo(0.5, 6)
    expect(lift.g).toBeCloseTo(-0.25, 6)
    expect(lift.b).toBeCloseTo(-0.25, 6)
  })

  it('clamps a point outside the disc back onto the rim', () => {
    expect(rgbFromPoint(0, 4, 'lift')).toEqual(rgbFromPoint(0, 1, 'lift'))
  })

  it('round-trips a point through the channel values', () => {
    for (const mode of WHEEL_MODES) {
      for (const [x, y] of [
        [0, 0],
        [0, 1],
        [-0.6, 0.3],
        [0.42, -0.77],
      ]) {
        // Channel values are quantized to 1e-4, so the round trip is exact to
        // roughly that resolution and no further.
        const back = pointFromRgb(rgbFromPoint(x, y, mode), mode)
        expect(back.x).toBeCloseTo(x, 3)
        expect(back.y).toBeCloseTo(y, 3)
      }
    }
  })

  it('keeps every channel inside the contract range', () => {
    for (const mode of WHEEL_MODES) {
      const { min, max } = WHEEL_CONFIG[mode]
      const value = rgbFromPoint(-1, -1, mode)
      for (const channel of [value.r, value.g, value.b]) {
        expect(channel).toBeGreaterThanOrEqual(min)
        expect(channel).toBeLessThanOrEqual(max)
      }
    }
  })

  it('falls back to neutral for a non-finite channel', () => {
    expect(clampChannel(Number.NaN, 'gain')).toBe(1)
    expect(clampChannel(Number.POSITIVE_INFINITY, 'lift')).toBe(0)
    expect(rgbFromPoint(Number.NaN, Number.NaN, 'lift')).toEqual(neutralRgb('lift'))
  })

  it('moves the puck by a keyboard nudge without leaving the disc', () => {
    const stepped = nudgeWheel(neutralRgb('gain'), 'gain', 0, 0.1)
    expect(stepped.r).toBeGreaterThan(1)
    expect(pointFromRgb(stepped, 'gain').y).toBeCloseTo(0.1, 6)
    expect(nudgeWheel(stepped, 'gain', 0, 40)).toEqual(rgbFromPoint(0, 1, 'gain'))
  })
})

describe('luminance ring', () => {
  it('reads zero at neutral and shifts every channel together', () => {
    expect(masterOf(neutralRgb('lift'), 'lift')).toBe(0)
    const raised = withMaster(neutralRgb('lift'), 'lift', 0.5)
    expect(raised).toEqual({ r: 0.25, g: 0.25, b: 0.25 })
    expect(masterOf(raised, 'lift')).toBeCloseTo(0.5, 6)
  })

  it('keeps the hue the disc holds while the ring moves', () => {
    const tinted = rgbFromPoint(0, 0.5, 'gain')
    const lifted = withMaster(tinted, 'gain', 0.4)
    expect(lifted.r - lifted.g).toBeCloseTo(tinted.r - tinted.g, 6)
    expect(pointFromRgb(lifted, 'gain').y).toBeCloseTo(pointFromRgb(tinted, 'gain').y, 6)
  })

  it('formats lift finer than the multiplier wheels', () => {
    expect(formatChannel(0.1234, 'lift')).toBe('0.123')
    expect(formatChannel(1.2345, 'gain')).toBe('1.23')
  })
})

describe('wheel edits reaching the wire payload', () => {
  beforeEach(resetColorAdvanced)

  it('contributes nothing while every wheel sits at neutral', () => {
    expect(colorAdvancedPayload()).toEqual({})
  })

  it('sends only the wheel the user actually moved', () => {
    colorAdvancedState.gain = rgbFromPoint(0, 1, 'gain')
    expect(colorAdvancedPayload()).toEqual({
      colorAdvanced: { gain: { r: 1.5, g: 0.75, b: 0.75 } },
    })
  })

  it('drops the wheel again once it is reset to neutral', () => {
    colorAdvancedState.lift = rgbFromPoint(0.3, -0.2, 'lift')
    expect(colorAdvancedPayload()).not.toEqual({})
    colorAdvancedState.lift = neutralRgb('lift')
    expect(colorAdvancedPayload()).toEqual({})
  })
})
