import type { WireAnimatableValue, WireCompositionTrack } from './types'
import { shiftAnimatableValue } from './keyframes'
type NativeClip = WireCompositionTrack['clips'][number]
/** Rebase local presentation curves while retaining off-clip keys and authored source clocks. */
export function rebaseNativeClipAnimation<T extends NativeClip>(clip: T, deltaUs: number): T {
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректный сдвиг времени анимации')
  function copy(value: unknown): unknown {
    if (Array.isArray(value)) return value.map(copy)
    if (!value || typeof value !== 'object') return value
    const object = value as Record<string, unknown>
    if (object.mode === 'constant' || (object.mode === 'keyframes' && object.track)) return shiftAnimatableValue(value as WireAnimatableValue, deltaUs)
    return Object.fromEntries(Object.entries(object).map(([key, entry]) => [key, copy(entry)]))
  }
  // Graphic layers sample their own retained clock; shifting their keys too would double the offset.
  const result = copy('graphic' in clip ? { ...clip, graphic: undefined } : clip) as T
  if ('colorAnimation' in clip && clip.colorAnimation) {
    const keys = clip.colorAnimation.keys.map(key => {
      const time = key.t_us + deltaUs
      if (!Number.isFinite(time) || Math.abs(time) > Number.MAX_SAFE_INTEGER) throw new Error('Время цвета выходит за допустимый диапазон')
      return { ...key, t_us: time }
    })
    Object.assign(result, { colorAnimation: { ...clip.colorAnimation, keys } })
  }
  if ('graphic' in clip) {
    const offset = (clip.animationOffsetUs ?? 0) - deltaUs
    if (!Number.isSafeInteger(offset)) throw new Error('Время графики выходит за допустимый диапазон')
    Object.assign(result, { graphic: clip.graphic, animationOffsetUs: offset })
  }
  return result
}
