import { describe, expect, it } from 'vitest'

import { encodeCanonicalCube, parseCubeLut, type RgbTuple } from './cube-lut'
import { BAKED_CUBE_SIZE, bakeCube33, bakeEditCube33, lutBakeCapability, LUT_BAKER_CONTRACT, LutBakerError } from './lut-baker'

function inverted2() {
  const values: RgbTuple[] = []
  for (let blue = 0; blue < 2; blue++) {
    for (let green = 0; green < 2; green++) {
      for (let red = 0; red < 2; red++) values.push([1 - red, 1 - green, 1 - blue])
    }
  }
  return parseCubeLut(encodeCanonicalCube(2, values).bytes)
}

describe('capability-gated 33^3 LUT baker', () => {
  it('bakes identity deterministically with exactly 35,937 entries', () => {
    const first = bakeCube33({ contract: LUT_BAKER_CONTRACT })
    const second = bakeCube33({ contract: LUT_BAKER_CONTRACT })
    expect(first.sha256).toBe(second.sha256)
    expect(first.canonicalBytes).toEqual(second.canonicalBytes)
    const parsed = parseCubeLut(first.canonicalBytes)
    expect(parsed.cubeSize).toBe(BAKED_CUBE_SIZE)
    expect(parsed.values).toHaveLength(BAKED_CUBE_SIZE ** 3 * 3)
    expect(Array.from(parsed.values.slice(0, 6))).toEqual([0, 0, 0, 1 / 32, 0, 0])
    expect(Array.from(parsed.values.slice(-3))).toEqual([1, 1, 1])
  })

  it('tetrahedrally resamples and blends a validated input LUT', () => {
    const baked = parseCubeLut(bakeCube33({
      contract: LUT_BAKER_CONTRACT,
      inputLut: inverted2(),
      intensity: 0.5,
    }).canonicalBytes)
    // An inversion blended at 50% maps every input to middle grey.
    for (const offset of [0, 16 * 3, (BAKED_CUBE_SIZE ** 3 - 1) * 3]) {
      expect(Array.from(baked.values.slice(offset, offset + 3))).toEqual([0.5, 0.5, 0.5])
    }
  })

  it('fails closed for unknown effects, invalid intensity and cancellation', () => {
    expect(lutBakeCapability({ contract: LUT_BAKER_CONTRACT, vignette: true })).toMatchObject({ available: false })
    expect(lutBakeCapability({ contract: LUT_BAKER_CONTRACT, intensity: 2 })).toMatchObject({ available: false })
    const controller = new AbortController()
    controller.abort()
    expect(() => bakeCube33({ contract: LUT_BAKER_CONTRACT }, controller.signal))
      .toThrowError(expect.objectContaining<Partial<LutBakerError>>({ code: 'cancelled' }))
  })

  it('matches the Rust v1 point-color baker identity', () => {
    expect(bakeEditCube33({
      size: 33,
      edit: { brightness: 0.1, contrast: 1.2, saturation: 0.8 },
    }).sha256).toBe('62f55a9e0fac2cf9eca2ca11833c015e270ab601078978c1541554a6db5f9a21')
    expect(bakeEditCube33({
      size: 33,
      edit: {
        brightness: -0.05,
        contrast: 1.1,
        saturation: 0.9,
        filter: 'sepia',
        curves: {
          master: [{ x: 0, y: 0 }, { x: 0.5, y: 0.65 }, { x: 1, y: 1 }],
          red: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
          green: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
          blue: [{ x: 0, y: 0 }, { x: 1, y: 1 }],
        },
      },
    }).sha256).toBe('7ee43a4a96d1699eee8598964055fa4fb6147fdcd6d1704c162d8adc7d7b424a')
  })

  it('mirrors backend deny_unknown_fields and accepts optional curve channels', () => {
    expect(() => bakeEditCube33({ size: 33, edit: { filter: 'warm' } } as never))
      .toThrowError(expect.objectContaining<Partial<LutBakerError>>({ code: 'unsupported_recipe' }))
    expect(() => bakeEditCube33({ size: 33, edit: { crop: { x: 0 } } } as never))
      .toThrowError(expect.objectContaining<Partial<LutBakerError>>({ code: 'unsupported_recipe' }))
    expect(() => bakeEditCube33({
      size: 33,
      edit: { curves: { master: [{ x: 0, y: 0 }, { x: 1, y: 1 }] } },
    } as never)).not.toThrow()
  })
})
