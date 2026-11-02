import type { CompositionBezierInterpolation } from './types'

export function isBezierInterpolation(value: unknown): value is CompositionBezierInterpolation {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  const point = (p: unknown) => Array.isArray(p) && p.length === 2 && p.every((n) => typeof n === 'number' && Number.isFinite(n)) && p[0] >= 0 && p[0] <= 1
  return candidate.kind === 'bezier' && Object.keys(candidate).every((key) => ['kind', 'p1', 'p2'].includes(key)) && point(candidate.p1) && point(candidate.p2) && coefficients(candidate as unknown as CompositionBezierInterpolation, 1).every(Number.isFinite)
}

function coefficients(value: CompositionBezierInterpolation, axis: 0 | 1): [number, number, number] {
  const c = 3 * value.p1[axis]
  const b = 3 * (value.p2[axis] - value.p1[axis]) - c
  return [1 - c - b, b, c]
}
function coordinate(value: CompositionBezierInterpolation, axis: 0 | 1, t: number): number {
  const [a, b, c] = coefficients(value, axis)
  return ((a * t + b) * t + c) * t
}
export function sampleBezier(value: CompositionBezierInterpolation, progress: number): number {
  if (progress === 0 || progress === 1) return progress
  const [a, b, c] = coefficients(value, 0)
  let t = progress
  for (let i = 0; i < 8; i++) {
    const error = coordinate(value, 0, t) - progress
    if (Math.abs(error) < 1e-7) return coordinate(value, 1, t)
    const derivative = (3 * a * t + 2 * b) * t + c
    if (Math.abs(derivative) < 1e-6) break
    t -= error / derivative
  }
  let lo = 0, hi = 1
  t = progress
  while (lo < hi) {
    const at = coordinate(value, 0, t)
    if (Math.abs(at - progress) < 1e-7) break
    if (progress > at) lo = t
    else hi = t
    const next = (hi - lo) * 0.5 + lo
    if (next === t) break
    t = next
  }
  return coordinate(value, 1, t)
}
export function bezierBounds(value: CompositionBezierInterpolation): [number, number] {
  const [rawA, rawB, rawC] = coefficients(value, 1)
  const scale = Math.max(Math.abs(rawA), Math.abs(rawB), Math.abs(rawC))
  const a = 3 * (rawA / scale), b = 2 * (rawB / scale), c = rawC / scale
  const bounds: [number, number] = [0, 1]
  const include = (root: number) => {
    if (root > 0 && root < 1) {
      const valueAt = coordinate(value, 1, root)
      bounds[0] = Math.min(bounds[0], valueAt); bounds[1] = Math.max(bounds[1], valueAt)
    }
  }
  if (a === 0) { if (b !== 0) include(-c / b) }
  else {
    const discriminant = b * b - 4 * a * c
    if (discriminant >= 0) {
      const q = -0.5 * (b + (b < 0 ? -1 : 1) * Math.sqrt(discriminant))
      if (q === 0) include(-b / (2 * a))
      else { include(q / a); include(c / q) }
    }
  }
  return bounds
}
export function bezierEndpointSlope(value: CompositionBezierInterpolation, before: boolean): number {
  let d = before ? value.p1 : [1 - value.p2[0], 1 - value.p2[1]]
  if (d[0] === 0 && d[1] === 0) d = before ? value.p2 : [1 - value.p1[0], 1 - value.p1[1]]
  if (d[0] === 0 && d[1] === 0) d = [1, 1]
  return d[0]! <= 0 ? 0 : d[1]! / d[0]!
}
