import {mediaVersionSourceIds,type Composition,type CompositionClip,type CompositionSource} from './types'
import { assertValidComposition, isStableId } from './validation'

export type CompositionRelinkErrorCode =
  | 'missing-source'
  | 'invalid-source'
  | 'incompatible-kind'
  | 'missing-audio'
  | 'source-too-short'
  | 'conflicting-source'

export class CompositionRelinkError extends Error {
  constructor(
    readonly code: CompositionRelinkErrorCode,
    message: string,
  ) {
    super(message)
    this.name = 'CompositionRelinkError'
  }
}

export interface CompositionRelinkRequirements {
  readonly sourceId: string
  readonly kind: CompositionSource['kind']
  readonly minimumDurationTicks: number
  readonly requiresAudio: boolean
  readonly referencedClipIds: readonly string[]
}

export interface CompositionRelinkCompatibility {
  readonly compatible: boolean
  readonly reason: string | null
}

/** Derive requirements from actual clip ranges instead of trusting stale local media metadata. */
export function compositionRelinkRequirements(
  composition: Composition,
  sourceId: string,
): CompositionRelinkRequirements {
  assertValidComposition(composition)
  const source = composition.sources[sourceId]
  if (!source) throw relinkError('missing-source', `Source ${sourceId} does not exist`)

  let minimumDurationTicks = 0
  let requiresAudio = false
  const referencedClipIds: string[] = []
  for (const track of composition.tracks) {
    for (const clip of track.clips) {
      if (!('sourceId' in clip) || clip.sourceId !== sourceId && !mediaVersionSourceIds(composition.metadata?.mediaVersions?.[clip.id]).includes(sourceId)) continue
      referencedClipIds.push(clip.id)
      if (clip.kind === 'video' || clip.kind === 'audio') {
        minimumDurationTicks = Math.max(minimumDurationTicks, clip.sourceOutTicks)
      }
      if (
        clip.kind === 'audio' ||
        (clip.kind === 'video' && clip.sourceAudioEnabled && clip.speedRamp?.audioPolicy !== 'mute')
      ) {
        requiresAudio = true
      }
    }
  }

  return {
    sourceId,
    kind: source.kind,
    minimumDurationTicks,
    requiresAudio,
    referencedClipIds,
  }
}

export function compositionRelinkCompatibility(
  requirements: CompositionRelinkRequirements,
  candidate: CompositionSource,
): CompositionRelinkCompatibility {
  if (!isStableId(candidate.id)) {
    return { compatible: false, reason: 'У нового медиа некорректный id.' }
  }
  if (candidate.kind !== requirements.kind) {
    return { compatible: false, reason: `Нужен источник типа ${requirements.kind}.` }
  }
  if (requirements.requiresAudio && !candidate.hasAudio) {
    return { compatible: false, reason: 'Используемые клипы требуют аудиодорожку.' }
  }
  if (candidate.kind !== 'image' && candidate.durationTicks < requirements.minimumDurationTicks) {
    return {
      compatible: false,
      reason: `Источник короче используемого диапазона ${formatTicks(requirements.minimumDurationTicks)}.`,
    }
  }
  return { compatible: true, reason: null }
}

export function compatibleCompositionRelinkSources(
  composition: Composition,
  sourceId: string,
  candidates: readonly CompositionSource[],
): CompositionSource[] {
  const requirements = compositionRelinkRequirements(composition, sourceId)
  return candidates.filter(
    (candidate) =>
      candidate.id !== sourceId &&
      compositionRelinkCompatibility(requirements, candidate).compatible,
  )
}

/**
 * Replace one persisted source id everywhere without changing clip timing or authoring metadata.
 * An already-registered target is allowed only when its probed metadata is identical.
 */
export function relinkCompositionSource(
  composition: Composition,
  sourceId: string,
  replacement: CompositionSource,
): Composition {
  const requirements = compositionRelinkRequirements(composition, sourceId)
  const compatibility = compositionRelinkCompatibility(requirements, replacement)
  if (!compatibility.compatible) throw compatibilityError(requirements, replacement, compatibility.reason)
  if (replacement.id === sourceId) return composition

  const registered = composition.sources[replacement.id]
  if (registered && !sameSourceMetadata(registered, replacement)) {
    throw relinkError(
      'conflicting-source',
      `Source ${replacement.id} is already registered with different metadata`,
    )
  }

  const sources: Record<string, CompositionSource> = {}
  for (const [id, source] of Object.entries(composition.sources)) {
    if (id === sourceId) {
      if (!registered) sources[replacement.id] = { ...replacement }
      continue
    }
    sources[id] = source
  }

  const tracks = composition.tracks.map((track) => {
    let changed = false
    const clips = track.clips.map((clip) => {
      if (!('sourceId' in clip) || clip.sourceId !== sourceId) return clip
      changed = true
      if(clip.kind==='video'){const {concatCutout:_cutout,...unbound}=clip;return {...unbound,sourceId:replacement.id}}
      return { ...clip, sourceId: replacement.id } as CompositionClip
    })
    return changed ? { ...track, clips } as typeof track : track
  })
  const mediaVersions = composition.metadata?.mediaVersions && Object.fromEntries(Object.entries(composition.metadata.mediaVersions).map(([id, pair]) => [id, { ...pair, ...(pair.videoCutout?{videoCutout:{...pair.videoCutout,corrections:pair.videoCutout.corrections.map(edit=>({...edit,correction:{...edit.correction,strokes:edit.correction.strokes.map(stroke=>stroke.region?{...stroke,region:{...stroke.region,sourceId:stroke.region.sourceId===sourceId?replacement.id:stroke.region.sourceId,maskSourceId:stroke.region.maskSourceId===sourceId?replacement.id:stroke.region.maskSourceId}}:stroke)}})),...(pair.videoCutout.trackedRegions?{trackedRegions:pair.videoCutout.trackedRegions.map(region=>({...region,sourceId:region.sourceId===sourceId?replacement.id:region.sourceId,maskSourceId:region.maskSourceId===sourceId?replacement.id:region.maskSourceId}))}:{}),sourceId:pair.videoCutout.sourceId===sourceId?replacement.id:pair.videoCutout.sourceId,maskSourceId:pair.videoCutout.maskSourceId===sourceId?replacement.id:pair.videoCutout.maskSourceId}}:{}), ...(pair.cutout?{cutout:{...pair.cutout,sourceId:pair.cutout.sourceId===sourceId?replacement.id:pair.cutout.sourceId,maskSourceId:pair.cutout.maskSourceId===sourceId?replacement.id:pair.cutout.maskSourceId,correction:{...pair.cutout.correction,strokes:pair.cutout.correction.strokes.map(stroke=>stroke.region?{...stroke,region:{...stroke.region,sourceId:stroke.region.sourceId===sourceId?replacement.id:stroke.region.sourceId,maskSourceId:stroke.region.maskSourceId===sourceId?replacement.id:stroke.region.maskSourceId}}:stroke)}}}:{}), originalSourceId: pair.originalSourceId === sourceId ? replacement.id : pair.originalSourceId, copySourceId: pair.copySourceId === sourceId ? replacement.id : pair.copySourceId }]))
  const relinked = { ...composition, sources, tracks, ...(mediaVersions ? { metadata: { ...composition.metadata, mediaVersions } } : {}) } as Composition
  assertValidComposition(relinked)
  return relinked
}

function compatibilityError(
  requirements: CompositionRelinkRequirements,
  replacement: CompositionSource,
  reason: string | null,
): CompositionRelinkError {
  if (replacement.kind !== requirements.kind) {
    return relinkError('incompatible-kind', reason ?? 'Replacement source has an incompatible kind')
  }
  if (requirements.requiresAudio && !replacement.hasAudio) {
    return relinkError('missing-audio', reason ?? 'Replacement source has no required audio')
  }
  if (replacement.kind !== 'image' && replacement.durationTicks < requirements.minimumDurationTicks) {
    return relinkError('source-too-short', reason ?? 'Replacement source is too short')
  }
  return relinkError('invalid-source', reason ?? 'Replacement source metadata is invalid')
}

function sameSourceMetadata(left: CompositionSource, right: CompositionSource): boolean {
  return left.id === right.id &&
    left.kind === right.kind &&
    left.durationTicks === right.durationTicks &&
    left.width === right.width &&
    left.height === right.height &&
    left.hasAudio === right.hasAudio
}

function formatTicks(ticks: number): string {
  return `${(ticks / 1_000_000).toFixed(3)} с`
}

function relinkError(code: CompositionRelinkErrorCode, message: string): CompositionRelinkError {
  return new CompositionRelinkError(code, message)
}
