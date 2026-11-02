import { sameKeyframeTangents } from './tangents'
import type { CompositionInterpolation } from './types'
import { cloneAnimatableValue, visualPropertyValue } from './keyframes'
import type { Composition, CompositionAnimatableValue, VisualClip } from './types'

/** Compare authored shapes, including outgoing easing, rather than sampled values. */
export function scaleTracksTwins(a: CompositionAnimatableValue, b: CompositionAnimatableValue): boolean {
  if (a.mode !== b.mode) return false
  if (a.mode === 'constant') return b.mode === 'constant' && a.value === b.value
  if (b.mode !== 'keyframes') return false
  return (a.track.extrapolation?.before ?? 'hold') === (b.track.extrapolation?.before ?? 'hold') &&
    (a.track.extrapolation?.after ?? 'hold') === (b.track.extrapolation?.after ?? 'hold') &&
    a.track.timeBase === b.track.timeBase && sameInterpolation(a.track.interpolation, b.track.interpolation) &&
    a.track.keyframes.length === b.track.keyframes.length &&
    a.track.keyframes.every((key, index) => {
      const other = b.track.keyframes[index]!
      return key.tick === other.tick && key.value === other.value && sameKeyframeTangents(key.tangents, other.tangents) && sameInterpolation(key.interpolation, other.interpolation)
    })
}

/** Linking replaces the entire Y track; unlinking preserves both axes. */
export function setVisualScaleLinked(document: Composition, clip: VisualClip, linked: boolean): VisualClip {
  if (!linked) return { ...clip, scaleLinked: false }
  return { ...clip, scaleLinked: true, animation: {
    ...clip.animation,
    scaleY: cloneAnimatableValue(visualPropertyValue(document, clip, 'scaleX')),
  } }
}

/** Run on the final result of a batch so temporary X/Y divergence cannot unlink it. */
export function normalizeVisualScaleLink(document: Composition, clip: VisualClip): VisualClip {
  if (clip.scaleLinked !== true) return clip
  return scaleTracksTwins(visualPropertyValue(document, clip, 'scaleX'), visualPropertyValue(document, clip, 'scaleY'))
    ? clip : { ...clip, scaleLinked: false }
}

function sameInterpolation(a: CompositionInterpolation | undefined, b: CompositionInterpolation | undefined): boolean {
  if (a === b) return true
  if (typeof a !== 'object' || typeof b !== 'object' || a.kind !== b.kind) return false
  if (a.kind === 'elastic' && b.kind === 'elastic') return a.dir === b.dir && a.amplitude === b.amplitude && a.period === b.period
  return a.kind === 'bezier' && b.kind === 'bezier' && a.p1.every((n, i) => n === b.p1[i]) && a.p2.every((n, i) => n === b.p2[i])
}

