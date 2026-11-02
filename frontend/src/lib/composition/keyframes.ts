import { sampleClockTick, shiftSampleClock } from './keyframeClock'
import { segmentInterpolation, cloneInterpolation, cloneKeyframe, normalizeKeyframeTangents, resolvedKeyframeTangents } from './tangents'
import { sampleBezier, bezierBounds, bezierEndpointSlope, isBezierInterpolation } from './bezier'
import { sampleElastic, elasticBounds, isElasticInterpolation } from './elastic'
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
  rotationDegrees: { minimum: -180, maximum: 180, step: 1 },
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
  const source = (clip.kind === 'color' || clip.kind === 'graphic') ? { width: clip.width,height: clip.height } : composition.sources[clip.sourceId]
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
  return mask[property] ?? constantAnimatable(0)
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
  compositionTimeBase: number = COMPOSITION_TIME_BASE,
): number {
  if (value.mode === 'constant') return value.value
  const tick = value.track.sampleClock ? sampleClockTick(value.track, clipLocalTicks, compositionTimeBase) : Math.round((clipLocalTicks * value.track.timeBase) / compositionTimeBase)
  return sampleAuthoredTick(value, tick)
}
function sampleAuthoredTick(value: Extract<CompositionAnimatableValue, { mode: 'keyframes' }>, tick: number): number {
  const keyframes = value.track.keyframes
  if (!keyframes.length) return 0
  const first = keyframes[0]!
  const last = keyframes.at(-1)!
  if (tick < first.tick || tick > last.tick) {
    const before = tick < first.tick
    const end = before ? first : last
    const extrapolation = value.track.extrapolation?.[before ? 'before' : 'after'] ?? 'hold'
    if (keyframes.length < 2 || extrapolation === 'hold') return end.value
    if (extrapolation === 'loop' || extrapolation === 'ping_pong' || extrapolation === 'offset') {
      const period = last.tick - first.tick
      const relative = tick - first.tick
      const cycle = Math.floor(relative / period)
      const remainder = relative - cycle * period
      const folded = extrapolation === 'ping_pong' && ((cycle % 2) + 2) % 2 === 1 ? last.tick - remainder : first.tick + remainder
      const sampled = sampleAuthoredTick(value, folded)
      return sampled + (extrapolation === 'offset' ? cycle * (last.value - first.value) : 0)
    }
    const left = before ? first : keyframes[keyframes.length - 2]!
    const right = before ? keyframes[1]! : last
    const mode = segmentInterpolation(left, right, value.track.interpolation)
    const slope = typeof mode === 'object' && mode.kind === 'bezier' ? bezierEndpointSlope(mode, before) : mode === 'linear' ? 1 : mode === 'ease_in' && !before || mode === 'ease_out' && before ? 3 : 0
    return end.value + slope * (right.value - left.value) * (tick - end.tick) / (right.tick - left.tick)
  }
  if (tick === last.tick) return last.value
  const nextIndex = keyframes.findIndex((keyframe) => keyframe.tick > tick)
  const left = keyframes[nextIndex - 1]!
  const right = keyframes[nextIndex]!
  if (tick === left.tick) return left.value
  const progress = (tick - left.tick) / (right.tick - left.tick)
  const eased = interpolateProgress(segmentInterpolation(left, right, value.track.interpolation), progress)
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
  track?: import('./types').CompositionKeyframeTrack,
): number {
  if (track?.sampleClock) return sampleClockTick(track, localTicks, compositionTimeBase)
  return Math.max(0, Math.round((localTicks * trackTimeBase) / compositionTimeBase))
}

export function upsertKeyframe(
  current: CompositionAnimatableValue | undefined,
  tick: number,
  value: number,
  fallback: number,
  interpolation?: CompositionInterpolation,
): CompositionAnimatableValue {
  const normalizedTick = safeTick(tick)
  const normalizedValue = finiteValue(value)
  const track = current?.mode === 'keyframes'
    ? current.track
    : { timeBase: COMPOSITION_TIME_BASE, interpolation: interpolation ?? 'linear', keyframes: [] }
  const existing = track.keyframes.find((keyframe) => keyframe.tick === normalizedTick)
  const preceding = track.keyframes.filter((keyframe) => keyframe.tick < normalizedTick).at(-1)
  const following = track.keyframes.find(keyframe => keyframe.tick > normalizedTick)
  const inheritedTangents = !existing && preceding && (preceding.tangents || following?.tangents)
    ? {
      incoming: following ? resolvedKeyframeTangents(track.keyframes,track.keyframes.indexOf(following),track.interpolation).incoming : { x: 2/3,y: 2/3,mode: 'free' as const },
      outgoing: resolvedKeyframeTangents(track.keyframes,track.keyframes.indexOf(preceding),track.interpolation).outgoing,
      continuity: 'broken' as const,
    }
    : undefined
  const inherited = preceding?.interpolation ?? (!preceding && track.interpolation !== 'linear' ? 'linear' : undefined)
  const next: CompositionKeyframe = {
    ...(existing ?? (inherited === undefined ? {} : { interpolation: inherited })),
    ...(inheritedTangents ? { tangents: inheritedTangents } : {}),
    ...(interpolation === undefined ? {} : { interpolation: normalizeInterpolation(interpolation) }),
    tick: normalizedTick,
    value: normalizedValue,
  }
  const keyframes = [
    ...track.keyframes.filter((keyframe) => keyframe.tick !== normalizedTick),
    next,
  ].sort((left, right) => left.tick - right.tick)
  if (keyframes.length > MAX_KEYFRAMES_PER_VALUE) {
    throw new Error(`Не больше ${MAX_KEYFRAMES_PER_VALUE} ключей на параметр`)
  }
  const result: CompositionAnimatableValue = {
    mode: 'keyframes',
    track: {
      ...track,
      timeBase: track.timeBase || COMPOSITION_TIME_BASE,
      interpolation: normalizeInterpolation(track.interpolation),
      keyframes: normalizeKeyframeTangents(keyframes.length ? keyframes : [{ tick: 0, value: fallback }], track.interpolation),
    },
  }
  return typeof interpolation === 'object' && interpolation.kind === 'bezier'
    ? updateSelectedKeyframeInterpolation(result,[normalizedTick],interpolation)
    : result
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
  const ordered = current.track.keyframes
    .map((keyframe): CompositionKeyframe => keyframe.tick === originalTick
      ? { ...keyframe, tick: nextTick, value: finiteValue(value) }
      : { ...keyframe })
    .sort((left, right) => left.tick - right.tick)
  const keyframes = new Map<number, CompositionKeyframe>()
  for (const keyframe of ordered) keyframes.set(keyframe.tick, keyframe)
  return {
    mode: 'keyframes',
    track: {
      ...current.track,
      keyframes: normalizeKeyframeTangents([...keyframes.values()], current.track.interpolation),
    },
  }
}

export function deleteKeyframe(
  current: CompositionAnimatableValue,
  tick: number,
): CompositionAnimatableValue | undefined {
  if (current.mode !== 'keyframes') return current
  const removed = current.track.keyframes.find((keyframe) => keyframe.tick === tick)
  if (!removed) return current
  const keyframes = current.track.keyframes.filter((keyframe) => keyframe.tick !== tick)
  return keyframes.length
    ? { mode: 'keyframes', track: { ...current.track, keyframes: normalizeKeyframeTangents(keyframes.map(cloneKeyframe), current.track.interpolation) } }
    : constantAnimatable(removed.value)
}

export function updateInterpolation(
  current: CompositionAnimatableValue,
  interpolation: CompositionInterpolation,
): CompositionAnimatableValue {
  if (current.mode !== 'keyframes') throw new Error('Сначала добавьте ключевой кадр')
  const mode = normalizeInterpolation(interpolation)
  const inheritedTicks = current.track.keyframes.slice(0, -1).filter(key => key.interpolation === undefined).map(key => key.tick)
  const baked = typeof mode === 'object' && mode.kind === 'bezier' && inheritedTicks.length
    ? updateSelectedKeyframeInterpolation(current, inheritedTicks, mode)
    : current
  if (baked.mode !== 'keyframes') throw new Error('Сначала добавьте ключевой кадр')
  const keys = baked.track.keyframes.map(key => {
    if (!inheritedTicks.includes(key.tick)) return key
    const inherited = { ...key }
    delete inherited.interpolation
    return inherited
  })
  return {
    mode: 'keyframes',
    track: { ...current.track, interpolation: mode, keyframes: normalizeKeyframeTangents(keys, mode) },
  }
}

export function cloneAnimatableValue(value: CompositionAnimatableValue): CompositionAnimatableValue {
  return value.mode === 'constant'
    ? { ...value }
    : { mode: 'keyframes', track: { ...value.track, interpolation: cloneInterpolation(value.track.interpolation), ...(value.track.extrapolation ? { extrapolation: { ...value.track.extrapolation } } : {}), ...(value.track.sampleClock ? { sampleClock: { ...value.track.sampleClock } } : {}), keyframes: value.track.keyframes.map(cloneKeyframe) } }
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
export function splitAnimatableValue(value: CompositionAnimatableValue, splitTicks: number, right: boolean): CompositionAnimatableValue {
  // A key outside a fragment still defines the interpolation/extrapolation
  // inside it. Keep that context, changing only the right fragment's clock.
  return right ? shiftAnimatableValue(value, -splitTicks) : cloneAnimatableValue(value)
}

export function splitVisualAnimation(animation: CompositionVisualAnimation | undefined, cut: number, right: boolean): CompositionVisualAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_VISUAL_PROPERTIES, value => splitAnimatableValue(value, cut, right))
}
export function splitAudioAnimation(animation: CompositionAudioAnimation | undefined, cut: number, right: boolean): CompositionAudioAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_AUDIO_PROPERTIES, value => splitAnimatableValue(value, cut, right))
}
export function splitVideoMasks(masks: readonly CompositionVideoMask[] | undefined, cut: number, right: boolean): readonly CompositionVideoMask[] | undefined {
  return masks?.map(mask => mapAnimation(mask, COMPOSITION_MASK_PROPERTIES, value => splitAnimatableValue(value, cut, right))!)
}

export function shiftAnimatableValue(value: CompositionAnimatableValue, deltaTicks: number, compositionTimeBase: number = COMPOSITION_TIME_BASE): CompositionAnimatableValue {
  const cloned = cloneAnimatableValue(value)
  if (cloned.mode === 'constant') return cloned
  if (cloned.track.sampleClock) return { ...cloned, track: { ...cloned.track, sampleClock: shiftSampleClock(cloned.track.sampleClock, deltaTicks, compositionTimeBase) } }
  const delta = Math.round(deltaTicks * cloned.track.timeBase / compositionTimeBase)
  return { ...cloned, track: { ...cloned.track, keyframes: cloned.track.keyframes.map(key => ({ ...key, tick: safeTick(key.tick + delta) })) } }
}

export function shiftVisualAnimation(animation: CompositionVisualAnimation | undefined, deltaTicks: number): CompositionVisualAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_VISUAL_PROPERTIES, value => shiftAnimatableValue(value, deltaTicks))
}
export function shiftAudioAnimation(animation: CompositionAudioAnimation | undefined, deltaTicks: number): CompositionAudioAnimation | undefined {
  return mapAnimation(animation, COMPOSITION_AUDIO_PROPERTIES, value => shiftAnimatableValue(value, deltaTicks))
}
export function shiftVideoMasks(masks: readonly CompositionVideoMask[] | undefined, deltaTicks: number): readonly CompositionVideoMask[] | undefined {
  return masks?.map(mask => mapAnimation(mask, COMPOSITION_MASK_PROPERTIES, value => shiftAnimatableValue(value, deltaTicks))!)
}

export function sliceAnimatableValue(
  value: CompositionAnimatableValue,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
  compositionTimeBase = COMPOSITION_TIME_BASE,
): CompositionAnimatableValue {
  if (value.mode === 'constant') return { ...value }
  if (value.track.sampleClock) return shiftAnimatableValue(value, -startTicks, compositionTimeBase)
  const track = value.track
  const nextDuration = Math.max(0, endTicks - startTicks)
  const next: CompositionKeyframe[] = []
  const compositionTicks = track.keyframes.map((keyframe) => (keyframe.tick * compositionTimeBase) / track.timeBase)
  for (const keyframe of track.keyframes) {
    const compositionTick = (keyframe.tick * compositionTimeBase) / track.timeBase
    if (compositionTick < Math.max(0, startTicks) || compositionTick > Math.min(sourceDurationTicks, endTicks)) continue
    next.push({
      ...keyframe,
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
  if (value.mode === 'keyframes') {
    for (let index = 0; index < value.track.keyframes.length - 1; index++) {
      const left = value.track.keyframes[index]!
      const right = value.track.keyframes[index + 1]!
      const interpolation = segmentInterpolation(left, right, value.track.interpolation)
      if (typeof interpolation === 'object') {
        const [lo, hi] = interpolation.kind === 'elastic' ? elasticBounds(interpolation, 0, 1) : bezierBounds(interpolation)
        values.push(left.value + (right.value - left.value) * lo, left.value + (right.value - left.value) * hi)
      }
    }
  }
  return { minimum: Math.min(...values), maximum: Math.max(...values) }
}

export function normalizeInterpolation(value: CompositionInterpolation): Exclude<CompositionInterpolation, 'ease_in_out_cubic'> {
  if (typeof value === 'object' && !isElasticInterpolation(value) && !isBezierInterpolation(value)) throw new Error('Invalid curve parameters')
  return value === 'ease_in_out_cubic' ? 'ease_in_out' : value
}

// WeftCut Penner Bounce: four parabolas, with zero Continue endpoint slope.
function bounceOut(t: number): number {
  if (t < 1 / 2.75) return 7.5625 * t * t
  if (t < 2 / 2.75) { t -= 1.5 / 2.75; return 7.5625 * t * t + 0.75 }
  if (t < 2.5 / 2.75) { t -= 2.25 / 2.75; return 7.5625 * t * t + 0.9375 }
  t -= 2.625 / 2.75
  return 7.5625 * t * t + 0.984375
}

function interpolateProgress(interpolation: CompositionInterpolation, progress: number): number {
  if (typeof interpolation === 'object') return interpolation.kind === 'elastic' ? sampleElastic(interpolation, progress) : sampleBezier(interpolation, progress)
  switch (interpolation) {
    case 'bounce_out': return bounceOut(progress)
    case 'bounce_in': return 1 - bounceOut(1 - progress)
    case 'bounce_in_out': return progress < 0.5 ? 0.5 * (1 - bounceOut(1 - 2 * progress)) : 0.5 + 0.5 * bounceOut(2 * progress - 1)
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
  if (!Number.isSafeInteger(value)) throw new Error('Keyframe tick должен быть безопасным целым')
  return value
}

function finiteValue(value: number): number {
  if (!Number.isFinite(value)) throw new Error('Значение keyframe должно быть конечным числом')
  return value
}

/** Change only the selected keys' outgoing interpolation, retaining all other segments. */
export function updateSelectedKeyframeInterpolation(value: CompositionAnimatableValue, ticks: readonly number[], interpolation: CompositionInterpolation): CompositionAnimatableValue {
  if (value.mode !== 'keyframes' || !ticks.length || ticks.some((tick) => !value.track.keyframes.some((key) => key.tick === tick))) throw new Error('Selected keyframe no longer exists')
  const mode = normalizeInterpolation(interpolation)
  const keys = value.track.keyframes.map((key) => ticks.includes(key.tick) ? { ...key, interpolation: mode } : { ...key })
  if (typeof mode === 'object' && mode.kind === 'bezier') {
    for (const tick of ticks) {
      const index = keys.findIndex(key => key.tick === tick)
      const left = keys[index]!, right = keys[index+1]
      if (!left.tangents && !right?.tangents) continue
      const l = resolvedKeyframeTangents(keys,index,value.track.interpolation)
      keys[index] = { ...left,tangents: { ...l,outgoing: { x: mode.p1[0],y: mode.p1[1],mode: 'free' } } }
      if (right) {
        const r = resolvedKeyframeTangents(keys,index+1,value.track.interpolation)
        keys[index+1] = { ...right,tangents: { ...r,incoming: { x: mode.p2[0],y: mode.p2[1],mode: 'free' } } }
      }
    }
  }
  return { ...value, track: { ...value.track, keyframes: normalizeKeyframeTangents(keys, value.track.interpolation) } }
}
