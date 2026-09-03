import {
  COMPOSITION_TIME_BASE,
  type CompositionAnimatableValue,
  type CompositionVisualAnimation,
  type CompositionVisualProperty,
  type Tick,
} from './types'

export type CompositionAnimationPreset =
  | 'fade_in'
  | 'fade_out'
  | 'slide_left_in'
  | 'slide_right_in'
  | 'zoom_in'
  | 'zoom_out'
  | 'pulse_loop'
  | 'spin_loop'

export interface AnimationPresetContext {
  readonly clipDurationTicks: Tick
  readonly presetDurationTicks: Tick
  readonly canvasWidth: number
  readonly values: Readonly<Record<CompositionVisualProperty, number>>
}

function track(
  interpolation: 'linear' | 'ease_in' | 'ease_out' | 'ease_in_out',
  keyframes: readonly { tick: Tick; value: number }[],
): CompositionAnimatableValue {
  return { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation, keyframes } }
}

const clamp = (value: number, minimum: number, maximum: number): number => Math.max(minimum, Math.min(maximum, value))

export function buildAnimationPreset(
  preset: CompositionAnimationPreset,
  context: AnimationPresetContext,
): CompositionVisualAnimation {
  const end = context.clipDurationTicks
  const duration = Math.max(1, Math.min(context.presetDurationTicks, end))
  const outStart = Math.max(0, end - duration)
  const value = context.values
  const opacityIn = track('ease_out', [{ tick: 0, value: 0 }, { tick: duration, value: value.opacity }])
  const opacityOut = track('ease_in', [{ tick: outStart, value: value.opacity }, { tick: end, value: 0 }])
  switch (preset) {
    case 'fade_in': return { opacity: opacityIn }
    case 'fade_out': return { opacity: opacityOut }
    case 'slide_left_in': return {
      x: track('ease_out', [{ tick: 0, value: clamp(value.x - context.canvasWidth * 0.35, -32_768, 32_768) }, { tick: duration, value: value.x }]),
      opacity: opacityIn,
    }
    case 'slide_right_in': return {
      x: track('ease_out', [{ tick: 0, value: clamp(value.x + context.canvasWidth * 0.35, -32_768, 32_768) }, { tick: duration, value: value.x }]),
      opacity: opacityIn,
    }
    case 'zoom_in': return {
      scaleX: track('ease_out', [{ tick: 0, value: Math.max(0.01, value.scaleX * 0.25) }, { tick: duration, value: value.scaleX }]),
      scaleY: track('ease_out', [{ tick: 0, value: Math.max(0.01, value.scaleY * 0.25) }, { tick: duration, value: value.scaleY }]),
      opacity: opacityIn,
    }
    case 'zoom_out': return {
      scaleX: track('ease_in', [{ tick: outStart, value: value.scaleX }, { tick: end, value: Math.max(0.01, value.scaleX * 0.25) }]),
      scaleY: track('ease_in', [{ tick: outStart, value: value.scaleY }, { tick: end, value: Math.max(0.01, value.scaleY * 0.25) }]),
      opacity: opacityOut,
    }
    case 'pulse_loop': {
      const keyframes = Array.from({ length: 9 }, (_, index) => ({
        tick: Math.round(end * index / 8),
        value: index % 2 ? 1.12 : 1,
      }))
      return {
        scaleX: track('ease_in_out', keyframes.map((keyframe) => ({ ...keyframe, value: clamp(value.scaleX * keyframe.value, 0.01, 16) }))),
        scaleY: track('ease_in_out', keyframes.map((keyframe) => ({ ...keyframe, value: clamp(value.scaleY * keyframe.value, 0.01, 16) }))),
      }
    }
    case 'spin_loop': {
      const target = value.rotationDegrees <= 3_240 ? value.rotationDegrees + 360 : value.rotationDegrees - 360
      return { rotationDegrees: track('linear', [{ tick: 0, value: value.rotationDegrees }, { tick: end, value: target }]) }
    }
  }
}
