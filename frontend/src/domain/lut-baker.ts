import {
  encodeCanonicalCube,
  type ParsedCubeLut,
  type RgbTuple,
} from './cube-lut'
import type { LutBakeRequest } from '../types'

export const LUT_BAKER_CONTRACT = 'sdr-lut-baker-v1' as const
export const BAKED_CUBE_SIZE = 33

/**
 * Deliberately narrow v1 recipe. It can bake identity or one canonical input
 * LUT plus intensity. Every other colour/effect operation must be rejected by
 * the capability check instead of being silently omitted.
 */
export interface LutBakeRecipeV1 {
  contract: typeof LUT_BAKER_CONTRACT
  inputLut?: ParsedCubeLut
  intensity?: number
}

export interface LutBakeCapability {
  available: boolean
  reason?: string
}

export interface BakedCubeLut {
  contract: typeof LUT_BAKER_CONTRACT
  cubeSize: typeof BAKED_CUBE_SIZE
  canonicalText: string
  canonicalBytes: Uint8Array
  sha256: string
}

export class LutBakerError extends Error {
  constructor(public readonly code: 'unsupported_recipe' | 'invalid_intensity' | 'cancelled') {
    super(`LUT baker failed: ${code}`)
    this.name = 'LutBakerError'
  }
}

export function lutBakeCapability(recipe: unknown): LutBakeCapability {
  if (!recipe || typeof recipe !== 'object' || Array.isArray(recipe)) {
    return { available: false, reason: 'Некорректный рецепт LUT' }
  }
  const value = recipe as Record<string, unknown>
  const allowed = new Set(['contract', 'inputLut', 'intensity'])
  if (Object.keys(value).some(key => !allowed.has(key))) {
    return { available: false, reason: 'Рецепт содержит эффект, который нельзя представить в 3D LUT' }
  }
  if (value.contract !== LUT_BAKER_CONTRACT) {
    return { available: false, reason: `Нужен контракт ${LUT_BAKER_CONTRACT}` }
  }
  if (value.inputLut !== undefined) {
    const lut = value.inputLut as Partial<ParsedCubeLut> | null
    if (!lut
      || lut.contract !== 'cube-lut-canonical-v1'
      || !(lut.values instanceof Float64Array)
      || !Number.isInteger(lut.cubeSize)
      || !lut.cubeSize
      || lut.values.length !== lut.cubeSize ** 3 * 3
      || !Array.isArray(lut.domainMin)
      || !Array.isArray(lut.domainMax)
      || lut.domainMin.length !== 3
      || lut.domainMax.length !== 3
      || !lut.domainMin.every((entry, index) => Number.isFinite(entry) && entry < lut.domainMax![index]!)) {
      return { available: false, reason: 'Исходный LUT не прошёл строгую проверку' }
    }
  }
  const intensity = value.intensity ?? 1
  if (typeof intensity !== 'number' || !Number.isFinite(intensity) || intensity < 0 || intensity > 1) {
    return { available: false, reason: 'Интенсивность LUT должна быть от 0 до 1' }
  }
  return { available: true }
}

function sample(lut: ParsedCubeLut, red: number, green: number, blue: number): RgbTuple {
  const coordinates = [red, green, blue].map((value, channel) => {
    const min = lut.domainMin[channel]!
    const max = lut.domainMax[channel]!
    return Math.max(0, Math.min(1, (value - min) / (max - min))) * (lut.cubeSize - 1)
  })
  const low = coordinates.map(Math.floor)
  const high = coordinates.map(value => Math.min(lut.cubeSize - 1, Math.ceil(value)))
  const fraction = coordinates.map((value, channel) => value - low[channel]!)
  const at = (r: number, g: number, b: number): RgbTuple => {
    const index = ((b * lut.cubeSize + g) * lut.cubeSize + r) * 3
    return [lut.values[index]!, lut.values[index + 1]!, lut.values[index + 2]!]
  }
  const [r0, g0, b0] = low as [number, number, number]
  const [r1, g1, b1] = high as [number, number, number]
  const [fr, fg, fb] = fraction as [number, number, number]
  const c000 = at(r0, g0, b0)
  const c100 = at(r1, g0, b0)
  const c010 = at(r0, g1, b0)
  const c001 = at(r0, g0, b1)
  const c110 = at(r1, g1, b0)
  const c101 = at(r1, g0, b1)
  const c011 = at(r0, g1, b1)
  const c111 = at(r1, g1, b1)
  const result = [0, 0, 0]
  const edge = (channel: number, origin: RgbTuple, ...steps: Array<[number, RgbTuple, RgbTuple]>) => {
    result[channel] = origin[channel]! + steps.reduce(
      (sum, [amount, to, from]) => sum + amount * (to[channel]! - from[channel]!),
      0,
    )
  }
  for (let channel = 0; channel < 3; channel++) {
    if (fr >= fg) {
      if (fg >= fb) edge(channel, c000, [fr, c100, c000], [fg, c110, c100], [fb, c111, c110])
      else if (fr >= fb) edge(channel, c000, [fr, c100, c000], [fb, c101, c100], [fg, c111, c101])
      else edge(channel, c000, [fb, c001, c000], [fr, c101, c001], [fg, c111, c101])
    } else if (fb >= fg) edge(channel, c000, [fb, c001, c000], [fg, c011, c001], [fr, c111, c011])
    else if (fb >= fr) edge(channel, c000, [fg, c010, c000], [fb, c011, c010], [fr, c111, c011])
    else edge(channel, c000, [fg, c010, c000], [fr, c110, c010], [fb, c111, c110])
  }
  return result as unknown as RgbTuple
}

export function bakeCube33(recipe: LutBakeRecipeV1, signal?: AbortSignal): BakedCubeLut {
  const capability = lutBakeCapability(recipe)
  if (!capability.available) {
    const intensity = recipe && typeof recipe === 'object' ? recipe.intensity : undefined
    throw new LutBakerError(typeof intensity === 'number' && (!Number.isFinite(intensity) || intensity < 0 || intensity > 1)
      ? 'invalid_intensity'
      : 'unsupported_recipe')
  }
  const intensity = recipe.intensity ?? 1
  const values = function* (): Iterable<RgbTuple> {
    for (let blue = 0; blue < BAKED_CUBE_SIZE; blue++) {
      if (signal?.aborted) throw new LutBakerError('cancelled')
      for (let green = 0; green < BAKED_CUBE_SIZE; green++) {
        for (let red = 0; red < BAKED_CUBE_SIZE; red++) {
          const input: RgbTuple = [
            red / (BAKED_CUBE_SIZE - 1),
            green / (BAKED_CUBE_SIZE - 1),
            blue / (BAKED_CUBE_SIZE - 1),
          ]
          const applied = recipe.inputLut ? sample(recipe.inputLut, ...input) : input
          yield input.map((value, channel) => {
            const blended = value * (1 - intensity) + applied[channel]! * intensity
            if (!Number.isFinite(blended)) throw new LutBakerError('unsupported_recipe')
            // Video egress clips to the representable encoded-sRGB domain.
            return Math.max(0, Math.min(1, blended))
          }) as unknown as RgbTuple
        }
      }
    }
  }
  const canonical = encodeCanonicalCube(BAKED_CUBE_SIZE, values())
  return {
    contract: LUT_BAKER_CONTRACT,
    cubeSize: BAKED_CUBE_SIZE,
    canonicalText: canonical.text,
    canonicalBytes: canonical.bytes,
    sha256: canonical.sha256,
  }
}

type Point = { x: number; y: number }
type DecodedCurves = Partial<Record<'master' | 'red' | 'green' | 'blue', Point[]>>
type DecodedEdit = {
  brightness?: number
  contrast?: number
  saturation?: number
  filter?: 'grayscale' | 'sepia'
  curves?: DecodedCurves
}

function strictRecord(value: unknown, allowed: readonly string[]): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new LutBakerError('unsupported_recipe')
  const record = value as Record<string, unknown>
  if (Object.keys(record).some(key => !allowed.includes(key))) throw new LutBakerError('unsupported_recipe')
  return record
}

function decodePublicRecipe(value: unknown): { size: 33; edit: DecodedEdit } {
  const request = strictRecord(value, ['size', 'edit'])
  if (request.size !== 33) throw new LutBakerError('unsupported_recipe')
  const rawEdit = strictRecord(request.edit, ['brightness', 'contrast', 'saturation', 'filter', 'curves'])
  const edit: DecodedEdit = {}
  for (const key of ['brightness', 'contrast', 'saturation'] as const) {
    const candidate = rawEdit[key]
    if (candidate !== undefined) {
      if (typeof candidate !== 'number' || !Number.isFinite(candidate)) throw new LutBakerError('unsupported_recipe')
      edit[key] = candidate
    }
  }
  if (rawEdit.filter !== undefined) {
    if (rawEdit.filter !== 'grayscale' && rawEdit.filter !== 'sepia') throw new LutBakerError('unsupported_recipe')
    edit.filter = rawEdit.filter
  }
  if (rawEdit.curves !== undefined) {
    const rawCurves = strictRecord(rawEdit.curves, ['master', 'red', 'green', 'blue'])
    const curves: DecodedCurves = {}
    for (const channel of ['master', 'red', 'green', 'blue'] as const) {
      const candidate = rawCurves[channel]
      if (candidate === undefined) continue
      if (!Array.isArray(candidate)) throw new LutBakerError('unsupported_recipe')
      curves[channel] = candidate.map(value => {
        const point = strictRecord(value, ['x', 'y'])
        if (typeof point.x !== 'number' || typeof point.y !== 'number') throw new LutBakerError('unsupported_recipe')
        return { x: point.x, y: point.y }
      })
    }
    edit.curves = curves
  }
  return { size: 33, edit }
}

function validateCurve(points: Point[]): void {
  if (points.length < 2 || points.length > 64) throw new LutBakerError('unsupported_recipe')
  let previous = -1
  for (const point of points) {
    if (!Number.isFinite(point.x) || !Number.isFinite(point.y)
      || point.x < 0 || point.x > 1 || point.y < 0 || point.y > 1 || point.x <= previous) {
      throw new LutBakerError('unsupported_recipe')
    }
    previous = point.x
  }
  if (points[0]!.x !== 0 || points.at(-1)!.x !== 1) throw new LutBakerError('unsupported_recipe')
}

function pchipSlopes(points: Point[]): number[] {
  if (points.length === 2) {
    const slope = (points[1]!.y - points[0]!.y) / (points[1]!.x - points[0]!.x)
    return [slope, slope]
  }
  const widths = points.slice(1).map((point, index) => point.x - points[index]!.x)
  const secants = widths.map((width, index) => (points[index + 1]!.y - points[index]!.y) / width)
  const slopes = new Array<number>(points.length).fill(0)
  for (let index = 1; index < points.length - 1; index++) {
    const before = secants[index - 1]!
    const after = secants[index]!
    if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after)) continue
    const firstWeight = 2 * widths[index]! + widths[index - 1]!
    const secondWeight = widths[index]! + 2 * widths[index - 1]!
    slopes[index] = (firstWeight + secondWeight) / (firstWeight / before + secondWeight / after)
  }
  const endpoint = (a: number, b: number, first: number, second: number) => {
    const candidate = ((2 * a + b) * first - a * second) / (a + b)
    if (Math.sign(candidate) !== Math.sign(first)) return 0
    if (Math.sign(first) !== Math.sign(second) && Math.abs(candidate) > 3 * Math.abs(first)) return 3 * first
    return candidate
  }
  slopes[0] = endpoint(widths[0]!, widths[1]!, secants[0]!, secants[1]!)
  const last = widths.length - 1
  slopes[points.length - 1] = endpoint(widths[last]!, widths[last - 1]!, secants[last]!, secants[last - 1]!)
  return slopes
}

function curveValue(points: Point[] | undefined, value: number): number {
  if (!points) return value
  validateCurve(points)
  const upper = Math.min(points.length - 1, Math.max(0, points.findIndex(point => point.x >= value)))
  if (upper === 0) return points[0]!.y
  const index = upper - 1
  const a = points[index]!
  const b = points[upper]!
  const width = b.x - a.x
  const t = (value - a.x) / width
  const slopes = pchipSlopes(points)
  const h00 = (2 * t - 3) * t * t + 1
  const h10 = ((t - 2) * t + 1) * t
  const h01 = (-2 * t + 3) * t * t
  const h11 = (t - 1) * t * t
  return Math.max(0, Math.min(1, h00 * a.y + h10 * width * slopes[index]! + h01 * b.y + h11 * width * slopes[upper]!))
}

/** Browser implementation of the public v1 recipe used by `/api/luts/bake`. */
export function bakeEditCube33(request: LutBakeRequest, signal?: AbortSignal): BakedCubeLut {
  const { edit } = decodePublicRecipe(request)
  const brightness = edit.brightness ?? 0
  const contrast = edit.contrast ?? 1
  const saturation = edit.saturation ?? 1
  if (!Number.isFinite(brightness) || brightness < -1 || brightness > 1
    || !Number.isFinite(contrast) || contrast < 0 || contrast > 3
    || !Number.isFinite(saturation) || saturation < 0 || saturation > 3) {
    throw new LutBakerError('unsupported_recipe')
  }
  const curves = edit.curves
  for (const points of curves ? [curves.master, curves.red, curves.green, curves.blue] : []) {
    if (points) validateCurve(points)
  }
  const values = function* (): Iterable<RgbTuple> {
    for (let blue = 0; blue < 33; blue++) {
      if (signal?.aborted) throw new LutBakerError('cancelled')
      for (let green = 0; green < 33; green++) for (let red = 0; red < 33; red++) {
        let rgb = [red / 32, green / 32, blue / 32].map(value =>
          Math.max(0, Math.min(1, (value - 0.5) * contrast + 0.5 + brightness))) as [number, number, number]
        const luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
        rgb = rgb.map(value => Math.max(0, Math.min(1, luma + (value - luma) * saturation))) as [number, number, number]
        if (edit.filter === 'grayscale') {
          const gray = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
          rgb = [gray, gray, gray]
        } else if (edit.filter === 'sepia') {
          const [r, g, b] = rgb
          rgb = [0.393 * r + 0.769 * g + 0.189 * b, 0.349 * r + 0.686 * g + 0.168 * b, 0.272 * r + 0.534 * g + 0.131 * b]
            .map(value => Math.max(0, Math.min(1, value))) as [number, number, number]
        }
        if (curves) {
          rgb = rgb.map(value => curveValue(curves.master, value)) as [number, number, number]
          rgb = [curveValue(curves.red, rgb[0]), curveValue(curves.green, rgb[1]), curveValue(curves.blue, rgb[2])]
        }
        yield rgb
      }
    }
  }
  const canonical = encodeCanonicalCube(33, values())
  return { contract: LUT_BAKER_CONTRACT, cubeSize: 33, canonicalText: canonical.text, canonicalBytes: canonical.bytes, sha256: canonical.sha256 }
}
