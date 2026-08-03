import { describe, expect, it } from 'vitest'
import exponentFixture from '../../../fixtures/lut-canonical/exponents-v1.json'

import { canonicalCubeNumber, CubeLutError, encodeCanonicalCube, parseCubeLut, type RgbTuple } from './cube-lut'

function identity2(): RgbTuple[] {
  const values: RgbTuple[] = []
  for (let blue = 0; blue < 2; blue++) {
    for (let green = 0; green < 2; green++) {
      for (let red = 0; red < 2; red++) values.push([red, green, blue])
    }
  }
  return values
}

describe('strict CUBE LUT codec', () => {
  it('matches Rust f64 Display for exponent values in canonical identity', () => {
    expect([1e-7, 1e-20, 1e20, 1e21, 1.23e-7].map(canonicalCubeNumber)).toEqual([
      '0.0000001', '0.00000000000000000001', '100000000000000000000',
      '1000000000000000000000', '0.000000123',
    ])
    const parsed = parseCubeLut(exponentFixture.input)
    expect(parsed.canonicalText).toBe(exponentFixture.canonical)
    expect(parsed.sha256).toBe(exponentFixture.sha256)
  })

  it('canonicalizes comments, title, whitespace and negative zero into one content identity', () => {
    const rows = identity2().map(row => `  ${row.join('   ')}  `).join('\r\n')
    const parsed = parseCubeLut(`\uFEFF# vendor metadata\r\nTITLE "Example"\r\nDOMAIN_MAX 1 1 1\r\nLUT_3D_SIZE 2\r\nDOMAIN_MIN -0 0 0\r\n${rows}\r\n`)
    const canonical = encodeCanonicalCube(2, identity2())
    expect(parsed.canonicalText).toBe(canonical.text)
    expect(parsed.sha256).toBe(canonical.sha256)
    expect(parsed.values).toHaveLength(24)
  })

  it.each([
    ['LUT_1D_SIZE 2\n0 0 0\n1 1 1\n', 'unsupported_one_dimensional'],
    ['LUT_3D_SIZE 1\n0 0 0\n', 'invalid_size'],
    ['LUT_3D_SIZE 2\n0 0 0\n', 'wrong_entry_count'],
    ['LUT_3D_SIZE 2\nNaN 0 0\n', 'non_finite_number'],
    ['LUT_3D_SIZE 2\n0 0 0\nDOMAIN_MIN 0 0 0\n', 'header_after_data'],
    ['VENDOR_MAGIC 3\nLUT_3D_SIZE 2\n', 'unknown_header'],
  ])('rejects malformed input (%s)', (text, code) => {
    expect(() => parseCubeLut(text)).toThrowError(expect.objectContaining<Partial<CubeLutError>>({ code }))
  })

  it('enforces the exact red-fastest entry count', () => {
    const parsed = parseCubeLut(encodeCanonicalCube(2, identity2()).bytes)
    expect(Array.from(parsed.values.slice(0, 12))).toEqual([
      0, 0, 0,
      1, 0, 0,
      0, 1, 0,
      1, 1, 0,
    ])
  })
})
