import {extractCaptionSpan} from './captionTime'
import { assertCompositionLocksPreserved } from './locks'
import {
  CompositionCommandError,
  withClipGridStart,
  deleteClip,
  findClipLocation,
} from './commands'
import {
  clipEndTicks,
  type AudioClip,
  type CaptionTrack,
  type Composition,
  type CompositionClip,
  type CompositionTrack,
  type ImageClip,
  type TextClip,
  type VideoClip,
} from './types'
import { assertValidComposition, isSafeTick } from './validation'

/** Close the union of the exact selection's vacated intervals across all tracks.
 * Semantics adapted from WeftCut ripple/plan.ts (MIT, docs/licenses/weftcut-MIT.txt).
 * Static visual endpoints use the canvas grid; source clips still retain source-derived duration.
 */
export function rippleDeleteClips(composition: Composition, clipIds: readonly string[]): Composition {
  assertValidComposition(composition)
  if (!Array.isArray(clipIds) || !clipIds.length || clipIds.length > 512) {
    throw new CompositionCommandError('invalid-range', 'Ripple delete requires 1–512 clips')
  }
  const doomed = new Set(clipIds)
  const selected = [...doomed].map((id) => findClipLocation(composition, id))
  for (const { track } of selected) if (track.locked) {
    throw new CompositionCommandError('track-locked', `Track ${track.id} is locked`)
  }
  const remaining = composition.tracks.flatMap((track) =>
    track.clips.filter((clip) => !doomed.has(clip.id)).map((clip) => ({ track, clip })),
  )
  const raw = selected.flatMap(({ track, clip }) => {
    let prevEnd = -Infinity
    let nextStart = Infinity
    for (const other of remaining) {
      if (other.track.id !== track.id) continue
      if (other.clip.timelineStartTicks < clip.timelineStartTicks) prevEnd = Math.max(prevEnd, clipEndTicks(other.clip))
      else nextStart = Math.min(nextStart, other.clip.timelineStartTicks)
    }
    const start = Math.max(clip.timelineStartTicks, prevEnd)
    const end = Math.min(clipEndTicks(clip), nextStart)
    return end > start ? [{ start, end }] : []
  }).sort((a, b) => a.start - b.start)
  return closeRippleIntervals(composition, doomed, raw)
}

export interface CompositionGap { readonly start: number; readonly end: number }

/** Exact bounded blank interval; leading space counts, trailing space does not. */
export function compositionGapAt(clips: readonly CompositionClip[], tick: number): CompositionGap | null {
  if (!isSafeTick(tick)) return null
  let start = 0
  let end = Infinity
  for (const clip of clips) {
    const clipEnd = clipEndTicks(clip)
    if (clip.timelineStartTicks <= tick && tick < clipEnd) return null
    if (clipEnd <= tick) start = Math.max(start, clipEnd)
    else end = Math.min(end, clip.timelineStartTicks)
  }
  return end === Infinity ? null : { start, end }
}

/** Bounded gaps in timeline order, independent of clip array order/overlap. */
export function compositionGaps(clips: readonly CompositionClip[]): readonly CompositionGap[] {
  const gaps: CompositionGap[] = []
  let cursor = 0
  for (const clip of [...clips].sort((a, b) => a.timelineStartTicks - b.timelineStartTicks)) {
    if (clip.timelineStartTicks > cursor) gaps.push({ start: cursor, end: clip.timelineStartTicks })
    cursor = Math.max(cursor, clipEndTicks(clip))
  }
  return gaps
}

export function rippleDeleteGap(composition: Composition, trackId: string, start: number, end: number): Composition {
  assertValidComposition(composition)
  const track = composition.tracks.find((item) => item.id === trackId)
  if (!track) throw new CompositionCommandError('missing-track', `Track ${trackId} does not exist`)
  const gap = compositionGapAt(track.clips, start)
  if (!isSafeTick(end) || !gap || gap.start !== start || gap.end !== end || end <= start) {
    throw new CompositionCommandError('invalid-range', 'Gap no longer exists at the exact supplied boundaries')
  }
  return closeRippleIntervals(composition, new Set(), [gap])
}

function closeRippleIntervals(composition: Composition, doomed: ReadonlySet<string>, raw: readonly CompositionGap[]): Composition {
  const remaining = composition.tracks.flatMap((track) =>
    track.clips.filter((clip) => !doomed.has(clip.id)).map((clip) => ({ track, clip })),
  )
  const holes: { start: number; end: number }[] = []
  for (const hole of raw) {
    const last = holes.at(-1)
    if (last && hole.start <= last.end) last.end = Math.max(last.end, hole.end)
    else holes.push({ ...hole })
  }
  const linkedToDeleted = new Set((composition.links ?? [])
    .filter((link) => link.memberIds.some((id) => doomed.has(id))).flatMap((link) => link.memberIds))
  for (const hole of holes) {
    for (const { clip } of remaining) {
      if (clip.timelineStartTicks < hole.start || clip.timelineStartTicks >= hole.end) continue
      if (hole.start === 0 && clip.timelineStartTicks === 0 && !linkedToDeleted.has(clip.id)) continue
      throw new CompositionCommandError('invalid-range', `Ripple: clip ${clip.id} starts inside the deleted interval; include it in the selection`)
    }
    for (const link of composition.links ?? []) {
      const members = remaining.filter(({ clip }) => link.memberIds.includes(clip.id))
      if (members.some(({ clip }) => clip.timelineStartTicks <= hole.start && clipEndTicks(clip) > hole.start)
        && members.some(({ clip }) => clip.timelineStartTicks >= hole.end)) {
        throw new CompositionCommandError('invalid-range', `Ripple: link ${link.id} straddles the deleted interval`)
      }
    }
  }
  const deltas = new Map(remaining.map(({ clip, track }) => {
    const delta = holes.reduce((sum, hole) => sum + (hole.end <= clip.timelineStartTicks ? hole.end - hole.start : 0), 0)
    if (delta && clip.locked) throw new CompositionCommandError('clip-locked', `Clip ${clip.id} is locked`)
    if (delta && track.locked) throw new CompositionCommandError('track-locked', `Track ${track.id} is locked`)
    return [clip.id, delta] as const
  }))
  // Deletion cleans transition endpoints and dissolved links; no live state is touched.
  let next = composition
  for (const id of doomed) next = deleteClip(next, id)
  const tracks = next.tracks.map((track) => {
    if (track.kind === 'video') for (const transition of track.transitions ?? []) {
      if (deltas.get(transition.fromClipId ?? '') !== deltas.get(transition.toClipId ?? '')) {
        throw new CompositionCommandError('transition-conflict', 'Ripple would change a transition overlap')
      }
    }
    return { ...track, clips: track.clips.map((clip) => withClipGridStart(composition, clip, clip.timelineStartTicks - (deltas.get(clip.id) ?? 0))) } as CompositionTrack
  })
  next = { ...next, tracks, ...(next.captionTracks ? { captionTracks: extractCaptionRanges(next.captionTracks, holes) } : {}) }
  assertValidComposition(next)
  assertCompositionLocksPreserved(composition, next)
  return next
}

/** Filmcraft edit/captions.rs extract_range (MIT): join surviving cue portions
 * across each closed range. Descending original coordinates prevent double shifts.
 */
export function extractCaptionRanges(tracks: readonly CaptionTrack[], holes: readonly CompositionGap[]): readonly CaptionTrack[] {
  return tracks.map((track) => {
    if (track.locked || track.syncLock === false || !holes.length) return track
    let clips = track.clips
    for (const { start: a, end: b } of [...holes].sort((left, right) => right.start - left.start)) {
      clips = clips.flatMap((cue) => {
        const span=extractCaptionSpan(cue.timelineStartTick,cue.timelineEndTick,a,b)
        return span?[{...cue,timelineStartTick:span[0],timelineEndTick:span[1]}]:[]
      })
    }
    return { ...track, clips }
  })
}

export function rippleDeleteClip(composition: Composition, clipId: string): Composition {
  return rippleDeleteClips(composition, [clipId])
}

/**
 * Close every gap from `anchorTicks` onward on one track. Clips crossing the
 * anchor remain fixed; later clips are packed after the preceding clip.
 */
export function magnetizeTrack(
  composition: Composition,
  trackId: string,
  anchorTicks = 0,
): Composition {
  assertValidComposition(composition)
  if (!isSafeTick(anchorTicks)) {
    throw new CompositionCommandError('invalid-range', 'Track magnet anchor must be a safe tick')
  }
  const trackIndex = composition.tracks.findIndex((track) => track.id === trackId)
  if (trackIndex < 0) {
    throw new CompositionCommandError('missing-track', `Track ${trackId} does not exist`)
  }
  const track = composition.tracks[trackIndex]!
  if (track.locked) {
    throw new CompositionCommandError('track-locked', `Track ${track.id} is locked`)
  }

  let cursor = anchorTicks
  const clips = [...track.clips]
    .sort((left, right) => left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id))
    .map((clip) => {
      const end = clipEndTicks(clip)
      if (end <= anchorTicks || clip.timelineStartTicks < anchorTicks) {
        cursor = Math.max(cursor, end)
        return clip
      }
      const packed = withClipGridStart(composition, clip, cursor)
      cursor = clipEndTicks(packed)
      return packed
    })

  return replaceTrackClips(composition, trackIndex, clips)
}

function replaceTrackClips(
  composition: Composition,
  trackIndex: number,
  clips: readonly CompositionClip[],
): Composition {
  const track = composition.tracks[trackIndex]!
  let replacement: CompositionTrack
  switch (track.kind) {
    case 'video':
      replacement = { ...track, clips: clips as readonly VideoClip[] }
      break
    case 'audio':
      replacement = { ...track, clips: clips as readonly AudioClip[] }
      break
    case 'image':
      replacement = { ...track, clips: clips as readonly ImageClip[] }
      break
    case 'graphic':
      replacement = { ...track, clips: clips as typeof track.clips }
      break
    case 'color':
      replacement = { ...track, clips: clips as typeof track.clips }
      break
    case 'text':
      replacement = { ...track, clips: clips as readonly TextClip[] }
      break
  }
  const tracks = [...composition.tracks]
  tracks[trackIndex] = replacement
  const next = { ...composition, tracks }
  assertValidComposition(next)
  assertCompositionLocksPreserved(composition, next)
  return next
}
