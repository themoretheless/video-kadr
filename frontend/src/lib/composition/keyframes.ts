import {
  COMPOSITION_AUDIO_PROPERTIES,
  COMPOSITION_MASK_PROPERTIES,
  COMPOSITION_TIME_BASE,
  COMPOSITION_VISUAL_PROPERTIES,
  MAX_KEYFRAMES_PER_VALUE,
  type AudioClip,
  type Composition,
  type CompositionAudioAnimation,
  type CompositionAudioProperty,
  type CompositionAnimatableValue,
  type CompositionInterpolation,
  type CompositionKeyframe,
  type CompositionMaskProperty,
  type CompositionVideoMask,
  type CompositionVisualAnimation,
  type CompositionVisualProperty,
  type VideoClip,
  type VisualClip,
} from './types'

export interface VisualPropertyBounds {
  readonly minimum: number
  readonly maximum: number
  readonly step: number
}

const PROPERTY_BOUNDS: Record<CompositionVisualProperty, VisualPropertyBounds> = {
  x: { minimum: -32_768, maximum: 32_768, step: 1 },
  y: { minimum: -32_768, maximum: 32_768, step: 1 },
  scaleX: { minimum: 0.01, maximum: 16, step: 0.01 },
  scaleY: { minimum: 0.01, maximum: 16, step: 0.01 },
  rotationDegrees: { minimum: -3_600, maximum: 3_600, step: 1 },
  opacity: { minimum: 0, maximum: 1, step: 0.01 },
}

const AUDIO_PROPERTY_BOUNDS: Record<CompositionAudioProperty, VisualPropertyBounds> = {
  gain: { minimum: 0, maximum: 16, step: 0.05 },
  pan: { minimum: -1, maximum: 1, step: 0.05 },
}

const MASK_PROPERTY_BOUNDS: Record<CompositionMaskProperty, VisualPropertyBounds> = {
  x: { minimum: 0, maximum: 1, step: 0.01 },
  y: { minimum: 0, maximum: 1, step: 0.01 },
  width: { minimum: 0.000_001, maximum: 2, step: 0.01 },
  height: { minimum: 0.000_001, maximum: 2, step: 0.01 },
}

export function visualPropertyBounds(property: CompositionVisualProperty): VisualPropertyBounds {
  return PROPERTY_BOUNDS[property]
}

export function audioPropertyBounds(property: CompositionAudioProperty): VisualPropertyBounds {
  return AUDIO_PROPERTY_BOUNDS[property]
}

export function maskPropertyBounds(property: CompositionMaskProperty): VisualPropertyBounds {
  return MASK_PROPERTY_BOUNDS[property]
}

export function constantAnimatable(value: number): CompositionAnimatableValue {
  return { mode: 'constant', value }
}

export function visualPropertyFallback(
  composition: Composition,
  clip: VisualClip,
  property: CompositionVisualProperty,
): number {
  if (property === 'opacity') return clip.opacity
  if (property === 'rotationDegrees') return clip.rotationDegrees ?? 0
  if (property === 'x') return clip.kind === 'text' ? clip.x : clip.transform.x
  if (property === 'y') return clip.kind === 'text' ? clip.y : clip.transform.y
  if (clip.kind === 'text') return 1
  const source = composition.sources[clip.sourceId]
  const sourceDimension = property === 'scaleX' ? source?.width : source?.height
  const targetDimension = property === 'scaleX' ? clip.transform.width : clip.transform.height
  return sourceDimension ? targetDimension / sourceDimension : 1
}

export function visualPropertyValue(
  composition: Composition,
  clip: VisualClip,
  property: CompositionVisualProperty,
): CompositionAnimatableValue {
  return clip.animation?.[property] ?? constantAnimatable(visualPropertyFallback(composition, clip, property))
}

export function audioPropertyFallback(
  clip: VideoClip | AudioClip,
  property: CompositionAudioProperty,
): number {
  if (property === 'pan') return clip.kind === 'video' ? clip.audioPan ?? 0 : clip.pan ?? 0
  return clip.kind === 'video' ? clip.audioGain : clip.gain
}

export function audioPropertyValue(
  clip: VideoClip | AudioClip,
  property: CompositionAudioProperty,
): CompositionAnimatableValue {
  return clip.audioAnimation?.[property] ?? constantAnimatable(audioPropertyFallback(clip, property))
}

/** Sample authored audio automation in clip-local composition ticks. */
export function sampleAudioProperty(
  clip: VideoClip | AudioClip,
  property: CompositionAudioProperty,
  clipLocalTicks: number,
): number {
  return sampleAnimatableValue(audioPropertyValue(clip, property), clipLocalTicks)
}

export function maskPropertyValue(
  mask: CompositionVideoMask,
  property: CompositionMaskProperty,
): CompositionAnimatableValue {
  return mask[property]
}

export function sampleMaskProperty(
  mask: CompositionVideoMask,
  property: CompositionMaskProperty,
  clipLocalTicks: number,
): number {
  return sampleAnimatableValue(maskPropertyValue(mask, property), clipLocalTicks)
}

export function sampleAnimatableValue(
  value: CompositionAnimatableValue,
  clipLocalTicks: number,
  compositionTimeBase = COMPOSITION_TIME_BASE,
): number {
  if (value.mode === 'constant') return value.value
  const keyframes = value.track.keyframes
  if (!keyframes.length) return 0
  const tick = Math.max(0, Math.round((clipLocalTicks * value.track.timeBase) / compositionTimeBase))
  const first = keyframes[0]!
  if (tick <= first.tick) return first.value
  const last = keyframes.at(-1)!
  if (tick >= last.tick) return last.value
  const nextIndex = keyframes.findIndex((keyframe) => keyframe.tick > tick)
  const left = keyframes[nextIndex - 1]!
  const right = keyframes[nextIndex]!
  const progress = (tick - left.tick) / (right.tick - left.tick)
  const eased = interpolateProgress(value.track.interpolation, progress)
  return left.value + (right.value - left.value) * eased
}

export function sampleVisualProperty(
  composition: Composition,
  clip: VisualClip,
  property: CompositionVisualProperty,
  clipLocalTicks: number,
): number {
  return sampleAnimatableValue(visualPropertyValue(composition, clip, property), clipLocalTicks, composition.timeBase)
}

export function keyframeTickAtLocalTime(
  localTicks: number,
  trackTimeBase: number = COMPOSITION_TIME_BASE,
  compositionTimeBase: number = COMPOSITION_TIME_BASE,
): number {
  return Math.max(0, Math.round((localTicks * trackTimeBase) / compositionTimeBase))
}

export function upsertKeyframe(
  current: CompositionAnimatableValue | undefined,
  tick: number,
  value: number,
  fallback: number,
  interpolation: CompositionInterpolation = 'linear',
): CompositionAnimatableValue {
  const normalizedTick = safeTick(tick)
  const normalizedValue = finiteValue(value)
  const track = current?.mode === 'keyframes'
    ? current.track
    : { timeBase: COMPOSITION_TIME_BASE, interpolation, keyframes: [] }
  const keyframes = [
    ...track.keyframes.filter((keyframe) => keyframe.tick !== normalizedTick),
    { tick: normalizedTick, value: normalizedValue },
  ].sort((left, right) => left.tick - right.tick)
  if (keyframes.length > MAX_KEYFRAMES_PER_VALUE) {
    throw new Error(`Не больше ${MAX_KEYFRAMES_PER_VALUE} ключей на параметр`)
  }
  return {
    mode: 'keyframes',
    track: {
      timeBase: track.timeBase || COMPOSITION_TIME_BASE,
      interpolation: normalizeInterpolation(track.interpolation),
      keyframes: keyframes.length ? keyframes : [{ tick: 0, value: fallback }],
    },
  }
}

export function updateKeyframe(
  current: CompositionAnimatableValue,
  originalTick: number,
  tick: number,
  value: number,
): CompositionAnimatableValue {
  if (current.mode !== 'keyframes') throw new Error('У параметра нет keyframe track')
  if (!current.track.keyframes.some((keyframe) => keyframe.tick === originalTick)) {
    throw new Error('Ключевой кадр не найден')
  }
  const nextTick = safeTick(tick)
  if (nextTick !== originalTick && current.track.keyframes.some((keyframe) => keyframe.tick === nextTick)) {
    throw new Error('На этой позиции уже есть ключевой кадр')
  }
  return {
    mode: 'keyframes',
    track: {
      ...current.track,
      keyframes: current.track.keyframes
        .map((keyframe): CompositionKeyframe => keyframe.tick === originalTick
          ? { tick: nextTick, value: finiteValue(value) }
          : { ...keyframe })
        .sort((left, right) => left.tick - right.tick),
    },
  }
}

export function deleteKeyframe(
  current: CompositionAnimatableValue,
  tick: number,
): CompositionAnimatableValue | undefined {
  if (current.mode !== 'keyframes') return undefined
  const keyframes = current.track.keyframes.filter((keyframe) => keyframe.tick !== tick)
  return keyframes.length
    ? { mode: 'keyframes', track: { ...current.track, keyframes: keyframes.map((keyframe) => ({ ...keyframe })) } }
    : undefined
}

export function updateInterpolation(
  current: CompositionAnimatableValue,
  interpolation: CompositionInterpolation,
): CompositionAnimatableValue {
  if (current.mode !== 'keyframes') throw new Error('Сначала добавьте ключевой кадр')
  return {
    mode: 'keyframes',
    track: { ...current.track, interpolation: normalizeInterpolation(interpolation) },
  }
}

export function cloneAnimatableValue(value: CompositionAnimatableValue): CompositionAnimatableValue {
  return value.mode === 'constant'
    ? { ...value }
    : { mode: 'keyframes', track: { ...value.track, keyframes: value.track.keyframes.map((keyframe) => ({ ...keyframe })) } }
}

export function cloneVisualAnimation(
  animation: CompositionVisualAnimation | undefined,
): CompositionVisualAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_VISUAL_PROPERTIES, cloneAnimatableValue)
}

export function cloneAudioAnimation(
  animation: CompositionAudioAnimation | undefined,
): CompositionAudioAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_AUDIO_PROPERTIES, cloneAnimatableValue)
}

export function cloneVideoMasks(
  masks: readonly CompositionVideoMask[] | undefined,
): readonly CompositionVideoMask[] | undefined {
  return masks?.map((mask) => mapAnimation(mask, COMPOSITION_MASK_PROPERTIES, cloneAnimatableValue)!)
}

/**
 * Keep a clip-local interval while preserving the sampled curve at trimmed
 * boundaries. `startTicks` may be negative and `endTicks` may extend past the
 * old duration when a trim expands a clip.
 */
export function sliceAnimatableValue(
  value: CompositionAnimatableValue,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
  compositionTimeBase = COMPOSITION_TIME_BASE,
): CompositionAnimatableValue {
  if (value.mode === 'constant') return { ...value }
  const track = value.track
  const nextDuration = Math.max(0, endTicks - startTicks)
  const next: CompositionKeyframe[] = []
  const compositionTicks = track.keyframes.map((keyframe) => (keyframe.tick * compositionTimeBase) / track.timeBase)
  for (const keyframe of track.keyframes) {
    const compositionTick = (keyframe.tick * compositionTimeBase) / track.timeBase
    if (compositionTick < Math.max(0, startTicks) || compositionTick > Math.min(sourceDurationTicks, endTicks)) continue
    next.push({
      tick: keyframeTickAtLocalTime(compositionTick - startTicks, track.timeBase, compositionTimeBase),
      value: keyframe.value,
    })
  }
  if (startTicks > 0 && startTicks < sourceDurationTicks && compositionTicks.some((tick) => tick < startTicks)) {
    next.push({ tick: 0, value: sampleAnimatableValue(value, startTicks, compositionTimeBase) })
  }
  if (endTicks >= 0 && endTicks < sourceDurationTicks && compositionTicks.some((tick) => tick > endTicks)) {
    next.push({
      tick: keyframeTickAtLocalTime(nextDuration, track.timeBase, compositionTimeBase),
      value: sampleAnimatableValue(value, endTicks, compositionTimeBase),
    })
  }
  const keyframes = [...new Map(
    next
      .sort((left, right) => left.tick - right.tick)
      .map((keyframe) => [keyframe.tick, keyframe] as const),
  ).values()]
  return {
    mode: 'keyframes',
    track: {
      ...track,
      interpolation: normalizeInterpolation(track.interpolation),
      keyframes: keyframes.length
        ? keyframes
        : [{ tick: 0, value: sampleAnimatableValue(value, Math.max(0, startTicks), compositionTimeBase) }],
    },
  }
}

export function sliceVisualAnimation(
  animation: CompositionVisualAnimation | undefined,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
): CompositionVisualAnimation | undefined {
  return mapAnimation(
    animation,
    COMPOSITION_VISUAL_PROPERTIES,
    (value) => sliceAnimatableValue(value, startTicks, endTicks, sourceDurationTicks),
  )
}

export function sliceAudioAnimation(
  animation: CompositionAudioAnimation | undefined,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
): CompositionAudioAnimation | undefined {
  return mapAnimation(
    animation,
    COMPOSITION_AUDIO_PROPERTIES,
    (value) => sliceAnimatableValue(value, startTicks, endTicks, sourceDurationTicks),
  )
}

export function sliceVideoMasks(
  masks: readonly CompositionVideoMask[] | undefined,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
): readonly CompositionVideoMask[] | undefined {
  return masks?.map((mask) => mapAnimation(
    mask,
    COMPOSITION_MASK_PROPERTIES,
    (value) => sliceAnimatableValue(value, startTicks, endTicks, sourceDurationTicks),
  )!)
}

function mapAnimation<T extends object>(
  animation: T | undefined,
  properties: readonly string[],
  transform: (value: CompositionAnimatableValue) => CompositionAnimatableValue,
): T | undefined {
  if (!animation) return undefined
  const mapped = { ...animation } as unknown as Record<string, unknown>
  const source = animation as unknown as Record<string, unknown>
  for (const property of properties) {
    const value = source[property] as CompositionAnimatableValue | undefined
    if (value) mapped[property] = transform(value)
  }
  return mapped as unknown as T
}

export function keyframeCount(value: CompositionAnimatableValue | undefined): number {
  return value?.mode === 'keyframes' ? value.track.keyframes.length : 0
}

export function animatableExtents(value: CompositionAnimatableValue): { minimum: number; maximum: number } {
  const values = value.mode === 'constant' ? [value.value] : value.track.keyframes.map((keyframe) => keyframe.value)
  return { minimum: Math.min(...values), maximum: Math.max(...values) }
}

export function normalizeInterpolation(value: CompositionInterpolation): Exclude<CompositionInterpolation, 'ease_in_out_cubic'> {
  return value === 'ease_in_out_cubic' ? 'ease_in_out' : value
}

function interpolateProgress(interpolation: CompositionInterpolation, progress: number): number {
  switch (interpolation) {
    case 'hold': return 0
    case 'linear': return progress
    case 'ease_in': return progress ** 3
    case 'ease_out': return 1 - (1 - progress) ** 3
    case 'ease_in_out':
    case 'ease_in_out_cubic':
      return progress < 0.5 ? 4 * progress ** 3 : 1 - ((-2 * progress + 2) ** 3) / 2
  }
}

function safeTick(value: number): number {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error('Keyframe tick должен быть неотрицательным целым')
  return value
}

function finiteValue(value: number): number {
  if (!Number.isFinite(value)) throw new Error('Значение keyframe должно быть конечным числом')
  return value
}
