import type { CompositionBezierInterpolation, CompositionInterpolation } from './types'
// Pinned WeftCut MIT preset coordinates, including arithmetic fractions.
const BACK_Y = 1.70158 / 3
export const BEZIER_PRESETS: readonly { id: string; interpolation: CompositionBezierInterpolation }[] = [
  { id: 'ease', interpolation: { kind: 'bezier', p1: [0.25, 0.1], p2: [0.25, 1] } },
  { id: 'ease_in', interpolation: { kind: 'bezier', p1: [0.42, 0], p2: [1, 1] } },
  { id: 'ease_out', interpolation: { kind: 'bezier', p1: [0, 0], p2: [0.58, 1] } },
  { id: 'ease_in_out', interpolation: { kind: 'bezier', p1: [0.42, 0], p2: [0.58, 1] } },
  { id: 'ease_in_sine', interpolation: { kind: 'bezier', p1: [0.12, 0], p2: [0.39, 0] } },
  { id: 'ease_out_sine', interpolation: { kind: 'bezier', p1: [0.61, 1], p2: [0.88, 1] } },
  { id: 'ease_in_out_sine', interpolation: { kind: 'bezier', p1: [0.37, 0], p2: [0.63, 1] } },
  { id: 'ease_in_quad', interpolation: { kind: 'bezier', p1: [1 / 3, 0], p2: [2 / 3, 1 / 3] } },
  { id: 'ease_out_quad', interpolation: { kind: 'bezier', p1: [1 / 3, 2 / 3], p2: [2 / 3, 1] } },
  { id: 'ease_in_out_quad', interpolation: { kind: 'bezier', p1: [0.45, 0], p2: [0.55, 1] } },
  { id: 'ease_in_cubic', interpolation: { kind: 'bezier', p1: [1 / 3, 0], p2: [2 / 3, 0] } },
  { id: 'ease_out_cubic', interpolation: { kind: 'bezier', p1: [1 / 3, 1], p2: [2 / 3, 1] } },
  { id: 'ease_in_out_cubic', interpolation: { kind: 'bezier', p1: [0.65, 0], p2: [0.35, 1] } },
  { id: 'ease_in_quart', interpolation: { kind: 'bezier', p1: [0.5, 0], p2: [0.75, 0] } },
  { id: 'ease_out_quart', interpolation: { kind: 'bezier', p1: [0.25, 1], p2: [0.5, 1] } },
  { id: 'ease_in_out_quart', interpolation: { kind: 'bezier', p1: [0.76, 0], p2: [0.24, 1] } },
  { id: 'ease_in_quint', interpolation: { kind: 'bezier', p1: [0.64, 0], p2: [0.78, 0] } },
  { id: 'ease_out_quint', interpolation: { kind: 'bezier', p1: [0.22, 1], p2: [0.36, 1] } },
  { id: 'ease_in_out_quint', interpolation: { kind: 'bezier', p1: [0.83, 0], p2: [0.17, 1] } },
  { id: 'ease_in_expo', interpolation: { kind: 'bezier', p1: [0.7, 0], p2: [0.84, 0] } },
  { id: 'ease_out_expo', interpolation: { kind: 'bezier', p1: [0.16, 1], p2: [0.3, 1] } },
  { id: 'ease_in_out_expo', interpolation: { kind: 'bezier', p1: [0.87, 0], p2: [0.13, 1] } },
  { id: 'ease_in_circ', interpolation: { kind: 'bezier', p1: [0.55, 0], p2: [1, 0.45] } },
  { id: 'ease_out_circ', interpolation: { kind: 'bezier', p1: [0, 0.55], p2: [0.45, 1] } },
  { id: 'ease_in_out_circ', interpolation: { kind: 'bezier', p1: [0.85, 0], p2: [0.15, 1] } },
  { id: 'ease_in_back', interpolation: { kind: 'bezier', p1: [1 / 3, 0], p2: [2 / 3, -BACK_Y] } },
  { id: 'ease_out_back', interpolation: { kind: 'bezier', p1: [1 / 3, 1 + BACK_Y], p2: [2 / 3, 1] } },
  { id: 'ease_in_out_back', interpolation: { kind: 'bezier', p1: [0.68, -0.6], p2: [0.32, 1.6] } },
]
export function bezierPresetId(value: CompositionInterpolation): string | undefined {
  if (typeof value !== 'object' || value.kind !== 'bezier') return undefined
  return BEZIER_PRESETS.find(p => p.interpolation.p1.every((n,i) => n === value.p1[i]) && p.interpolation.p2.every((n,i) => n === value.p2[i]))?.id
}

export const EASING_PRESETS: readonly { id: string; interpolation: CompositionInterpolation }[] = [
  { id: 'linear',interpolation: 'linear' },
  { id: 'hold',interpolation: 'hold' },
  ...BEZIER_PRESETS,
  ...(['in','out','in_out'] as const).map(dir => ({ id: `ease_${dir}_elastic`,interpolation: { kind: 'elastic' as const,dir,amplitude: 1,period: 0.3 } })),
  ...(['in','out','in_out'] as const).map(dir => ({ id: `ease_${dir}_bounce`,interpolation: `bounce_${dir}` as CompositionInterpolation })),
]

export function easingPresetId(value: CompositionInterpolation): string | undefined {
  if (typeof value === 'object' && value.kind === 'bezier') return bezierPresetId(value)
  return EASING_PRESETS.find(({ interpolation }) => {
    if (typeof value === 'string') return interpolation === value
    return typeof interpolation === 'object' && interpolation.kind === 'elastic' && value.kind === 'elastic'
      && interpolation.dir === value.dir && interpolation.amplitude === value.amplitude && interpolation.period === value.period
  })?.id
}
