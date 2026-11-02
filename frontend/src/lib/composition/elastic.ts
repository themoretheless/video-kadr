import type { CompositionElasticInterpolation } from './types'

export function isElasticInterpolation(value: unknown): value is CompositionElasticInterpolation {
  if (!value || typeof value !== 'object') return false
  const candidate = value as Record<string, unknown>
  return Object.keys(candidate).every((key) => ['kind', 'dir', 'amplitude', 'period'].includes(key)) && candidate.kind === 'elastic' &&
    ['in', 'out', 'in_out'].includes(String(candidate.dir)) && typeof candidate.amplitude === 'number' &&
    Number.isFinite(candidate.amplitude) && candidate.amplitude >= 1 && typeof candidate.period === 'number' &&
    Number.isFinite(candidate.period) && candidate.period > 0 && Number.isFinite(2 * Math.PI / candidate.period)
}

export function sampleElastic(value: CompositionElasticInterpolation, t: number): number {
  const phase = value.period / (2 * Math.PI) * Math.asin(1 / value.amplitude)
  const inward = (at: number) => -(value.amplitude * 2 ** (10 * (at - 1)) * Math.sin((at - 1 - phase) * 2 * Math.PI / value.period))
  const out = (at: number) => value.amplitude * 2 ** (-10 * at) * Math.sin((at - phase) * 2 * Math.PI / value.period) + 1
  if (value.dir === 'in') return inward(t)
  if (value.dir === 'out') return out(t)
  return t < 0.5 ? 0.5 * inward(2 * t) : 0.5 + 0.5 * out(2 * t - 1)
}

/** Twin of native Elastic::bounds: stationary extrema, independent of cycle count. */
export function elasticBounds(value: CompositionElasticInterpolation, start: number, end: number): [number, number] {
  if (!isElasticInterpolation(value) || !Number.isFinite(start) || !Number.isFinite(end) || start < 0 || end > 1 || start > end) throw new Error('Invalid Elastic interval')
  let bounds: [number, number]
  if (value.dir === 'in_out') {
    bounds = [Infinity, -Infinity]
    if (start < 0.5) {
      const [lo, hi] = branchBounds({ ...value, dir: 'in' }, 2 * start, 2 * Math.min(end, 0.5))
      bounds = [0.5 * lo, 0.5 * hi]
    }
    if (end >= 0.5) {
      const [lo, hi] = branchBounds({ ...value, dir: 'out' }, 2 * Math.max(start, 0.5) - 1, 2 * end - 1)
      bounds = [Math.min(bounds[0], 0.5 + 0.5 * lo), Math.max(bounds[1], 0.5 + 0.5 * hi)]
    }
  } else bounds = branchBounds(value, start, end)
  if (start === 0) bounds = [Math.min(bounds[0], 0), Math.max(bounds[1], 0)]
  if (end === 1) bounds = [Math.min(bounds[0], 1), Math.max(bounds[1], 1)]
  return bounds
}

function branchBounds(value: CompositionElasticInterpolation, start: number, end: number): [number, number] {
  const a = sampleElastic(value, start), b = sampleElastic(value, end)
  const bounds: [number, number] = [Math.min(a, b), Math.max(a, b)]
  const omega = 2 * Math.PI / value.period
  const shift = value.dir === 'in' ? 1 : 0
  const k = (value.dir === 'in' ? 10 : -10) * Math.LN2
  const phase = Math.asin(1 / value.amplitude)
  const root = Math.atan2(-omega, k)
  const first = Math.ceil(((start - shift) * omega - phase - root) / Math.PI)
  const last = Math.floor(((end - shift) * omega - phase - root) / Math.PI)
  for (const cycle of [first, first + 1, last - 1, last]) {
    if (cycle < first || cycle > last) continue
    const at = shift + (root + phase + cycle * Math.PI) / omega
    if (at < start || at > end) continue
    const sample = sampleElastic(value, at)
    bounds[0] = Math.min(bounds[0], sample)
    bounds[1] = Math.max(bounds[1], sample)
  }
  return bounds
}
