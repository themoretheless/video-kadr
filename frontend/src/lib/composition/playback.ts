import {
  clipDurationTicks,
  type AudioClip,
  type CompositionPlaybackMode,
  type VideoClip,
} from './types'
import { speedAtSourceProgress, speedRampSourceProgressAtTimelineTick } from './speedRamp'

export const FORWARD_PLAYBACK_MODE: CompositionPlaybackMode = { mode: 'forward' }

export function videoPlaybackMode(clip: VideoClip): CompositionPlaybackMode {
  return clip.playbackMode ?? FORWARD_PLAYBACK_MODE
}

/**
 * Map a composition playhead tick to the source tick shown by a video clip.
 * The result stays inside the half-open authored source range.
 */
export function videoSourceTickAtTimelineTick(clip: VideoClip, timelineTick: number): number {
  const mode = videoPlaybackMode(clip)
  if (mode.mode === 'freeze') return mode.sourceTick

  const sourceOffset = clipSourceProgressAtTimelineTick(clip, timelineTick)
  if (mode.mode === 'reverse') {
    return Math.max(clip.sourceInTicks, clip.sourceOutTicks - 1 - sourceOffset)
  }
  return Math.min(clip.sourceOutTicks - 1, clip.sourceInTicks + sourceOffset)
}

export function sourceClipTickAtTimelineTick(clip: VideoClip | AudioClip, timelineTick: number): number {
  if (clip.kind === 'video') return videoSourceTickAtTimelineTick(clip, timelineTick)
  const sourceOffset = clipSourceProgressAtTimelineTick(clip, timelineTick)
  if (clip.reversed) return Math.max(clip.sourceInTicks, clip.sourceOutTicks - 1 - sourceOffset)
  return Math.min(clip.sourceOutTicks - 1, clip.sourceInTicks + sourceOffset)
}

export function clipSpeedAtTimelineTick(clip: VideoClip | AudioClip, timelineTick: number): number {
  const baseline = clip.speed ?? 1
  if (!clip.speedRamp) return baseline
  const sourceSpan = clip.sourceOutTicks - clip.sourceInTicks
  const progress = clipSourceProgressAtTimelineTick(clip, timelineTick)
  return speedAtSourceProgress(sourceSpan, baseline, clip.speedRamp, progress)
}

function clipSourceProgressAtTimelineTick(clip: VideoClip | AudioClip, timelineTick: number): number {
  const duration = clipDurationTicks(clip)
  const localTick = Math.max(
    0,
    Math.min(Math.max(0, duration - 1), Math.round(timelineTick - clip.timelineStartTicks)),
  )
  const sourceSpan = clip.sourceOutTicks - clip.sourceInTicks
  const progress = clip.speedRamp
    ? speedRampSourceProgressAtTimelineTick(sourceSpan, clip.speed ?? 1, clip.speedRamp, localTick)
    : Math.round(localTick * (clip.speed ?? 1))
  return Math.max(0, Math.min(sourceSpan - 1, progress))
}

export function videoPlaybackIsForward(clip: VideoClip): boolean {
  return videoPlaybackMode(clip).mode === 'forward'
}
