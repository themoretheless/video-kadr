import {rewindowConcatClipAnimation,rewindowDspSourceKeys,rewindowConcatSourceEffects,rewindowConcatSourceRun} from './concatSourceWindow'
import {splitTransitionWindow,transitionWindowReplaces,transitionSourceSpan} from './transitionTimeline'
import { cloneGraphicValue } from './graphicLayers'
import { rebaseNativeEffects } from './nativeEffects'
import { rebaseDspEffects } from './dsp'
import { AUDIO_TIME_GRID, canvasTimeGrid, shiftTimeGridMembers, shiftTimeGridSpan, snapTimeGridTicks } from './timeGrid'
import { assertCompositionLocksPreserved } from './locks'
import { rebaseColorAnimation } from './colorAnimation'
import { createCompositionLink, linkedClipIds, pruneCompositionLinks } from './links.js'
import {
  COMPOSITION_SCHEMA_VERSION,
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_DURATION_TICKS,
  clipDurationTicks,
  clipEndTicks,
  isVisualTrack,
  type AudioClip,
  type AudioTrack,
  type Composition,
  type CompositionCanvas,
  type CompositionClip,
  type CompositionSource,
  type CompositionSourceRegistry,
  type CompositionTrack,
  type CompositionTransition,
  type ImageClip,
  type ImageTrack,
  type TextClip,
  type TextTrack,
  type VideoClip,
  type VideoTrack,
} from './types'
import {
  cloneAnimatableValue,
  shiftAnimatableValue,
  cloneAudioAnimation,
  cloneVisualAnimation,
  cloneVideoMasks,
  splitAudioAnimation,
  splitVisualAnimation,
  splitVideoMasks,
  shiftAudioAnimation,
  shiftVisualAnimation,
  shiftVideoMasks,
} from './keyframes'
import {
  cloneCompositionSpeedRamp,
  sliceCompositionSpeedRamp,
  speedRampSourceProgressAtTimelineTick,
  speedRampTimelineTickAtSourceProgress,
  speedRampTimelineDurationTicks,
} from './speedRamp'
import {
  assertValidComposition,
  canMoveClipToTrack,
  compositionTransitionUnavailableReason,
  isSafeTick,
  isStableId,
} from './validation'

export type CompositionCommandErrorCode =
  | 'duplicate-id'
  | 'invalid-id'
  | 'invalid-index'
  | 'invalid-range'
  | 'missing-clip'
  | 'missing-source'
  | 'missing-track'
  | 'source-in-use'
  | 'track-kind'
  | 'track-locked'
  | 'clip-locked'
  | 'transition-conflict'

export class CompositionCommandError extends Error {
  constructor(
    readonly code: CompositionCommandErrorCode,
    message: string,
  ) {
    super(message)
    this.name = 'CompositionCommandError'
  }
}

export interface ClipLocation {
  readonly trackIndex: number
  readonly clipIndex: number
  readonly track: CompositionTrack
  readonly clip: CompositionClip
}

export function createComposition(
  canvas: CompositionCanvas,
  sources: CompositionSourceRegistry = {},
): Composition {
  const composition: Composition = {
    schemaVersion: COMPOSITION_SCHEMA_VERSION,
    timeBase: COMPOSITION_TIME_BASE,
    canvas: { ...canvas },
    sources: { ...sources },
    tracks: [],
  }
  return finalize(composition)
}

export function registerSource(composition: Composition, source: CompositionSource): Composition {
  assertValidComposition(composition)
  if (!isStableId(source.id)) throw commandError('invalid-id', 'Source id is not a stable id')
  if (Object.hasOwn(composition.sources, source.id)) {
    throw commandError('duplicate-id', `Source ${source.id} already exists`)
  }
  return finalize({
    ...composition,
    sources: { ...composition.sources, [source.id]: { ...source } },
  })
}

export function unregisterSource(composition: Composition, sourceId: string): Composition {
  assertValidComposition(composition)
  if (!Object.hasOwn(composition.sources, sourceId)) {
    throw commandError('missing-source', `Source ${sourceId} does not exist`)
  }
  if (composition.tracks.some((track) => track.clips.some((clip) => 'sourceId' in clip && clip.sourceId === sourceId))) {
    throw commandError('source-in-use', `Source ${sourceId} is still used by a clip`)
  }
  const sources = Object.fromEntries(Object.entries(composition.sources).filter(([id]) => id !== sourceId))
  return finalize({ ...composition, sources })
}

export function addTrack(
  composition: Composition,
  track: CompositionTrack,
  index: number = composition.tracks.length,
): Composition {
  assertValidComposition(composition)
  if (!isStableId(track.id)) throw commandError('invalid-id', 'Track id is not a stable id')
  if (composition.tracks.some((candidate) => candidate.id === track.id)) {
    throw commandError('duplicate-id', `Track ${track.id} already exists`)
  }
  if (!Number.isSafeInteger(index) || index < 0 || index > composition.tracks.length) {
    throw commandError('invalid-index', `Track index ${index} is out of bounds`)
  }
  for (const clip of track.clips) ensureUniqueClipId(composition, clip.id)
  const tracks = [...composition.tracks]
  tracks.splice(index, 0, cloneTrack(track))
  return finalize({ ...composition, tracks })
}

export function deleteTrack(composition: Composition, trackId: string): Composition {
  assertValidComposition(composition)
  const index = composition.tracks.findIndex((track) => track.id === trackId)
  if (index < 0) throw commandError('missing-track', `Track ${trackId} does not exist`)
  if (composition.tracks[index]!.locked) throw commandError('track-locked', `Track ${trackId} is locked`)
  for (const clip of composition.tracks[index]!.clips) ensureUnlocked(composition.tracks[index]!, clip)
  return finalize({
    ...composition,
    tracks: composition.tracks.filter((track) => track.id !== trackId),
    ...(composition.metadata?.captionTextTracks ? { metadata: { ...composition.metadata,
      captionTextTracks: composition.metadata.captionTextTracks.filter((id) => id !== trackId),
    } } : {}),
  })
}

export function addClip(composition: Composition, trackId: string, clip: CompositionClip): Composition {
  assertValidComposition(composition)
  ensureUniqueClipId(composition, clip.id)
  const trackIndex = findTrackIndex(composition, trackId)
  const track = composition.tracks[trackIndex]!
  ensureUnlocked(track)
  ensureCompatible(track, clip)
  const replacement = withClips(track, sortClips([...track.clips, cloneClip(clip)]))
  return finalize(replaceTrack(composition, trackIndex, replacement))
}

export function moveClip(
  composition: Composition,
  clipId: string,
  targetTrackId: string,
  timelineStartTicks: number,
  escapeLink = false,
): Composition {
  assertValidComposition(composition)
  if (!isSafeTick(timelineStartTicks)) throw commandError('invalid-range', 'Clip start must be a non-negative safe tick')
  const location = findClipLocation(composition, clipId)
  const members = escapeLink ? [clipId] : linkedClipIds(composition, clipId)
  if (members.length > 1) {
    return moveClipSet(composition, [clipId, ...members.filter((id) => id !== clipId)], targetTrackId, timelineStartTicks, true)
  }
  if (location.track.kind === 'video' && location.track.transitions?.some(transition=>transition.timeline&&(transition.fromClipId===clipId||transition.toClipId===clipId))) return moveClipSet(composition,[clipId],targetTrackId,timelineStartTicks)
  const targetTrackIndex = findTrackIndex(composition, targetTrackId)
  const targetTrack = composition.tracks[targetTrackIndex]!
  ensureUnlocked(location.track, location.clip)
  if (targetTrack.id !== location.track.id) ensureUnlocked(targetTrack)
  ensureCompatible(targetTrack, location.clip)
  if (
    targetTrack.id !== location.track.id &&
    location.track.kind === 'video' &&
    (location.track.transitions ?? []).some((transition) => transition.fromClipId === clipId || transition.toClipId === clipId)
  ) {
    throw commandError('transition-conflict', 'Remove transitions before moving this clip to another track')
  }

  const moved = withClipGridStart(composition, location.clip, timelineStartTicks, true)
  ensureTimelineRange(moved)
  if (targetTrack.id === location.track.id && JSON.stringify(moved) === JSON.stringify(location.clip)) return composition

  const tracks = [...composition.tracks]
  const sourceClips = location.track.clips.filter((clip) => clip.id !== clipId)
  if (targetTrackIndex === location.trackIndex) {
    tracks[location.trackIndex] = withClips(location.track, sortClips([...sourceClips, moved]))
  } else {
    tracks[location.trackIndex] = withClips(location.track, sourceClips)
    tracks[targetTrackIndex] = withClips(targetTrack, sortClips([...targetTrack.clips, moved]))
  }
  return finalize({ ...composition, tracks })
}

/** Move an exact subject set together. Only the anchor changes track. */
export function moveClips(composition: Composition, clipIds: readonly string[], targetTrackId: string, timelineStartTicks: number): Composition {
  return moveClipSet(composition, clipIds, targetTrackId, timelineStartTicks)
}

/** Explicit per-subject lane destinations; links never expand this set. */
export function moveClipPlacements(composition: Composition, placements: readonly { clipId: string; trackId: string }[], anchorId: string, startTicks: number): Composition {
  if (!Array.isArray(placements) || placements.some((placement) => !placement || typeof placement.clipId !== 'string' || typeof placement.trackId !== 'string') || !placements.some((placement) => placement.clipId === anchorId)) {
    throw commandError('invalid-range', 'Placements require a selected anchor and valid destinations')
  }
  const ordered = [anchorId, ...placements.filter((placement) => placement.clipId !== anchorId).map((placement) => placement.clipId)]
  if (new Set(placements.map((placement) => placement.clipId)).size !== placements.length) throw commandError('invalid-range', 'Placements require distinct subjects')
  const targets = new Map(placements.map((placement) => [placement.clipId, placement.trackId]))
  return moveClipSet(composition, ordered, targets.get(anchorId)!, startTicks, false, targets)
}

/** Shift every displayed subject by the anchor's displayed lane offset; hidden members stay put. */
export function moveClipsByTrackOffset(composition: Composition, clipIds: readonly string[], anchorId: string, targetTrackId: string, startTicks: number, displayedTrackIds: readonly string[]): Composition {
  if (new Set(displayedTrackIds).size !== displayedTrackIds.length || displayedTrackIds.some((id) => !composition.tracks.some((track) => track.id === id))) throw commandError('invalid-range', 'Displayed lanes must be distinct existing tracks')
  const anchor = findClipLocation(composition, anchorId)
  const origin = displayedTrackIds.indexOf(anchor.track.id)
  const destination = displayedTrackIds.indexOf(targetTrackId)
  if (origin < 0 || destination < 0 || !clipIds.includes(anchorId)) throw commandError('invalid-range', 'Move anchor and destination must be displayed')
  const offset = destination - origin
  const placements = clipIds.map((clipId) => {
    const location = findClipLocation(composition, clipId)
    const index = displayedTrackIds.indexOf(location.track.id)
    const trackId = index < 0 ? location.track.id : displayedTrackIds[index + offset]
    if (!trackId) throw commandError('invalid-range', 'Move exceeds displayed lanes')
    return { clipId, trackId }
  })
  return moveClipPlacements(composition, placements, anchorId, startTicks)
}

function moveClipSet(composition: Composition, clipIds: readonly string[], targetTrackId: string, timelineStartTicks: number, preserveStationaryPartners = false, destinations?: ReadonlyMap<string, string>): Composition {
  assertValidComposition(composition)
  if (!Array.isArray(clipIds) || !clipIds.length || clipIds.length > 512 || new Set(clipIds).size !== clipIds.length || !isSafeTick(timelineStartTicks)) throw commandError('invalid-range', 'Move requires distinct clips and a safe start')
  const locations = clipIds.map((id) => findClipLocation(composition, id))
  const earliest = Math.min(...locations.map(({ clip }) => clip.timelineStartTicks))
  const delta = Math.max(-earliest, snapClipGridStart(composition, locations[0]!.clip, timelineStartTicks) - locations[0]!.clip.timelineStartTicks)
  const movedMembers = locations.map((item, index) => {
    const target = composition.tracks[findTrackIndex(composition, destinations?.get(item.clip.id) ?? (index === 0 ? targetTrackId : item.track.id))]!
    ensureUnlocked(item.track, item.clip)
    ensureUnlocked(target)
    ensureCompatible(target, item.clip)
    const clip = withClipGridStart(composition, item.clip, item.clip.timelineStartTicks + delta, !preserveStationaryPartners || index === 0)
    ensureTimelineRange(clip)
    return { trackId: target.id, clip }
  })
  const targetOf = (id: string | null, sourceTrackId: string) => movedMembers.find((item) => item.clip.id === id)?.trackId ?? sourceTrackId
  const transitions = composition.tracks.flatMap((track) => track.kind === 'video' ? (track.transitions ?? []).flatMap((transition) => {
    // Filmcraft move_items deletes originals before placing them: touched explicit
    // transitions become orphans even when both endpoints are moved together.
    if (transition.timeline && [transition.fromClipId, transition.toClipId].some(id=>id!==null&&clipIds.includes(id))) return []
    const fromTrackId = targetOf(transition.fromClipId, track.id)
    const toTrackId = targetOf(transition.toClipId, track.id)
    if (fromTrackId !== toTrackId) throw commandError('transition-conflict', 'Transition participants must move together to one track')
    return [{ trackId: fromTrackId, sourceTrackId: track.id, transition }]
  }) : [])
  const result = finalize({ ...composition, tracks: composition.tracks.map((track) => {
    const next = withClips(track, sortClips([...track.clips.filter((clip) => !clipIds.includes(clip.id)), ...movedMembers.filter((item) => item.trackId === track.id).map((item) => item.clip)]))
    if (next.kind !== 'video') return next
    const landedTransitions = transitions.filter((item) => item.trackId === track.id).map((item) => item.transition)
    return next.transitions !== undefined || landedTransitions.length ? { ...next, transitions: landedTransitions } : next
  }) })
  for (const item of transitions.filter((item) => item.trackId !== item.sourceTrackId)) {
    const reason = compositionTransitionUnavailableReason(result, item.trackId, item.transition)
    if (reason) throw commandError('transition-conflict', reason)
  }
  return result
}

/** Raise an exact, homogeneous subject set without rounding authored times or expanding links. */
export function moveClipsToNewTrack(composition: Composition, clipIds: readonly string[], trackId: string, anchor?: { clipId: string; startTicks: number }): Composition {
  assertValidComposition(composition)
  if (!Array.isArray(clipIds) || !clipIds.length || clipIds.length > 512 || new Set(clipIds).size !== clipIds.length) {
    throw commandError('invalid-range', 'Raise requires 1–512 distinct clips')
  }
  if (!isStableId(trackId)) throw commandError('invalid-id', 'Track id is not a stable id')
  if (composition.tracks.some((track) => track.id === trackId)) throw commandError('duplicate-id', `Track ${trackId} already exists`)
  const locations = clipIds.map((id) => findClipLocation(composition, id))
  if (anchor !== undefined && (!anchor || !clipIds.includes(anchor.clipId) || !isSafeTick(anchor.startTicks))) {
    throw commandError('invalid-range', 'Landing requires a selected anchor and a safe start')
  }
  const kind = locations[0]!.clip.kind
  for (const { track, clip } of locations) {
    ensureUnlocked(track, clip)
    if (clip.kind !== kind) throw commandError('track-kind', 'Mixed clip kinds require a mixed timeline lane')
  }
  const selected = new Set(clipIds)
  const carriedTransitions = composition.tracks.flatMap((track) => track.kind === 'video' ? (track.transitions ?? []).filter((transition) => {
    const from = (transition.fromClipId !== null && selected.has(transition.fromClipId))
    const to = (transition.toClipId !== null && selected.has(transition.toClipId))
    if (transition.timeline) return false
    if (from !== to) throw commandError('transition-conflict', 'Transition participants must be raised together')
    return from && to
  }) : [])
  const sourceTracks = new Set(locations.map(({ track }) => track.id))
  const rate = canvasTimeGrid(composition.canvas)
  const seed = anchor ? locations.find(({ clip }) => clip.id === anchor.clipId)!.clip : null
  const landings = anchor && seed ? shiftTimeGridMembers(locations.map(({ clip }) => ({ id: clip.id, kind: clip.kind, start: clip.timelineStartTicks, end: clipEndTicks(clip) })), snapTimeGridTicks(anchor.startTicks, seed.kind === 'audio' ? AUDIO_TIME_GRID : rate) - seed.timelineStartTicks, rate) : null
  const moved = locations.map(({ clip }) => {
    const landed = landings?.get(clip.id)
    if (!landed) return cloneClip(clip)
    const duration = landed.end - landed.start
    if (clip.kind === 'video' || clip.kind === 'audio') {
      const next = shiftClipAutomation({ ...cloneClip(clip), timelineStartTicks: landed.start, timelineDurationTicks: duration }, 0)
      ensureTimelineRange(next)
      return next
    }
    const next = { ...cloneClip(clip), timelineStartTicks: landed.start, durationTicks: duration }
    ensureTimelineRange(next)
    return next
  })
  const fresh = withClips({ ...locations[0]!.track, id: trackId, name: 'Новая дорожка', clips: [], ...(kind === 'video' ? { transitions: carriedTransitions } : {}) } as CompositionTrack, sortClips(moved))
  const tracks = composition.tracks.map((track) => {
    if (!sourceTracks.has(track.id)) return track
    const remaining = withClips(track, track.clips.filter((clip) => !selected.has(clip.id)))
    return remaining.kind === 'video' && remaining.transitions ? { ...remaining, transitions: remaining.transitions.filter((transition) => transition.timeline ? ![transition.fromClipId,transition.toClipId].some(id=>id!==null&&selected.has(id)) : !(transition.fromClipId !== null && selected.has(transition.fromClipId))) } : remaining
  }).filter((track) => !sourceTracks.has(track.id) || track.clips.length > 0)
  // Array index zero is the highest local visual layer.
  const result = finalize({ ...composition, tracks: [fresh, ...tracks] })
  for (const transition of carriedTransitions) {
    const reason = compositionTransitionUnavailableReason(result, trackId, transition)
    if (reason) throw commandError('transition-conflict', reason)
  }
  return result
}

/** Move media under a fixed timeline range without changing clip placement. */
export function slipClip(composition: Composition, clipId: string, sourceDeltaTicks: number): Composition {
  assertValidComposition(composition)
  if (!Number.isSafeInteger(sourceDeltaTicks)) throw commandError('invalid-range', 'Slip delta must be a safe tick')
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track, location.clip)
  if (location.clip.kind !== 'video' && location.clip.kind !== 'audio') {
    throw commandError('track-kind', 'Only video and audio clips can slip')
  }
  if (location.clip.speedRamp || (location.clip.kind === 'video' && location.clip.playbackMode?.mode === 'freeze')) {
    throw commandError('invalid-range', 'Freeze and speed-ramped clips cannot slip')
  }
  const source = composition.sources[location.clip.sourceId]
  if (!source) throw commandError('missing-source', `Source ${location.clip.sourceId} does not exist`)
  const span = location.clip.sourceOutTicks - location.clip.sourceInTicks
  const sourceInTicks = Math.max(0, Math.min(source.durationTicks - span, location.clip.sourceInTicks + sourceDeltaTicks))
  if (sourceInTicks === location.clip.sourceInTicks) return composition
  return finalize(replaceLocatedClip(composition, location, {
    ...location.clip,
    sourceInTicks,
    sourceOutTicks: sourceInTicks + span,
  }))
}

/**
 * Trim against composition-time boundaries. Source clips move their source-in
 * and source-out by the same deltas; image/text clips update duration.
 */
export function trimClip(
  composition: Composition,
  clipId: string,
  newStartTicks: number,
  newEndTicks: number,
  escapeLink = false,
): Composition {
  assertValidComposition(composition)
  if (!isSafeTick(newStartTicks) || !isSafeTick(newEndTicks) || newEndTicks <= newStartTicks) {
    throw commandError('invalid-range', 'Trim must produce a positive safe range')
  }
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track, location.clip)
  const oldStart = location.clip.timelineStartTicks
  const oldEnd = clipEndTicks(location.clip)
  if (oldStart === newStartTicks && oldEnd === newEndTicks) return composition

  const members = escapeLink ? [clipId] : linkedClipIds(composition, clipId)
  if (members.length > 1) {
    const affected = members.map((id) => findClipLocation(composition, id)).filter((item) =>
      (newStartTicks !== oldStart && item.clip.timelineStartTicks === oldStart) ||
      (newEndTicks !== oldEnd && clipEndTicks(item.clip) === oldEnd))
    let startDelta = newStartTicks - oldStart
    let endDelta = newEndTicks - oldEnd
    for (const item of affected) {
      ensureUnlocked(item.track, item.clip)
      const clip = item.clip
      const startAligned = clip.timelineStartTicks === oldStart
      const endAligned = clipEndTicks(clip) === oldEnd
      if (startAligned) startDelta = Math.max(startDelta, -clip.timelineStartTicks)
      if (endAligned) endDelta = Math.min(endDelta, MAX_COMPOSITION_DURATION_TICKS - clipEndTicks(clip))
      if (clip.kind === 'video' || clip.kind === 'audio') {
        if (clip.speedRamp) {
          // Existing ramp evaluation supports slicing the authored range, not extrapolation.
          if (startAligned) startDelta = Math.max(startDelta, 0)
          if (endAligned) endDelta = Math.min(endDelta, 0)
        } else if (!(clip.kind === 'video' && clip.playbackMode?.mode === 'freeze')) {
          const source = composition.sources[clip.sourceId]!
          const reverse = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
          const speed = clip.speed ?? 1
          const before = reverse ? source.durationTicks - clip.sourceOutTicks : clip.sourceInTicks
          const after = reverse ? clip.sourceInTicks : source.durationTicks - clip.sourceOutTicks
          if (startAligned) startDelta = Math.max(startDelta, -Math.floor(before / speed))
          if (endAligned) endDelta = Math.min(endDelta, Math.floor(after / speed))
        }
      }
    }
    // Keep every touched range positive; a slipped, unaligned edge stays fixed.
    for (const { clip } of affected) {
      const duration = clipDurationTicks(clip)
      const left = clip.timelineStartTicks === oldStart
      const right = clipEndTicks(clip) === oldEnd
      if (left && !right) startDelta = Math.min(startDelta, duration - 1)
      if (right && !left) endDelta = Math.max(endDelta, 1 - duration)
      if (left && right && duration + endDelta - startDelta <= 0) throw commandError('invalid-range', 'Linked trim would invert a member')
    }
    let result = composition
    for (const { clip } of affected) result = trimClip(result, clip.id,
      clip.timelineStartTicks + (clip.timelineStartTicks === oldStart ? startDelta : 0),
      clipEndTicks(clip) + (clipEndTicks(clip) === oldEnd ? endDelta : 0), true)
    return result
  }

  const startDelta = newStartTicks - oldStart
  const endDelta = newEndTicks - oldEnd
  let trimmed: CompositionClip
  if ((location.clip.kind === 'video' || location.clip.kind === 'audio') && location.clip.speedRamp) {
    trimmed = trimRampedSourceClip(
      location.clip,
      newStartTicks,
      newStartTicks - oldStart,
      newEndTicks - oldStart,
    )
  } else if (location.clip.kind === 'video') {
    const speed = location.clip.speed ?? 1
    const mode = location.clip.playbackMode?.mode ?? 'forward'
    if (mode === 'freeze') {
      trimmed = {
        ...location.clip,
        timelineStartTicks: newStartTicks,
        ...freezeSourceRange(composition, location.clip, newEndTicks - newStartTicks),
      }
    } else if (mode === 'reverse') {
      trimmed = {
        ...location.clip,
        timelineStartTicks: newStartTicks,
        sourceInTicks: location.clip.sourceInTicks - Math.round(endDelta * speed),
        sourceOutTicks: location.clip.sourceOutTicks - Math.round(startDelta * speed),
      }
    } else {
      trimmed = {
        ...location.clip,
        timelineStartTicks: newStartTicks,
        sourceInTicks: location.clip.sourceInTicks + Math.round(startDelta * speed),
        sourceOutTicks: location.clip.sourceOutTicks + Math.round(endDelta * speed),
      }
    }
  } else if (location.clip.kind === 'audio') {
    const speed = location.clip.speed ?? 1
    trimmed = {
      ...location.clip,
      timelineStartTicks: newStartTicks,
      sourceInTicks: location.clip.sourceInTicks + Math.round(startDelta * speed),
      sourceOutTicks: location.clip.sourceOutTicks + Math.round(endDelta * speed),
    }
  } else {
    trimmed = {
      ...location.clip,
      timelineStartTicks: newStartTicks,
      durationTicks: newEndTicks - newStartTicks,
    }
  }
  if ((trimmed.kind === 'video' || trimmed.kind === 'audio') && location.clip.kind !== 'text' && location.clip.kind !== 'image' && location.clip.kind !== 'color' && location.clip.kind !== 'graphic' && location.clip.timelineDurationTicks !== undefined) {
    trimmed = { ...trimmed, timelineDurationTicks: newEndTicks - newStartTicks }
  }
  trimmed = shiftClipAutomation(trimmed, oldStart - newStartTicks)
  if((trimmed.kind==='video'||trimmed.kind==='audio')&&trimmed.concatGainKeys?.length)trimmed={...trimmed,concatGainKeys:rewindowConcatSourceRun(trimmed.concatGainKeys,(newStartTicks-oldStart)/(oldEnd-oldStart),(newEndTicks-oldStart)/(oldEnd-oldStart))}
  if((trimmed.kind==='video'||trimmed.kind==='audio')&&trimmed.dspEffects)trimmed={...trimmed,dspEffects:rewindowDspSourceKeys(trimmed.dspEffects,oldEnd-oldStart,newStartTicks-oldStart,newEndTicks-oldStart)}
  if((trimmed.kind==='video'||trimmed.kind==='image')&&trimmed.effects)trimmed={...trimmed,effects:rewindowConcatSourceEffects(trimmed.effects,oldEnd-oldStart,newStartTicks-oldStart,newEndTicks-oldStart)}
  if((trimmed.kind==='video'||trimmed.kind==='image')&&trimmed.concatAnimation)trimmed={...trimmed,concatAnimation:rewindowConcatClipAnimation(trimmed.concatAnimation,oldEnd-oldStart,newStartTicks-oldStart,newEndTicks-oldStart)}
  ensureTimelineRange(trimmed)
  return replaceLocatedClip(composition, location, trimmed)
}

export function splitClip(
  composition: Composition,
  clipId: string,
  atTicks: number,
  rightClipId: string,
  escapeLink = false,
): Composition {
  assertValidComposition(composition)
  ensureUniqueClipId(composition, rightClipId)
  const location = findClipLocation(composition, clipId)
  const members = escapeLink ? [clipId] : linkedClipIds(composition, clipId)
  if (members.length > 1) {
    if (!isSafeTick(atTicks) || atTicks <= location.clip.timelineStartTicks || atTicks >= clipEndTicks(location.clip)) throw commandError('invalid-range', 'Split point must be strictly inside the clip')
    let result = composition
    const added: string[] = []
    for (const [index, member] of members.entries()) {
      const item = findClipLocation(composition, member)
      if (item.clip.timelineStartTicks >= atTicks || clipEndTicks(item.clip) <= atTicks) continue
      const rightId = member === clipId ? rightClipId : `${rightClipId}-linked-${index}`
      result = splitClip(result, member, atTicks, rightId, true)
      added.push(rightId)
    }
    // Filmcraft split_track_at assigns one fresh link to the right pieces.
    // Left pieces and unsplit partners retain the original link.
    const rightLinkId = `${rightClipId}-link`
    if (added.length >= 2 && result.links?.some((link) => link.id === rightLinkId)) {
      throw commandError('duplicate-id', `Link ${rightLinkId} already exists`)
    }
    return finalize({ ...result, links: [
      ...(result.links ?? []),
      ...(added.length >= 2 ? [{ id: rightLinkId, memberIds: added }] : []),
    ] })
  }
  ensureUnlocked(location.track, location.clip)
  const end = clipEndTicks(location.clip)
  if (!isSafeTick(atTicks) || atTicks <= location.clip.timelineStartTicks || atTicks >= end) {
    throw commandError('invalid-range', 'Split point must be strictly inside the clip')
  }
  const leftDuration = atTicks - location.clip.timelineStartTicks
  const rightDuration = end - atTicks
  let left: CompositionClip
  let right: CompositionClip
  if (location.clip.kind === 'video') {
    const mode = location.clip.playbackMode?.mode ?? 'forward'
    if (location.clip.speedRamp) {
      const split = splitRampedSourceClip(location.clip, leftDuration, rightClipId)
      left = split.left
      right = split.right
    } else if (mode === 'freeze') {
      left = { ...location.clip, ...freezeSourceRange(composition, location.clip, leftDuration) }
      right = {
        ...location.clip,
        ...freezeSourceRange(composition, location.clip, rightDuration),
        id: rightClipId,
        timelineStartTicks: atTicks,
      }
    } else {
      const sourceBoundary = mode === 'reverse'
        ? location.clip.sourceOutTicks - Math.round(leftDuration * (location.clip.speed ?? 1))
        : location.clip.sourceInTicks + Math.round(leftDuration * (location.clip.speed ?? 1))
      left = mode === 'reverse'
        ? { ...location.clip, sourceInTicks: sourceBoundary }
        : { ...location.clip, sourceOutTicks: sourceBoundary }
      right = mode === 'reverse'
        ? {
            ...location.clip,
            id: rightClipId,
            timelineStartTicks: atTicks,
            sourceOutTicks: sourceBoundary,
          }
        : {
            ...location.clip,
            id: rightClipId,
            timelineStartTicks: atTicks,
            sourceInTicks: sourceBoundary,
          }
    }
  } else if (location.clip.kind === 'audio') {
    if (location.clip.speedRamp) {
      const split = splitRampedSourceClip(location.clip, leftDuration, rightClipId)
      left = split.left
      right = split.right
    } else {
      const sourceDelta = Math.round(leftDuration * (location.clip.speed ?? 1))
      const sourceBoundary = location.clip.reversed
        ? location.clip.sourceOutTicks - sourceDelta
        : location.clip.sourceInTicks + sourceDelta
      left = location.clip.reversed
        ? { ...location.clip, sourceInTicks: sourceBoundary }
        : { ...location.clip, sourceOutTicks: sourceBoundary }
      right = {
        ...location.clip,
        id: rightClipId,
        timelineStartTicks: atTicks,
        ...(location.clip.reversed ? { sourceOutTicks: sourceBoundary } : { sourceInTicks: sourceBoundary }),
      }
    }
  } else {
    left = { ...location.clip, durationTicks: leftDuration }
    right = {
      ...location.clip,
      id: rightClipId,
      timelineStartTicks: atTicks,
      durationTicks: rightDuration,
    }
  }
  if ((left.kind === 'video' || left.kind === 'audio') && (right.kind === 'video' || right.kind === 'audio') && left.timelineDurationTicks !== undefined) {
    left = { ...left, timelineDurationTicks: leftDuration }
    right = { ...right, timelineDurationTicks: rightDuration }
  }
  for(const side of ['left','right']as const){const clip=side==='left'?left:right;if((clip.kind==='video'||clip.kind==='audio')&&clip.concatGainKeys?.length){const next={...clip,concatGainKeys:rewindowConcatSourceRun(clip.concatGainKeys,side==='left'?0:leftDuration/(leftDuration+rightDuration),side==='left'?leftDuration/(leftDuration+rightDuration):1)};if(side==='left')left=next;else right=next;}}
  for(const side of ['left','right']as const){const clip=side==='left'?left:right;if((clip.kind==='video'||clip.kind==='audio')&&clip.dspEffects){const next={...clip,dspEffects:rewindowDspSourceKeys(clip.dspEffects,leftDuration+rightDuration,side==='left'?0:leftDuration,side==='left'?leftDuration:leftDuration+rightDuration)};if(side==='left')left=next;else right=next;}}
  for(const side of ['left','right']as const){const clip=side==='left'?left:right;if((clip.kind==='video'||clip.kind==='image')&&clip.effects){const next={...clip,effects:rewindowConcatSourceEffects(clip.effects,leftDuration+rightDuration,side==='left'?0:leftDuration,side==='left'?leftDuration:leftDuration+rightDuration)};if(side==='left')left=next;else right=next;}}
  for(const side of ['left','right']as const){const clip=side==='left'?left:right;if((clip.kind==='video'||clip.kind==='image')&&clip.concatAnimation){const next={...clip,concatAnimation:rewindowConcatClipAnimation(clip.concatAnimation,leftDuration+rightDuration,side==='left'?0:leftDuration,side==='left'?leftDuration:leftDuration+rightDuration)};if(side==='left')left=next;else right=next;}}
  left = splitClipAutomation(left, leftDuration, false)
  right = splitClipAutomation(right, leftDuration, true)
  const clips = [...location.track.clips]
  clips.splice(location.clipIndex, 1, left, right)
  let replacement = withClips(location.track, sortClips(clips))
  if (replacement.kind === 'video') {
    replacement = {
      ...replacement,
      transitions: (replacement.transitions ?? []).flatMap((transition) => {
        if (transition.timeline) {
          const result=splitTransitionWindow({...transition,timeline:transition.timeline},atTicks,clipId,rightClipId)
          return result?[result]:[]
        }
        return [transition.fromClipId === clipId ? { ...transition, fromClipId: rightClipId } : transition]
      }),
    }
  }
  let next = replaceTrack(composition, location.trackIndex, replacement)
  next = copyMediaVersions(next, clipId, rightClipId)
  const name = composition.metadata?.clipNames?.[clipId]
  if (name !== undefined) next = { ...next, metadata: { ...next.metadata, clipNames: { ...next.metadata?.clipNames, [rightClipId]: name } } }
  return finalize(next)
}

export interface SourceTickRange {
  readonly start: number
  readonly end: number
}

/**
 * Replace one audio-bearing source clip with its audible source ranges. The
 * existing trim command owns reverse/ramp/keyframe slicing; this command only
 * orders the slices, closes removed gaps, and ripples later clips on the track.
 */
export function removeSilenceFromClip(
  composition: Composition,
  clipId: string,
  audibleSourceRanges: readonly SourceTickRange[],
  replacementIds: readonly string[],
): Composition {
  assertValidComposition(composition)
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track, location.clip)
  if (location.clip.kind !== 'video' && location.clip.kind !== 'audio') {
    throw commandError('track-kind', 'Silence removal requires a video or audio clip')
  }
  if (location.clip.kind === 'video' && location.clip.playbackMode?.mode === 'freeze') {
    throw commandError('invalid-range', 'Freeze clip has no source audio timeline')
  }
  if (
    location.track.kind === 'video' &&
    (location.track.transitions ?? []).some(
      (transition) => transition.fromClipId === clipId || transition.toClipId === clipId,
    )
  ) {
    throw commandError('transition-conflict', 'Remove the clip transition before silence removal')
  }

  const clip = location.clip
  const ranges = audibleSourceRanges
    .map(({ start, end }) => ({
      start: Math.max(clip.sourceInTicks, Math.round(start)),
      end: Math.min(clip.sourceOutTicks, Math.round(end)),
    }))
    .filter(({ start, end }) => end > start)
    .sort((left, right) => left.start - right.start || left.end - right.end)
  const ordered = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
    ? ranges.reverse()
    : ranges
  if (!ordered.length) throw commandError('invalid-range', 'Silence removal would remove the complete clip')
  if (replacementIds.length !== ordered.length - 1) {
    throw commandError('invalid-id', 'Silence removal replacement ids do not match the slices')
  }
  for (const id of replacementIds) ensureUniqueClipId(composition, id)
  if (new Set(replacementIds).size !== replacementIds.length) {
    throw commandError('duplicate-id', 'Silence removal replacement ids must be unique')
  }

  const sourceSpan = clip.sourceOutTicks - clip.sourceInTicks
  const reverse = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
  const speed = clip.speed ?? 1
  const sourceProgress = (sourceTick: number): number => {
    if (!reverse) return sourceTick - clip.sourceInTicks
    return clip.sourceOutTicks - sourceTick
  }
  const timelineProgress = (progress: number): number => clip.speedRamp
    ? speedRampTimelineTickAtSourceProgress(sourceSpan, speed, clip.speedRamp, progress)
    : Math.round(progress / speed)

  let cursor = clip.timelineStartTicks
  const slices: CompositionClip[] = []
  for (const [index, range] of ordered.entries()) {
    const startProgress = reverse
      ? sourceProgress(range.end)
      : sourceProgress(range.start)
    const endProgress = reverse
      ? sourceProgress(range.start)
      : sourceProgress(range.end)
    const localStart = timelineProgress(startProgress)
    const localEnd = timelineProgress(endProgress)
    const trimmedDocument = trimClip(
      composition,
      clipId,
      clip.timelineStartTicks + localStart,
      clip.timelineStartTicks + localEnd,
    )
    const trimmed = findClipLocation(trimmedDocument, clipId).clip
    const id = index === 0 ? clipId : replacementIds[index - 1]!
    const slice = { ...trimmed, id, timelineStartTicks: cursor } as CompositionClip
    slices.push(slice)
    cursor += clipDurationTicks(slice)
  }

  const removedDuration = clipEndTicks(clip) - cursor
  const clips = location.track.clips
    .filter((candidate) => candidate.id !== clipId)
    .map((candidate) => candidate.timelineStartTicks >= clipEndTicks(clip)
      ? { ...candidate, timelineStartTicks: candidate.timelineStartTicks - removedDuration } as CompositionClip
      : candidate)
  clips.push(...slices)
  const next = finalize(replaceTrack(
    composition,
    location.trackIndex,
    withClips(location.track, sortClips(clips)),
  ))
  assertCompositionLocksPreserved(composition, next)
  return next
}

/** Exact selection, no link expansion. All guards precede constructing the result. */
export function setClipLocked(composition: Composition, clipId: string, locked: boolean): Composition {
  assertValidComposition(composition)
  if (typeof locked !== 'boolean') throw commandError('invalid-range', 'locked must be a boolean')
  const location = findClipLocation(composition, clipId)
  return finalize(replaceTrack(composition, location.trackIndex, withClips(location.track, location.track.clips.map((clip) => clip.id === clipId ? { ...clip, locked } : clip))))
}

export function setClipsEnabled(composition: Composition, clipIds: readonly string[], enabled: boolean): Composition {
  assertValidComposition(composition)
  if (typeof enabled !== 'boolean' || !Array.isArray(clipIds) || !clipIds.length || clipIds.length > 512) {
    throw commandError('invalid-range', 'Enabled command requires a boolean and 1–512 clip IDs')
  }
  const ids = new Set(clipIds)
  for (const id of ids) { const location = findClipLocation(composition, id); ensureUnlocked(location.track, location.clip) }
  return finalize({ ...composition, tracks: composition.tracks.map((track) =>
    withClips(track, track.clips.map((clip) => ids.has(clip.id) ? { ...clip, enabled } : clip)),
  ) })
}

export function deleteClip(composition: Composition, clipId: string): Composition {
  assertValidComposition(composition)
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track, location.clip)
  let replacement = withClips(location.track, location.track.clips.filter((clip) => clip.id !== clipId))
  if (replacement.kind === 'video') {
    replacement = {
      ...replacement,
      transitions: (replacement.transitions ?? []).filter(
        (transition) => transition.fromClipId !== clipId && transition.toClipId !== clipId,
      ),
    }
  }
  const next = finalize(replaceTrack(composition, location.trackIndex, replacement))
  assertCompositionLocksPreserved(composition, next)
  return next
}

export function upsertTransition(
  composition: Composition,
  trackId: string,
  transition: CompositionTransition,
): Composition {
  assertValidComposition(composition)
  const trackIndex = findTrackIndex(composition, trackId)
  const track = composition.tracks[trackIndex]!
  ensureUnlocked(track)
  if (track.kind !== 'video') throw commandError('track-kind', 'Transitions require a video track')
  const windowOf = (item:CompositionTransition) => ({...item,timeline:item.timeline??{startTick:(track.clips.find(clip=>clip.id===item.toClipId)?.timelineStartTicks??0)-Math.floor(item.durationTicks/2),durationTick:item.durationTicks}})
  const removed=(track.transitions??[]).filter(item=>item.id===transition.id||(transition.timeline&&transitionWindowReplaces(windowOf(item),windowOf(transition))))
  const affected = [transition.fromClipId, transition.toClipId, ...removed.flatMap(item=>[item.fromClipId,item.toClipId])]
  for (const id of affected) { const clip = track.clips.find((clip) => clip.id === id); if (clip) ensureUnlocked(track, clip) }
  if (!isStableId(transition.id)) throw commandError('invalid-id', 'Transition id is not stable')
  const reason = transition.timeline ? null : compositionTransitionUnavailableReason(composition, trackId, transition)
  if (reason) throw commandError('transition-conflict', reason)
  const transitions = [...(track.transitions ?? []).filter(candidate=>!removed.includes(candidate)), { ...transition }]
    .sort((left, right) => {
      if (left.timeline || right.timeline) {
        const a=transitionSourceSpan(windowOf(left).timeline)[0],b=transitionSourceSpan(windowOf(right).timeline)[0]
        return a<b?-1:a>b?1:0
      }
      return (left.fromClipId ?? '').localeCompare(right.fromClipId ?? '') || left.id.localeCompare(right.id)
    })
  return finalize(replaceTrack(composition, trackIndex, { ...track, transitions }))
}

export function deleteTransition(composition: Composition, trackId: string, transitionId: string): Composition {
  assertValidComposition(composition)
  const trackIndex = findTrackIndex(composition, trackId)
  const track = composition.tracks[trackIndex]!
  ensureUnlocked(track)
  if (track.kind !== 'video') throw commandError('track-kind', 'Transitions require a video track')
  const removedTransition = track.transitions?.find((transition) => transition.id === transitionId)
  if (removedTransition) for (const id of [removedTransition.fromClipId, removedTransition.toClipId]) { const clip = track.clips.find((clip) => clip.id === id); if (clip) ensureUnlocked(track, clip) }
  if (!(track.transitions ?? []).some((transition) => transition.id === transitionId)) {
    throw commandError('missing-clip', `Transition ${transitionId} does not exist`)
  }
  return finalize(replaceTrack(composition, trackIndex, {
    ...track,
    transitions: (track.transitions ?? []).filter((transition) => transition.id !== transitionId),
  }))
}

export interface DuplicateClipOptions {
  /** Clipboard reads a protected source while destination admission stays enforced. */
  readonly allowLockedSource?: boolean
  readonly escapeLink?: boolean
  readonly linkId?: string
  readonly id: string
  readonly targetTrackId?: string
  readonly timelineStartTicks?: number
}

export function duplicateClip(
  composition: Composition,
  clipId: string,
  options: DuplicateClipOptions,
): Composition {
  assertValidComposition(composition)
  ensureUniqueClipId(composition, options.id)
  const location = findClipLocation(composition, clipId)
  if (!options.allowLockedSource) ensureUnlocked(location.track, location.clip)
  const targetTrackId = options.targetTrackId ?? location.track.id
  const targetIndex = findTrackIndex(composition, targetTrackId)
  const target = composition.tracks[targetIndex]!
  ensureUnlocked(target)
  ensureCompatible(target, location.clip)
  const start = options.timelineStartTicks ?? clipEndTicks(location.clip)
  if (!isSafeTick(start)) throw commandError('invalid-range', 'Duplicate start must be a non-negative safe tick')
  const members = options.escapeLink ? [clipId] : linkedClipIds(composition, clipId)
  if (members.length > 1) {
    const earliest = Math.min(...members.map((id) => findClipLocation(composition, id).clip.timelineStartTicks))
    const delta = Math.max(-earliest, snapClipGridStart(composition, location.clip, start) - location.clip.timelineStartTicks)
    let result = composition
    const copies: string[] = []
    for (const [index, member] of members.entries()) {
      const item = findClipLocation(composition, member)
      const id = member === clipId ? options.id : `${options.id}-linked-${index}`
      result = duplicateClip(result, member, {
        id, escapeLink: true,
        targetTrackId: member === clipId ? targetTrackId : item.track.id,
        timelineStartTicks: item.clip.timelineStartTicks + delta,
      })
      copies.push(id)
    }
    return createCompositionLink(result, options.linkId ?? `${options.id}-link`, copies)
  }
  const duplicate = withClipGridStart(composition, { ...cloneClip(location.clip), id: options.id } as CompositionClip, start, true)
  ensureTimelineRange(duplicate)
  return finalize(copyMediaVersions(replaceTrack(composition, targetIndex, withClips(target, sortClips([...target.clips, duplicate]))), clipId, options.id))
}

/** Copy exactly the named subjects. The first subject anchors time and destination. */
export function pasteClips(composition: Composition, subjects: readonly { clipId: string; id: string }[], options: { targetTrackId?: string; timelineStartTicks?: number; linkIds?: readonly string[]; allowLockedSource?: boolean } = {}): Composition {
  assertValidComposition(composition)
  if (!Array.isArray(subjects) || !subjects.length || subjects.length > 512 || subjects.some((subject) => !subject || typeof subject.clipId !== 'string' || typeof subject.id !== 'string') || new Set(subjects.map((subject) => subject.clipId)).size !== subjects.length || new Set(subjects.map((subject) => subject.id)).size !== subjects.length || (options.linkIds !== undefined && !Array.isArray(options.linkIds))) throw commandError('invalid-range', 'Paste requires distinct subjects and copy ids')
  const locations = subjects.map((subject) => {
    ensureUniqueClipId(composition, subject.id)
    const location = findClipLocation(composition, subject.clipId)
    if (!options.allowLockedSource) ensureUnlocked(location.track, location.clip)
    return location
  })
  const anchor = locations[0]!
  const proposed = options.timelineStartTicks ?? anchor.clip.timelineStartTicks + Math.max(...locations.map(({ clip }) => clipEndTicks(clip))) - Math.min(...locations.map(({ clip }) => clip.timelineStartTicks))
  if (!isSafeTick(proposed)) throw commandError('invalid-range', 'Paste start must be a non-negative safe tick')
  const delta = Math.max(-Math.min(...locations.map(({ clip }) => clip.timelineStartTicks)), snapClipGridStart(composition, anchor.clip, proposed) - anchor.clip.timelineStartTicks)
  const copyIds = new Map(subjects.map((subject) => [subject.clipId, subject.id]))
  const copiedLinks = (composition.links ?? []).map((link) => link.memberIds.filter((id) => copyIds.has(id)).map((id) => copyIds.get(id)!)).filter((members) => members.length >= 2)
  if (options.linkIds && options.linkIds.length !== copiedLinks.length) throw commandError('invalid-range', 'Paste link ids must match the copied link subsets')
  let result = composition
  for (const [index, subject] of subjects.entries()) {
    const location = locations[index]!
    result = duplicateClip(result, subject.clipId, {
      id: subject.id, escapeLink: true, allowLockedSource: options.allowLockedSource,
      targetTrackId: index === 0 ? options.targetTrackId ?? location.track.id : location.track.id,
      timelineStartTicks: location.clip.timelineStartTicks + delta,
    })
  }
  for (const [index, members] of copiedLinks.entries()) result = createCompositionLink(result, options.linkIds?.[index] ?? `${subjects[0]!.id}-link-${index}`, members)
  return result
}

export function reorderTrack(composition: Composition, trackId: string, toIndex: number): Composition {
  assertValidComposition(composition)
  const fromIndex = findTrackIndex(composition, trackId)
  if (!Number.isSafeInteger(toIndex) || toIndex < 0 || toIndex >= composition.tracks.length) {
    throw commandError('invalid-index', `Track index ${toIndex} is out of bounds`)
  }
  if (fromIndex === toIndex) return composition
  const tracks = [...composition.tracks]
  const [track] = tracks.splice(fromIndex, 1)
  tracks.splice(toIndex, 0, track!)
  return finalize({ ...composition, tracks })
}

export function findClipLocation(composition: Composition, clipId: string): ClipLocation {
  for (const [trackIndex, track] of composition.tracks.entries()) {
    const clipIndex = track.clips.findIndex((clip) => clip.id === clipId)
    if (clipIndex >= 0) {
      return { trackIndex, clipIndex, track, clip: track.clips[clipIndex]! }
    }
  }
  throw commandError('missing-clip', `Clip ${clipId} does not exist`)
}

export type SnapTargetKind = 'zero' | 'playhead' | 'clip-start' | 'clip-end'

export interface SnapTarget {
  readonly tick: number
  readonly kind: SnapTargetKind
  readonly clipId?: string
}

export interface CollectSnapTargetsOptions {
  readonly playheadTicks?: number
  readonly excludeClipId?: string
  readonly includeHidden?: boolean
  readonly includeZero?: boolean
}

export function collectSnapTargets(
  composition: Composition,
  options: CollectSnapTargetsOptions = {},
): SnapTarget[] {
  assertValidComposition(composition)
  const targets: SnapTarget[] = []
  if (options.includeZero !== false) targets.push({ tick: 0, kind: 'zero' })
  if (options.playheadTicks !== undefined) {
    if (!isSafeTick(options.playheadTicks) || options.playheadTicks > MAX_COMPOSITION_DURATION_TICKS) {
      throw commandError('invalid-range', 'Playhead must be inside the composition limit')
    }
    targets.push({ tick: options.playheadTicks, kind: 'playhead' })
  }
  for (const track of composition.tracks) {
    if (!options.includeHidden && isVisualTrack(track) && track.hidden) continue
    for (const clip of track.clips) {
      if (clip.id === options.excludeClipId) continue
      targets.push({ tick: clip.timelineStartTicks, kind: 'clip-start', clipId: clip.id })
      targets.push({ tick: clipEndTicks(clip), kind: 'clip-end', clipId: clip.id })
    }
  }
  const priority: Record<SnapTargetKind, number> = { playhead: 0, zero: 1, 'clip-start': 2, 'clip-end': 3 }
  targets.sort((left, right) => left.tick - right.tick || priority[left.kind] - priority[right.kind] || (left.clipId ?? '').localeCompare(right.clipId ?? ''))
  const unique: SnapTarget[] = []
  for (const target of targets) {
    if (unique.at(-1)?.tick !== target.tick) unique.push(target)
  }
  return unique
}

export interface SnapTickResult {
  readonly valueTicks: number
  readonly snapped: boolean
  readonly deltaTicks: number
  readonly target?: SnapTarget
}

export function snapTick(proposedTicks: number, targets: readonly SnapTarget[], thresholdTicks: number): SnapTickResult {
  validateSnapInputs(proposedTicks, thresholdTicks)
  const match = bestSnap(
    targets.map((target) => ({ target, delta: target.tick - proposedTicks })),
    thresholdTicks,
  )
  return match
    ? { valueTicks: proposedTicks + match.delta, snapped: true, deltaTicks: match.delta, target: match.target }
    : { valueTicks: proposedTicks, snapped: false, deltaTicks: 0 }
}

export interface SnapClipResult {
  readonly timelineStartTicks: number
  readonly snapped: boolean
  readonly deltaTicks: number
  readonly edge?: 'start' | 'end'
  readonly target?: SnapTarget
}

/** Snap either moving edge, then return the adjusted clip start. */
export function snapClipStart(
  proposedStartTicks: number,
  durationTicks: number,
  targets: readonly SnapTarget[],
  thresholdTicks: number,
): SnapClipResult {
  validateSnapInputs(proposedStartTicks, thresholdTicks)
  if (!isSafeTick(durationTicks) || durationTicks <= 0) {
    throw commandError('invalid-range', 'Snap duration must be a positive safe tick count')
  }
  const candidates: Array<{ target: SnapTarget; delta: number; edge: 'start' | 'end' }> = []
  for (const target of targets) {
    const startDelta = target.tick - proposedStartTicks
    const endDelta = target.tick - (proposedStartTicks + durationTicks)
    if (isValidSnappedStart(proposedStartTicks + startDelta, durationTicks)) {
      candidates.push({ target, delta: startDelta, edge: 'start' })
    }
    if (isValidSnappedStart(proposedStartTicks + endDelta, durationTicks)) {
      candidates.push({ target, delta: endDelta, edge: 'end' })
    }
  }
  candidates.sort(
    (left, right) =>
      Math.abs(left.delta) - Math.abs(right.delta) ||
      left.target.tick - right.target.tick ||
      (left.edge === 'start' ? -1 : 1),
  )
  const match = candidates.find((candidate) => Math.abs(candidate.delta) <= thresholdTicks)
  return match
    ? {
        timelineStartTicks: proposedStartTicks + match.delta,
        snapped: true,
        deltaTicks: match.delta,
        edge: match.edge,
        target: match.target,
      }
    : { timelineStartTicks: proposedStartTicks, snapped: false, deltaTicks: 0 }
}

function bestSnap(
  candidates: Array<{ target: SnapTarget; delta: number }>,
  thresholdTicks: number,
): { target: SnapTarget; delta: number } | undefined {
  return candidates
    .filter((candidate) => Math.abs(candidate.delta) <= thresholdTicks)
    .sort((left, right) => Math.abs(left.delta) - Math.abs(right.delta) || left.target.tick - right.target.tick)[0]
}

function validateSnapInputs(proposedTicks: number, thresholdTicks: number): void {
  if (!isSafeTick(proposedTicks) || !isSafeTick(thresholdTicks)) {
    throw commandError('invalid-range', 'Snap values must be non-negative safe ticks')
  }
}

function isValidSnappedStart(start: number, duration: number): boolean {
  return isSafeTick(start) && Number.isSafeInteger(start + duration) && start + duration <= MAX_COMPOSITION_DURATION_TICKS
}

function replaceLocatedClip(
  composition: Composition,
  location: ClipLocation,
  replacement: CompositionClip,
): Composition {
  const clips = [...location.track.clips]
  clips[location.clipIndex] = replacement
  return finalize(
    replaceTrack(composition, location.trackIndex, withClips(location.track, sortClips(clips))),
  )
}

function replaceTrack(composition: Composition, index: number, track: CompositionTrack): Composition {
  const tracks = [...composition.tracks]
  tracks[index] = track
  return { ...composition, tracks }
}

function withClips(track: CompositionTrack, clips: readonly CompositionClip[]): CompositionTrack {
  switch (track.kind) {
    case 'video':
      return { ...track, clips: clips as readonly VideoClip[] }
    case 'audio':
      return { ...track, clips: clips as readonly AudioClip[] }
    case 'image':
      return { ...track, clips: clips as readonly ImageClip[] }
    case 'graphic': return { ...track, clips: clips as typeof track.clips }
    case 'color':
      return { ...track, clips: clips as typeof track.clips }
    case 'text':
      return { ...track, clips: clips as readonly TextClip[] }
  }
}

function cloneTrack(track: CompositionTrack): CompositionTrack {
  switch (track.kind) {
    case 'video':
      return {
        ...track,
        clips: track.clips.map(cloneClip) as VideoClip[],
        transitions: track.transitions?.map((transition) => ({ ...transition })),
      } as VideoTrack
    case 'audio':
      return { ...track, clips: track.clips.map(cloneClip) as AudioClip[] } as AudioTrack
    case 'image':
      return { ...track, clips: track.clips.map(cloneClip) as ImageClip[] } as ImageTrack
    case 'graphic': return cloneGraphicValue(track)
    case 'color':
      return { ...track, clips: track.clips.map(cloneClip) }
    case 'text':
      return { ...track, clips: track.clips.map(cloneClip) as TextClip[] } as TextTrack
  }
}

function cloneClip<T extends CompositionClip>(clip: T): T {
  if((clip.kind==='video'||clip.kind==='image')&&clip.concatAnimation)clip={...clip,concatAnimation:cloneGraphicValue(clip.concatAnimation)}
  if((clip.kind==='audio'||clip.kind==='video')&&clip.concatGainKeys)clip={...clip,concatGainKeys:cloneGraphicValue(clip.concatGainKeys)}
  if (clip.kind === 'graphic') return cloneGraphicValue(clip)
  if (clip.kind === 'video' || clip.kind === 'image' || clip.kind === 'color') {
    return {
      ...clip,
      transform: { ...clip.transform },
      ...((clip.kind === 'image' || clip.kind === 'video') && clip.effects ? {effects:rebaseNativeEffects(clip.effects)} : {}),
      ...(clip.kind === 'color' && clip.colorAnimation ? { colorAnimation: rebaseColorAnimation(clip.colorAnimation) } : {}),
      ...(clip.position ? { position: rebasePosition(clip.position,0) } : {}),
      ...(clip.animation ? { animation: cloneVisualAnimation(clip.animation) } : {}),
      ...(clip.kind === 'video' && clip.dspEffects ? {dspEffects:rebaseDspEffects(clip.dspEffects)}:{}),
      ...(clip.kind === 'video' && clip.videoEffects ? { videoEffects: clip.videoEffects.map((effect) => ({ ...effect })) } : {}),
      ...(clip.kind === 'video' && clip.chromaKey ? { chromaKey: { ...clip.chromaKey } } : {}),
      ...(clip.kind === 'video' && clip.masks ? { masks: cloneVideoMasks(clip.masks) } : {}),
      ...(clip.kind === 'video' && clip.audioAnimation ? { audioAnimation: cloneAudioAnimation(clip.audioAnimation) } : {}),
      ...(clip.kind === 'video' && clip.playbackMode ? { playbackMode: { ...clip.playbackMode } } : {}),
      ...(clip.kind === 'video' && clip.stabilization ? { stabilization: { ...clip.stabilization } } : {}),
      ...(clip.kind === 'video' && clip.speedRamp ? { speedRamp: cloneCompositionSpeedRamp(clip.speedRamp) } : {}),
    } as T
  }
  if (clip.kind === 'text') {
    return {
      ...clip,
      style: { ...clip.style },
      ...(clip.colorAnimation ? { colorAnimation: rebaseColorAnimation(clip.colorAnimation) } : {}),
      ...(clip.position ? { position: rebasePosition(clip.position,0) } : {}),
      ...(clip.animation ? { animation: cloneVisualAnimation(clip.animation) } : {}),
    } as T
  }
  return {
    ...clip,
    ...(clip.kind === 'audio' && clip.dspEffects ? { dspEffects: rebaseDspEffects(clip.dspEffects) } : {}),
    ...(clip.kind === 'audio' && clip.audioAnimation ? { audioAnimation: cloneAudioAnimation(clip.audioAnimation) } : {}),
    ...(clip.kind === 'audio' && clip.speedRamp ? { speedRamp: cloneCompositionSpeedRamp(clip.speedRamp) } : {}),
  }
}

function trimRampedSourceClip<T extends VideoClip | AudioClip>(
  clip: T,
  timelineStartTicks: number,
  localStartTicks: number,
  localEndTicks: number,
): T {
  const ramp = clip.speedRamp!
  const baseline = clip.speed ?? 1
  const sourceSpan = clip.sourceOutTicks - clip.sourceInTicks
  const startProgress = speedRampSourceProgressAtTimelineTick(sourceSpan, baseline, ramp, localStartTicks)
  const endProgress = speedRampSourceProgressAtTimelineTick(sourceSpan, baseline, ramp, localEndTicks)
  const sliced = sliceCompositionSpeedRamp(sourceSpan, baseline, ramp, startProgress, endProgress)
  const reverse = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
  const sourceInTicks = reverse
    ? clip.sourceOutTicks - endProgress
    : clip.sourceInTicks + startProgress
  const sourceOutTicks = reverse
    ? clip.sourceOutTicks - startProgress
    : clip.sourceInTicks + endProgress
  const slicedDuration = speedRampTimelineDurationTicks(
    sourceOutTicks - sourceInTicks,
    sliced.speed,
    sliced.speedRamp,
  )
  // When only the in-edge is trimmed, preserve the authored out-edge. Integer
  // source progress can make the representable start differ by one tick.
  const snappedStart = localEndTicks === clipDurationTicks(clip)
    ? clipEndTicks(clip) - slicedDuration
    : timelineStartTicks
  return {
    ...clip,
    timelineStartTicks: snappedStart,
    sourceInTicks,
    sourceOutTicks,
    speed: sliced.speed,
    speedRamp: sliced.speedRamp,
  }
}

function splitRampedSourceClip<T extends VideoClip | AudioClip>(
  clip: T,
  requestedLeftDurationTicks: number,
  rightClipId: string,
): { readonly left: T; readonly right: T } {
  const ramp = clip.speedRamp!
  const baseline = clip.speed ?? 1
  const sourceSpan = clip.sourceOutTicks - clip.sourceInTicks
  const approximateBoundary = speedRampSourceProgressAtTimelineTick(
    sourceSpan,
    baseline,
    ramp,
    requestedLeftDurationTicks,
  )
  const originalDuration = speedRampTimelineDurationTicks(sourceSpan, baseline, ramp)
  const split = findRepresentableRampSplit(
    sourceSpan,
    baseline,
    ramp,
    approximateBoundary,
    requestedLeftDurationTicks,
    originalDuration,
  )
  const { boundary, leftRamp, rightRamp, leftDuration } = split
  const reverse = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
  const sourceBoundary = reverse
    ? clip.sourceOutTicks - boundary
    : clip.sourceInTicks + boundary
  const left = {
    ...clip,
    sourceInTicks: reverse ? sourceBoundary : clip.sourceInTicks,
    sourceOutTicks: reverse ? clip.sourceOutTicks : sourceBoundary,
    speed: leftRamp.speed,
    speedRamp: leftRamp.speedRamp,
  } as T
  const right = {
    ...clip,
    id: rightClipId,
    // Independent cumulative rounding can move a mathematically exact split by
    // one tick. Snap the edit to a nearby source tick whose two backend
    // integrals preserve the original total, then place the right clip at the
    // left clip's actual end so the split can never create a gap or overlap.
    timelineStartTicks: clip.timelineStartTicks + leftDuration,
    sourceInTicks: reverse ? clip.sourceInTicks : sourceBoundary,
    sourceOutTicks: reverse ? sourceBoundary : clip.sourceOutTicks,
    speed: rightRamp.speed,
    speedRamp: rightRamp.speedRamp,
  } as T
  return { left, right }
}

function findRepresentableRampSplit(
  sourceSpan: number,
  baseline: number,
  ramp: NonNullable<VideoClip['speedRamp']>,
  approximateBoundary: number,
  requestedLeftDuration: number,
  originalDuration: number,
): {
  readonly boundary: number
  readonly leftRamp: ReturnType<typeof sliceCompositionSpeedRamp>
  readonly rightRamp: ReturnType<typeof sliceCompositionSpeedRamp>
  readonly leftDuration: number
} {
  const maxDistance = Math.min(4_096, sourceSpan - 1)
  let best: ReturnType<typeof candidateAt> | undefined

  for (let distance = 0; distance <= maxDistance; distance += 1) {
    const candidates = distance === 0
      ? [approximateBoundary]
      : [approximateBoundary - distance, approximateBoundary + distance]
    for (const boundary of candidates) {
      const candidate = candidateAt(boundary)
      if (!candidate || candidate.totalDuration !== originalDuration) continue
      if (!best || candidate.leftError < best.leftError) best = candidate
      if (candidate.leftError === 0) return candidate
    }
  }
  if (best) return best
  throw commandError(
    'invalid-range',
    'Speed ramp split cannot be represented without changing duration; move the split point slightly',
  )

  function candidateAt(boundary: number) {
    if (!Number.isSafeInteger(boundary) || boundary <= 0 || boundary >= sourceSpan) return undefined
    try {
      const leftRamp = sliceCompositionSpeedRamp(sourceSpan, baseline, ramp, 0, boundary)
      const rightRamp = sliceCompositionSpeedRamp(sourceSpan, baseline, ramp, boundary, sourceSpan)
      const leftDuration = speedRampTimelineDurationTicks(boundary, leftRamp.speed, leftRamp.speedRamp)
      const rightDuration = speedRampTimelineDurationTicks(
        sourceSpan - boundary,
        rightRamp.speed,
        rightRamp.speedRamp,
      )
      return {
        boundary,
        leftRamp,
        rightRamp,
        leftDuration,
        totalDuration: leftDuration + rightDuration,
        leftError: Math.abs(leftDuration - requestedLeftDuration),
      }
    } catch {
      return undefined
    }
  }
}

export function freezeSourceRange(
  composition: Composition,
  clip: VideoClip,
  timelineDurationTicks: number,
): { readonly sourceInTicks: number; readonly sourceOutTicks: number } {
  const playbackMode = clip.playbackMode
  if (playbackMode?.mode !== 'freeze') throw commandError('invalid-range', 'Freeze source range requires freeze playback')
  const source = composition.sources[clip.sourceId]
  const sourceSpan = Math.max(1, Math.round(timelineDurationTicks * (clip.speed ?? 1)))
  if (!source || sourceSpan > source.durationTicks) {
    throw commandError('invalid-range', 'Trimmed freeze clip exceeds its source duration')
  }
  const originalOffset = Math.max(0, playbackMode.sourceTick - clip.sourceInTicks)
  const desiredStart = playbackMode.sourceTick - Math.min(originalOffset, sourceSpan - 1)
  const sourceInTicks = Math.max(0, Math.min(source.durationTicks - sourceSpan, desiredStart))
  return { sourceInTicks, sourceOutTicks: sourceInTicks + sourceSpan }
}

function splitClipAutomation<T extends CompositionClip>(clip: T, cut: number, right: boolean): T {
  if (clip.kind === 'graphic') clip = { ...clip, animationOffsetUs: (clip.animationOffsetUs ?? 0) + (right ? cut : 0) }
  if ((clip.kind === 'text' || clip.kind === 'color') && clip.colorAnimation) clip = { ...clip,colorAnimation: rebaseColorAnimation(clip.colorAnimation,right ? -cut : 0) }
  const duration = clipDurationTicks(clip)
  if (clip.kind === 'audio') return { ...clip, dspEffects: rebaseDspEffects(clip.dspEffects, right ? -cut : 0), audioAnimation: splitAudioAnimation(clip.audioAnimation, cut, right), fadeInTicks: right ? 0 : clip.fadeInTicks ?? 0, fadeOutTicks: right ? clip.fadeOutTicks ?? 0 : 0 } as T
  return { ...clip, ...((clip.kind === 'image' || clip.kind === 'video') && clip.effects ? {effects:rebaseNativeEffects(clip.effects,right ? -cut : 0)} : {}), ...(clip.position ? { position: rebasePosition(clip.position,right ? -cut : 0) } : {}), ...(clip.animation ? { animation: splitVisualAnimation(clip.animation, cut, right) } : {}), ...(clip.kind === 'video' ? { ...(clip.dspEffects?{dspEffects:rebaseDspEffects(clip.dspEffects,right ? -cut : 0)}:{}),audioFadeInTicks:right?0:clip.audioFadeInTicks??0,audioFadeOutTicks:right?clip.audioFadeOutTicks??0:0, ...(clip.audioAnimation ? { audioAnimation: splitAudioAnimation(clip.audioAnimation, cut, right) } : {}), ...(clip.masks ? { masks: splitVideoMasks(clip.masks, cut, right) } : {}) } : {}) } as T
}

export function shiftClipAutomation<T extends CompositionClip>(clip: T, delta: number): T {
  if (clip.kind === 'graphic') clip = { ...clip, animationOffsetUs: (clip.animationOffsetUs ?? 0) - delta }
  if ((clip.kind === 'text' || clip.kind === 'color') && clip.colorAnimation) clip = { ...clip,colorAnimation: rebaseColorAnimation(clip.colorAnimation,delta) }
  const duration = clipDurationTicks(clip)
  if (clip.kind === 'audio') return { ...clip, dspEffects: rebaseDspEffects(clip.dspEffects, delta), audioAnimation: shiftAudioAnimation(clip.audioAnimation, delta) } as T
  return { ...clip, ...((clip.kind === 'image' || clip.kind === 'video') && clip.effects ? {effects:rebaseNativeEffects(clip.effects,delta)} : {}), ...(clip.position ? { position: rebasePosition(clip.position,delta) } : {}), ...(clip.animation ? { animation: shiftVisualAnimation(clip.animation, delta) } : {}), ...(clip.kind === 'video' ? { ...(clip.dspEffects?{dspEffects:rebaseDspEffects(clip.dspEffects,delta)}:{}), ...(clip.audioAnimation ? { audioAnimation: shiftAudioAnimation(clip.audioAnimation, delta) } : {}), ...(clip.masks ? { masks: shiftVideoMasks(clip.masks, delta) } : {}) } : {}) } as T
}

function withTimelineStart(clip: CompositionClip, timelineStartTicks: number): CompositionClip {
  return { ...clip, timelineStartTicks }
}

function sortClips<T extends CompositionClip>(clips: readonly T[]): T[] {
  return [...clips].sort(
    (left, right) => left.timelineStartTicks - right.timelineStartTicks || clipEndTicks(left) - clipEndTicks(right) || left.id.localeCompare(right.id),
  )
}

function ensureTimelineRange(clip: CompositionClip): void {
  const duration = clipDurationTicks(clip)
  const end = clipEndTicks(clip)
  if (
    !isSafeTick(clip.timelineStartTicks) ||
    !Number.isSafeInteger(duration) ||
    duration <= 0 ||
    !Number.isSafeInteger(end) ||
    end > MAX_COMPOSITION_DURATION_TICKS
  ) {
    throw commandError('invalid-range', 'Clip range is outside the composition limit')
  }
}

function ensureCompatible(track: CompositionTrack, clip: CompositionClip): void {
  if (!canMoveClipToTrack(clip, track)) {
    throw commandError('track-kind', `${clip.kind} clip cannot be placed on ${track.kind} track`)
  }
}

function ensureUnlocked(track: CompositionTrack, clip?: CompositionClip): void {
  if (clip?.locked) throw commandError('clip-locked', `Clip ${clip.id} is locked`)
  if (track.locked) throw commandError('track-locked', `Track ${track.id} is locked`)
}

function ensureUniqueClipId(composition: Composition, id: string): void {
  if (!isStableId(id)) throw commandError('invalid-id', 'Clip id is not a stable id')
  if (composition.tracks.some((track) => track.clips.some((clip) => clip.id === id))) {
    throw commandError('duplicate-id', `Clip ${id} already exists`)
  }
}

function findTrackIndex(composition: Composition, trackId: string): number {
  const index = composition.tracks.findIndex((track) => track.id === trackId)
  if (index < 0) throw commandError('missing-track', `Track ${trackId} does not exist`)
  return index
}

function copyMediaVersions(document: Composition, from: string, to: string): Composition {
  const versions = document.metadata?.mediaVersions?.[from]
  return versions ? { ...document, metadata: { ...document.metadata, mediaVersions: { ...document.metadata?.mediaVersions, [to]: { ...versions } } } } : document
}

function finalize(composition: Composition): Composition {
  if (composition.metadata?.mediaVersions) {
    const ids = new Set(composition.tracks.flatMap(t => t.clips.map(c => c.id)))
    composition = { ...composition, metadata: { ...composition.metadata, mediaVersions: Object.fromEntries(Object.entries(composition.metadata.mediaVersions).filter(([id]) => ids.has(id))) } }
  }
  composition = pruneCompositionLinks(composition)
  assertValidComposition(composition)
  return composition
}

function commandError(code: CompositionCommandErrorCode, message: string): CompositionCommandError {
  return new CompositionCommandError(code, message)
}

function snapClipGridStart(composition: Composition, clip: CompositionClip, proposedStart: number): number {
  return snapTimeGridTicks(proposedStart, clip.kind === 'audio' ? AUDIO_TIME_GRID : canvasTimeGrid(composition.canvas))
}

export function withClipGridStart(composition: Composition, clip: CompositionClip, proposedStart: number, resnap = false): CompositionClip {
  if (!resnap && proposedStart === clip.timelineStartTicks) return withTimelineStart(clip, proposedStart)
  const span = shiftTimeGridSpan(clip.timelineStartTicks, clipEndTicks(clip), proposedStart - clip.timelineStartTicks, clip.kind === 'audio' ? AUDIO_TIME_GRID : canvasTimeGrid(composition.canvas))
  const durationTicks = span.end - span.start
  if (clip.kind === 'video' || clip.kind === 'audio') {
    const moved = { ...clip, timelineStartTicks: span.start }
    if (durationTicks === clipDurationTicks(clip)) return moved
    return shiftClipAutomation({ ...moved, timelineDurationTicks: durationTicks }, 0)
  }
  return { ...clip, timelineStartTicks: span.start, durationTicks }
}


/** Retains the full curve, including keys outside the edited clip interval. */
function rebasePosition(position: import('./types').CompositionPositionAnimation,delta: number): import('./types').CompositionPositionAnimation {
  const rebase = (value: import('./types').CompositionAnimatableValue) => delta === 0 ? cloneAnimatableValue(value) : shiftAnimatableValue(value,delta)
  if (position.mode === 'xy') return { mode: 'xy',x: rebase(position.x),y: rebase(position.y) }
  return { mode: 'path',path: { nodes: position.path.nodes.map(node => ({ ...node,point: { ...node.point },inHandle: { ...node.inHandle },outHandle: { ...node.outHandle } })) },progress: rebase(position.progress) }
}
