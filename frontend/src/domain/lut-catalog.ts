import type { LutAsset } from '../types'
import { parseCubeLut } from './cube-lut'

export type LutCatalogFilter = 'all' | 'favorites'

export function normalizeLutSearch(value: string): string {
  return value.normalize('NFKC').trim().toLocaleLowerCase()
}

export function filterLutCatalog(
  assets: readonly LutAsset[],
  query: string,
  filter: LutCatalogFilter,
): LutAsset[] {
  const needle = normalizeLutSearch(query)
  return assets.filter(asset => {
    if (filter === 'favorites' && !asset.favorite) return false
    const searchable = normalizeLutSearch(`${asset.name} ${asset.cubeSize}x${asset.cubeSize}x${asset.cubeSize}`)
    return !needle || searchable.includes(needle)
  })
}

function cssByte(value: number): number {
  return Math.round(Math.min(1, Math.max(0, value)) * 255)
}

/** Parse the strict, canonical 3D CUBE subset served by Video Kadr and return
 * a small colour reference. This is intentionally labelled as a reference,
 * not as a source-frame preview. */
export function cubeReferenceGradient(source: string): string {
  const lut = parseCubeLut(source)
  const point = ([red, green, blue]: readonly [number, number, number]) => {
    const r = Math.round((lut.cubeSize - 1) * red)
    const g = Math.round((lut.cubeSize - 1) * green)
    const b = Math.round((lut.cubeSize - 1) * blue)
    const index = ((b * lut.cubeSize + g) * lut.cubeSize + r) * 3
    const rgb = [lut.values[index]!, lut.values[index + 1]!, lut.values[index + 2]!]
    return `rgb(${cssByte(rgb[0])} ${cssByte(rgb[1])} ${cssByte(rgb[2])})`
  }
  const chart = [
    [0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1],
    [1, 1, 0], [0, 1, 1], [1, 0, 1], [0.75, 0.5, 0.35], [1, 1, 1],
  ] as const
  return `linear-gradient(90deg, ${chart.map(point).join(', ')})`
}
