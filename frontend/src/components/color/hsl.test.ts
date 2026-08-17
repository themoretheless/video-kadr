import { beforeEach, describe, expect, it } from 'vitest'
import {
  colorAdvancedPayload,
  colorAdvancedState,
  resetColorAdvanced,
} from '../../store/colorAdvanced'
import {
  adjustmentOf,
  isNeutralAdjustment,
  neutralAdjustment,
  withBandField,
  withoutBand,
  HSL_SCALE_MAX,
  HSL_UI_BANDS,
} from './hsl'

describe('HSL band list', () => {
  it('offers only the bands the export can select', () => {
    expect(HSL_UI_BANDS.map((entry) => entry.band)).toEqual([
      'red',
      'yellow',
      'green',
      'cyan',
      'blue',
      'magenta',
    ])
    // `huesaturation` has no orange range, so the backend drops it silently.
    expect(HSL_UI_BANDS.some((entry) => entry.band === 'orange')).toBe(false)
  })

  it('reads an untouched band as neutral', () => {
    expect(adjustmentOf([], 'blue')).toEqual(neutralAdjustment('blue'))
    expect(isNeutralAdjustment(neutralAdjustment('blue'))).toBe(true)
  })

  it('adds a band the first time a field moves', () => {
    const list = withBandField([], 'cyan', 'saturation', 1.4)
    expect(list).toEqual([{ band: 'cyan', hue: 0, saturation: 1.4, luminance: 1 }])
  })

  it('edits a band in place instead of duplicating it', () => {
    let list = withBandField([], 'red', 'hue', 20)
    list = withBandField(list, 'red', 'luminance', 0.8)
    expect(list).toEqual([{ band: 'red', hue: 20, saturation: 1, luminance: 0.8 }])
  })

  it('drops a band that is put back to its defaults', () => {
    let list = withBandField([], 'green', 'hue', 15)
    list = withBandField(list, 'green', 'hue', 0)
    expect(list).toEqual([])
  })

  it('clamps to the range the export can actually reach', () => {
    expect(withBandField([], 'blue', 'hue', 900)[0].hue).toBe(180)
    expect(withBandField([], 'blue', 'saturation', 99)[0].saturation).toBe(HSL_SCALE_MAX)
    expect(withBandField([], 'blue', 'luminance', -5)[0].luminance).toBe(0)
  })

  it('ignores a non-finite value instead of storing NaN', () => {
    const list = withBandField([], 'red', 'hue', Number.NaN)
    expect(list).toEqual([])
  })

  it('removes one band and leaves the others', () => {
    let list = withBandField([], 'red', 'hue', 10)
    list = withBandField(list, 'blue', 'hue', -10)
    expect(withoutBand(list, 'red')).toEqual([{ band: 'blue', hue: -10, saturation: 1, luminance: 1 }])
  })
})

describe('HSL edits reaching the wire payload', () => {
  beforeEach(resetColorAdvanced)

  it('appears in the payload only once a band is touched', () => {
    expect(colorAdvancedPayload()).toEqual({})
    colorAdvancedState.hsl = withBandField(colorAdvancedState.hsl, 'yellow', 'saturation', 1.25)
    expect(colorAdvancedPayload()).toEqual({
      colorAdvanced: { hsl: [{ band: 'yellow', hue: 0, saturation: 1.25, luminance: 1 }] },
    })
    colorAdvancedState.hsl = withoutBand(colorAdvancedState.hsl, 'yellow')
    expect(colorAdvancedPayload()).toEqual({})
  })
})
