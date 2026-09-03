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
  cloneAudioAnimation,
  cloneVisualAnimation,
  cloneVideoMasks,
  sliceAudioAnimation,
  sliceVisualAnimation,
  sliceVideoMasks,
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
  return finalize({
    ...composition,
    tracks: composition.tracks.filter((track) => track.id !== trackId),
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
): Composition {
  assertValidComposition(composition)
  if (!isSafeTick(timelineStartTicks)) throw commandError('invalid-range', 'Clip start must be a non-negative safe tick')
  const location = findClipLocation(composition, clipId)
  const targetTrackIndex = findTrackIndex(composition, targetTrackId)
  const targetTrack = composition.tracks[targetTrackIndex]!
  ensureUnlocked(location.track)
  if (targetTrack.id !== location.track.id) ensureUnlocked(targetTrack)
  ensureCompatible(targetTrack, location.clip)
  if (
    targetTrack.id !== location.track.id &&
    location.track.kind === 'video' &&
    (location.track.transitions ?? []).some((transition) => transition.fromClipId === clipId || transition.toClipId === clipId)
  ) {
    throw commandError('transition-conflict', 'Remove transitions before moving this clip to another track')
  }

  const moved = withTimelineStart(location.clip, timelineStartTicks)
  ensureTimelineRange(moved)
  if (targetTrack.id === location.track.id && timelineStartTicks === location.clip.timelineStartTicks) return composition

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

/** Move media under a fixed timeline range without changing clip placement. */
export function slipClip(composition: Composition, clipId: string, sourceDeltaTicks: number): Composition {
  assertValidComposition(composition)
  if (!Number.isSafeInteger(sourceDeltaTicks)) throw commandError('invalid-range', 'Slip delta must be a safe tick')
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track)
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
): Composition {
  assertValidComposition(composition)
  if (!isSafeTick(newStartTicks) || !isSafeTick(newEndTicks) || newEndTicks <= newStartTicks) {
    throw commandError('invalid-range', 'Trim must produce a positive safe range')
  }
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track)
  const oldStart = location.clip.timelineStartTicks
  const oldEnd = clipEndTicks(location.clip)
  const oldDuration = clipDurationTicks(location.clip)
  if (oldStart === newStartTicks && oldEnd === newEndTicks) return composition

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
  trimmed = sliceClipVisualData(trimmed, newStartTicks - oldStart, newEndTicks - oldStart, oldDuration)
  ensureTimelineRange(trimmed)
  return replaceLocatedClip(composition, location, trimmed)
}

export function splitClip(
  composition: Composition,
  clipId: string,
  atTicks: number,
  rightClipId: string,
): Composition {
  assertValidComposition(composition)
  ensureUniqueClipId(composition, rightClipId)
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track)
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
      const sourceBoundary = location.clip.sourceInTicks + Math.round(leftDuration * (location.clip.speed ?? 1))
      left = { ...location.clip, sourceOutTicks: sourceBoundary }
      right = {
        ...location.clip,
        id: rightClipId,
        timelineStartTicks: atTicks,
        sourceInTicks: sourceBoundary,
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
  left = sliceClipVisualData(left, 0, leftDuration, leftDuration + rightDuration)
  right = sliceClipVisualData(right, leftDuration, leftDuration + rightDuration, leftDuration + rightDuration)
  const clips = [...location.track.clips]
  clips.splice(location.clipIndex, 1, left, right)
  let replacement = withClips(location.track, sortClips(clips))
  if (replacement.kind === 'video') {
    replacement = {
      ...replacement,
      transitions: (replacement.transitions ?? []).map((transition) =>
        transition.fromClipId === clipId ? { ...transition, fromClipId: rightClipId } : transition,
      ),
    }
  }
  return finalize(replaceTrack(composition, location.trackIndex, replacement))
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
  ensureUnlocked(location.track)
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
  return finalize(replaceTrack(
    composition,
    location.trackIndex,
    withClips(location.track, sortClips(clips)),
  ))
}

export function deleteClip(composition: Composition, clipId: string): Composition {
  assertValidComposition(composition)
  const location = findClipLocation(composition, clipId)
  ensureUnlocked(location.track)
  let replacement = withClips(location.track, location.track.clips.filter((clip) => clip.id !== clipId))
  if (replacement.kind === 'video') {
    replacement = {
      ...replacement,
      transitions: (replacement.transitions ?? []).filter(
        (transition) => transition.fromClipId !== clipId && transition.toClipId !== clipId,
      ),
    }
  }
  return finalize(replaceTrack(composition, location.trackIndex, replacement))
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
  if (!isStableId(transition.id)) throw commandError('invalid-id', 'Transition id is not stable')
  const reason = compositionTransitionUnavailableReason(composition, trackId, transition)
  if (reason) throw commandError('transition-conflict', reason)
  const transitions = [...(track.transitions ?? []).filter((candidate) => candidate.id !== transition.id), { ...transition }]
    .sort((left, right) => left.fromClipId.localeCompare(right.fromClipId) || left.id.localeCompare(right.id))
  return finalize(replaceTrack(composition, trackIndex, { ...track, transitions }))
}

export function deleteTransition(composition: Composition, trackId: string, transitionId: string): Composition {
  assertValidComposition(composition)
  const trackIndex = findTrackIndex(composition, trackId)
  const track = composition.tracks[trackIndex]!
  ensureUnlocked(track)
  if (track.kind !== 'video') throw commandError('track-kind', 'Transitions require a video track')
  if (!(track.transitions ?? []).some((transition) => transition.id === transitionId)) {
    throw commandError('missing-clip', `Transition ${transitionId} does not exist`)
  }
  return finalize(replaceTrack(composition, trackIndex, {
    ...track,
    transitions: (track.transitions ?? []).filter((transition) => transition.id !== transitionId),
  }))
}

export interface DuplicateClipOptions {
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
  ensureUnlocked(location.track)
  const targetTrackId = options.targetTrackId ?? location.track.id
  const targetIndex = findTrackIndex(composition, targetTrackId)
  const target = composition.tracks[targetIndex]!
  if (target.id !== location.track.id) ensureUnlocked(target)
  ensureCompatible(target, location.clip)
  const start = options.timelineStartTicks ?? clipEndTicks(location.clip)
  if (!isSafeTick(start)) throw commandError('invalid-range', 'Duplicate start must be a non-negative safe tick')
  const duplicate = withTimelineStart({ ...cloneClip(location.clip), id: options.id } as CompositionClip, start)
  ensureTimelineRange(duplicate)
  return finalize(
    replaceTrack(composition, targetIndex, withClips(target, sortClips([...target.clips, duplicate]))),
  )
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
    case 'text':
      return { ...track, clips: track.clips.map(cloneClip) as TextClip[] } as TextTrack
  }
}

function cloneClip<T extends CompositionClip>(clip: T): T {
  if (clip.kind === 'video' || clip.kind === 'image') {
    return {
      ...clip,
      transform: { ...clip.transform },
      ...(clip.animation ? { animation: cloneVisualAnimation(clip.animation) } : {}),
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
      ...(clip.animation ? { animation: cloneVisualAnimation(clip.animation) } : {}),
    } as T
  }
  return {
    ...clip,
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

function freezeSourceRange(
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

function sliceClipVisualData<T extends CompositionClip>(
  clip: T,
  startTicks: number,
  endTicks: number,
  sourceDurationTicks: number,
): T {
  if (clip.kind === 'audio') {
    const duration = clipDurationTicks(clip)
    return {
      ...clip,
      ...(clip.audioAnimation ? {
        audioAnimation: sliceAudioAnimation(clip.audioAnimation, startTicks, endTicks, sourceDurationTicks),
      } : {}),
      fadeInTicks: Math.min(clip.fadeInTicks ?? 0, duration),
      fadeOutTicks: Math.min(clip.fadeOutTicks ?? 0, duration),
    } as T
  }
  return {
    ...clip,
    ...(clip.animation ? {
      animation: sliceVisualAnimation(clip.animation, startTicks, endTicks, sourceDurationTicks),
    } : {}),
    ...(clip.kind === 'video' && clip.masks ? {
      masks: sliceVideoMasks(clip.masks, startTicks, endTicks, sourceDurationTicks),
    } : {}),
    ...(clip.kind === 'video' && clip.audioAnimation ? {
      audioAnimation: sliceAudioAnimation(clip.audioAnimation, startTicks, endTicks, sourceDurationTicks),
    } : {}),
  } as T
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

function ensureUnlocked(track: CompositionTrack): void {
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

function finalize(composition: Composition): Composition {
  assertValidComposition(composition)
  return composition
}

function commandError(code: CompositionCommandErrorCode, message: string): CompositionCommandError {
  return new CompositionCommandError(code, message)
}
