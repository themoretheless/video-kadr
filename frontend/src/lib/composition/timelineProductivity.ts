import {
  CompositionCommandError,
  deleteClip,
  findClipLocation,
} from './commands'
import {
  clipEndTicks,
  type AudioClip,
  type Composition,
  type CompositionClip,
  type CompositionTrack,
  type ImageClip,
  type TextClip,
  type VideoClip,
} from './types'
import { assertValidComposition, isSafeTick } from './validation'

/**
 * Remove one clip and close exactly that interval on its track. Other tracks
 * keep their timing, which makes the operation deterministic around overlays.
 */
export function rippleDeleteClip(composition: Composition, clipId: string): Composition {
  assertValidComposition(composition)
  const location = findClipLocation(composition, clipId)
  if (location.track.locked) {
    throw new CompositionCommandError('track-locked', `Track ${location.track.id} is locked`)
  }
  const removedStart = location.clip.timelineStartTicks
  const removedEnd = clipEndTicks(location.clip)
  const delta = removedEnd - removedStart
  const deleted = deleteClip(composition, clipId)
  const trackIndex = deleted.tracks.findIndex((track) => track.id === location.track.id)
  const track = deleted.tracks[trackIndex]!
  const clips = track.clips.map((clip) =>
    clip.timelineStartTicks >= removedEnd
      ? withTimelineStart(clip, clip.timelineStartTicks - delta)
      : clip,
  )
  return replaceTrackClips(deleted, trackIndex, clips)
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
      const packed = withTimelineStart(clip, cursor)
      cursor = clipEndTicks(packed)
      return packed
    })

  return replaceTrackClips(composition, trackIndex, clips)
}

function withTimelineStart<C extends CompositionClip>(clip: C, timelineStartTicks: number): C {
  return { ...clip, timelineStartTicks } as C
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
    case 'text':
      replacement = { ...track, clips: clips as readonly TextClip[] }
      break
  }
  const tracks = [...composition.tracks]
  tracks[trackIndex] = replacement
  const next = { ...composition, tracks }
  assertValidComposition(next)
  return next
}
