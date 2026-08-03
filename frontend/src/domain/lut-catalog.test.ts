import { describe, expect, it } from 'vitest'
import { cubeReferenceGradient, filterLutCatalog, normalizeLutSearch } from './lut-catalog'

const assets = [
  { id: '1', name: 'Café Film', cubeSize: 33, sizeBytes: 1, favorite: true },
  { id: '2', name: 'Холодный', cubeSize: 17, sizeBytes: 1, favorite: false },
]

describe('LUT catalog domain', () => {
  it('normalizes Unicode/case and searches names or dimensions', () => {
    expect(normalizeLutSearch('  ＣＡＦÉ ')).toBe('café')
    expect(filterLutCatalog(assets, 'CAFÉ', 'all').map(item => item.id)).toEqual(['1'])
    expect(filterLutCatalog(assets, '17x17x17', 'all').map(item => item.id)).toEqual(['2'])
    expect(filterLutCatalog(assets, '', 'favorites').map(item => item.id)).toEqual(['1'])
  })

  it('builds a bounded reference only from a complete cube', () => {
    const cube = 'LUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n'
    const identity = cubeReferenceGradient(cube)
    expect(identity).toContain('rgb(0 0 0)')
    expect(identity).toContain('rgb(255 255 255)')
    const swapRedBlue = cubeReferenceGradient(
      'LUT_3D_SIZE 2\n0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n',
    )
    expect(swapRedBlue).not.toBe(identity)
    expect(() => cubeReferenceGradient('LUT_3D_SIZE 2\n0 0 0')).toThrow()
    expect(() => cubeReferenceGradient(`${cube.replace('1 1 1', 'NaN 1 1')}`)).toThrow()
  })
})
