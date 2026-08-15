import { describe, expect, it } from 'vitest'

import { analyzeScopeFrame, SCOPE_RESOLUTION, WAVEFORM_HEIGHT } from './scopes'

describe('local deterministic color scopes', () => {
  it('counts RGB, luma, waveform, and vectorscope bins exactly', () => {
    const frame = new Uint8ClampedArray([
      255, 0, 0, 255,
      0, 255, 0, 255,
      0, 0, 255, 255,
      255, 255, 255, 0,
    ])
    const result = analyzeScopeFrame(frame, 2, 2)

    expect(result.sampledPixels).toBe(3)
    expect(result.histogram.red[255]).toBe(1)
    expect(result.histogram.red[0]).toBe(2)
    expect(result.histogram.green[255]).toBe(1)
    expect(result.histogram.blue[255]).toBe(1)
    expect(result.histogram.luma[54]).toBe(1)
    expect(result.histogram.luma[182]).toBe(1)
    expect(result.histogram.luma[18]).toBe(1)
    expect(result.waveform.reduce((sum, value) => sum + value, 0)).toBe(3)
    expect(result.vectorscope.reduce((sum, value) => sum + value, 0)).toBe(3)
  })

  it('keeps neutral gray at vectorscope center and transparent pixels out', () => {
    const result = analyzeScopeFrame(new Uint8ClampedArray([128, 128, 128, 255]), 1, 1)
    expect(result.vectorscope[(255 - 128) * SCOPE_RESOLUTION + 128]).toBe(1)
    expect(result.waveform.reduce((sum, value) => sum + value, 0)).toBe(1)
    expect(result.waveform.length).toBe(SCOPE_RESOLUTION * WAVEFORM_HEIGHT)

    const transparent = analyzeScopeFrame(new Uint8ClampedArray([255, 0, 0, 0]), 1, 1)
    expect(transparent.sampledPixels).toBe(0)
    expect(transparent.histogram.maximum).toBe(0)
  })

  it('rejects malformed or unbounded frame buffers', () => {
    expect(() => analyzeScopeFrame(new Uint8ClampedArray(3), 1, 1)).toThrow('limit')
    expect(() => analyzeScopeFrame(new Uint8ClampedArray(), 0, 1)).toThrow('dimensions')
    expect(() => analyzeScopeFrame(new Uint8ClampedArray(), 513, 513)).toThrow('limit')
  })
})
