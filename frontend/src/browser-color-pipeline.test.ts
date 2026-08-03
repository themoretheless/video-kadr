import { describe, expect, it } from 'vitest'
import {
  browserColorFilterPlan,
  browserSdrExportBoundary,
  browserVideoFilterArgs,
  linearColorCorrectionFfmpegFilter,
  primaryCorrectionFfmpegFilter,
  selectiveHslFfmpegFilter,
} from './browser-color-pipeline'

const curves = {
  master: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  red: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  green: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  blue: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
}

describe('browser color filter plan', () => {
  it('puts explicit assumed-709 conversion before grade/LUT and output conversion after curves', () => {
    const boundary = browserSdrExportBoundary('mp4', true)
    const plan = browserColorFilterPlan({ brightness: 0.1, lut: { id: 'look', intensity: 1 }, curves }, 'look.cube')
    const args = browserVideoFilterArgs(plan, { prefixFilters: [boundary.inputFilter!], suffixFilters: [boundary.outputFilter!] })
    const chain = args[1]!
    const positions = ['zscale=matrixin=', 'eq=', 'lut3d=', 'curves=', 'zscale=matrix=bt709', 'format=yuv420p'].map(token => chain.indexOf(token))
    expect(positions.every(position => position >= 0)).toBe(true)
    expect(positions).toEqual([...positions].sort((a, b) => a - b))
    expect(boundary.outputArgs).toEqual(['-color_range', 'tv', '-colorspace', 'bt709', '-color_trc', 'bt709', '-color_primaries', 'bt709'])
    expect(boundary.warning).toContain('предполагается SDR Rec.709 limited')
  })

  it('fails closed without zscale but lets MP3 bypass all video color work', () => {
    expect(() => browserSdrExportBoundary('mp4', false)).toThrow(/zscale/)
    expect(browserSdrExportBoundary('mp3', false)).toEqual({ bypassVideo: true, inputFilter: null, outputFilter: null, outputArgs: [], warning: null })
  })

  it('uses full-range sRGB output boundaries for stills and warns for GIF palette', () => {
    expect(browserSdrExportBoundary('png', true).outputFilter).toContain('transfer=iec61966-2-1')
    expect(browserSdrExportBoundary('jpg', true).outputFilter).toContain('format=rgb24')
    expect(browserSdrExportBoundary('gif', true).warning).toContain('палитрой')
  })
  it('fixes primary corrections before EQ, preset, LUT, and authored curves', () => {
    const plan = browserColorFilterPlan({
      temperature: 0.5,
      tint: -0.25,
      highlights: 0.4,
      shadows: -0.3,
      brightness: 0.1,
      filter: 'warm',
      lut: { id: 'look', intensity: 1 },
      curves,
    }, 'look.cube')
    const filters = [...plan.beforeLut, plan.lutFilter!, ...plan.afterLut]
    expect(filters[0]).toBe('format=gbrap16le')
    expect(filters[1]).toMatch(/^geq=/)
    expect(filters[1]).toContain('r(X,Y)/65535')
    expect(filters[1]).toContain('65535*if(')
    expect(filters[1]).toContain(":a='alpha(X,Y)'")
    expect(filters[2]).toMatch(/^geq=/)
    expect(filters[2]).toContain('clip(((r(X,Y)/65535-0.5)')
    expect(filters[3]).toMatch(/^colorbalance=/)
    expect(filters[4]).toBe("lut3d=file='look.cube':interp=tetrahedral")
    expect(filters[5]).toMatch(/^curves=interp=pchip/)
  })

  it('emits no primary filter for neutral or non-finite values', () => {
    expect(primaryCorrectionFfmpegFilter({})).toBeNull()
    expect(primaryCorrectionFfmpegFilter({
      temperature: Number.NaN,
      tint: Number.POSITIVE_INFINITY,
      highlights: 0,
      shadows: 0,
    })).toBeNull()
    expect(browserColorFilterPlan({})).toEqual({
      beforeLut: [], lutFilter: null, lutIntensity: 0, afterLut: [],
    })
  })

  it('compiles Lift, Gamma, and Gain into the same linear geq stage before EQ', () => {
    const payload = {
      colorWheels: {
        lift: { master: -0.1, red: 0.2, green: 0, blue: -0.2 },
        gamma: { master: 0.15, red: -0.3, green: 0.1, blue: 0 },
        gain: { master: -0.2, red: 0.4, green: 0, blue: -0.1 },
      },
      brightness: 0.1,
    }
    const filter = linearColorCorrectionFfmpegFilter(payload)
    expect(filter).toMatch(/^geq=/)
    expect(filter).toContain('max(0,')
    expect(filter).toContain('+0.25*0.100000000000')
    expect(filter).toContain('pow(2,0.150000000000)')
    expect(filter).toContain('pow(2,0.200000000000)')
    const plan = browserColorFilterPlan(payload)
    expect(plan.beforeLut[0]).toBe('format=gbrap16le')
    expect(plan.beforeLut[1]).toBe(filter)
    expect(plan.beforeLut[2]).toMatch(/^geq=/)
  })

  it('compiles exact encoded-sRGB Selective HSL after primary/LGG and preserves alpha', () => {
    const payload = {
      colorWheels: {
        lift: { master: 0, red: 0.2, green: 0, blue: 0 },
        gamma: {},
        gain: {},
      },
      hslSelective: {
        selection: { centerDegrees: 359, halfWidthDegrees: 10, featherDegrees: 5 },
        adjustment: { hueDegrees: 30, saturation: -0.2, lightness: 0.4 },
      },
      brightness: 0.1,
    }
    const selective = selectiveHslFfmpegFilter(payload)
    expect(selective).toMatch(/^geq=/)
    expect(selective).toContain('st(0,r(X,Y)/65535)')
    expect(selective).toContain('st(9,if(eq(ld(5),0),0,1-(')
    expect(selective).toContain('st(6,mod(ld(6)+ld(9)*0.083333333333333+1,1))')
    expect(selective).toContain('st(8,clip(ld(8)+0.25*ld(9)*0.400000000000000,0,1))')
    expect(selective?.match(/st\(8,clip\(ld\(8\)\+0\.25\*ld\(9\)/g)).toHaveLength(3)
    expect(selective).toContain(":a='alpha(X,Y)'")

    const plan = browserColorFilterPlan(payload)
    expect(plan.beforeLut[0]).toBe('format=gbrap16le')
    expect(plan.beforeLut[1]).toBe(linearColorCorrectionFfmpegFilter(payload))
    expect(plan.beforeLut[2]).toBe(selective)
    expect(plan.beforeLut[3]).toMatch(/^geq=/)
    expect(selectiveHslFfmpegFilter({
      hslSelective: {
        selection: { centerDegrees: 0, halfWidthDegrees: 30, featherDegrees: 15 },
        adjustment: { hueDegrees: 0, saturation: 0, lightness: 0 },
      },
    })).toBeNull()
  })

  it('bypasses a zero-intensity LUT without requiring an asset or graph', () => {
    const plan = browserColorFilterPlan({ lut: { id: 'look', intensity: 0 }, curves })
    const args = browserVideoFilterArgs(plan, { prefixFilters: ['crop=10:10:0:0'] })
    expect(args[0]).toBe('-vf')
    expect(args.join(' ')).not.toContain('lut3d=')
    expect(args.join(' ')).not.toContain('split=2')
    expect(args.join(' ')).not.toContain('-map')
    expect(args.join(' ')).toContain('crop=10:10:0:0,format=gbrap16le,curves=')
  })

  it('uses split/lut3d/blend for partial intensity and maps filtered video plus optional audio', () => {
    const plan = browserColorFilterPlan({
      colorWheels: {
        lift: { master: 0.2, red: 0.1, green: 0, blue: -0.1 },
        gamma: { master: -0.1, red: 0, green: 0.2, blue: 0 },
        gain: { master: 0.1, red: 0, green: 0, blue: 0.2 },
      },
      brightness: 0.1,
      filter: 'warm',
      hslSelective: {
        selection: { centerDegrees: 0, halfWidthDegrees: 20, featherDegrees: 10 },
        adjustment: { hueDegrees: 15, saturation: 0.1, lightness: -0.1 },
      },
      lut: { id: 'look', intensity: 0.35 },
      curves,
    }, 'look.cube')
    const args = browserVideoFilterArgs(plan, {
      prefixFilters: ['crop=10:10:0:0'],
      suffixFilters: ['vignette'],
      mapAudio: true,
    })
    expect(args[0]).toBe('-filter_complex')
    const graph = args[1]!
    const positions = [
      'crop=', 'geq=', 'st(0,r(X,Y)/65535)', 'clip(((r(X,Y)/65535-0.5)', 'colorbalance=', 'split=2', 'lut3d=', 'blend=', 'curves=', 'vignette',
    ].map(token => graph.indexOf(token))
    expect(positions.every(position => position >= 0)).toBe(true)
    expect(positions).toEqual([...positions].sort((left, right) => left - right))
    expect(graph).toContain("blend=all_expr='A*(1-0.350000)+B*0.350000'")
    expect(graph.indexOf('geq=')).toBeLessThan(graph.indexOf('split=2'))
    expect(graph.indexOf('blend=')).toBeLessThan(graph.indexOf('curves='))
    expect(args.slice(2)).toEqual(['-map', '[browser_vout]', '-map', '0:a?'])
  })

  it('keeps a full-intensity LUT in a simple ordered -vf chain', () => {
    const plan = browserColorFilterPlan({ lut: { id: 'look', intensity: 1 }, curves }, 'look.cube')
    const args = browserVideoFilterArgs(plan, { mapAudio: true })
    expect(args[0]).toBe('-vf')
    expect(args).not.toContain('-filter_complex')
    expect(args).not.toContain('-map')
    const chain = args[1]!
    expect(chain.indexOf('lut3d=')).toBeLessThan(chain.indexOf('curves='))
  })
})
