import { describe, expect, it } from 'vitest'
import {
  browserColorFilterPlan,
  browserVideoFilterArgs,
  primaryCorrectionFfmpegFilter,
} from './browser-color-pipeline'

const curves = {
  master: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  red: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  green: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
  blue: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
}

describe('browser color filter plan', () => {
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
    expect(filters[2]).toMatch(/^eq=/)
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
      temperature: 0.5,
      brightness: 0.1,
      filter: 'warm',
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
      'crop=', 'geq=', 'eq=', 'colorbalance=', 'split=2', 'lut3d=', 'blend=', 'curves=', 'vignette',
    ].map(token => graph.indexOf(token))
    expect(positions.every(position => position >= 0)).toBe(true)
    expect(positions).toEqual([...positions].sort((left, right) => left - right))
    expect(graph).toContain("blend=all_expr='A*(1-0.350000)+B*0.350000'")
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
