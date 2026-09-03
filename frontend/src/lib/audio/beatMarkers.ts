import { COMPOSITION_TIME_BASE, clipEndTicks, type Composition, type CompositionClip } from '../composition/types'
import { speedRampTimelineTickAtSourceProgress } from '../composition/speedRamp'
import type { BeatDetectionResult } from './beats'

export interface TimelineBeat {
  readonly tick: number
  readonly strength: number
}

/** Map source-domain onsets into the exact constant-speed composition range. */
export function mapDetectedBeatsToTimeline(
  composition: Composition,
  clipId: string,
  detection: BeatDetectionResult,
): readonly TimelineBeat[] {
  const location = findClip(composition, clipId)
  if (!location) throw new Error(`Audio clip ${clipId} не найден`)
  const { clip, muted } = location
  if (clip.kind === 'image' || clip.kind === 'text') throw new Error('Auto Beat требует audio/video clip')
  if (muted) throw new Error('Auto Beat source track выключена')
  if (clip.kind === 'video') {
    if (!clip.sourceAudioEnabled) throw new Error('Source audio выключен для выбранного video clip')
    if ((clip.playbackMode?.mode ?? 'forward') === 'freeze') {
      throw new Error('Freeze clip не содержит source audio для Auto Beat')
    }
  }
  const source = composition.sources[clip.sourceId]
  if (!source?.hasAudio) throw new Error('Выбранный source не содержит подтверждённого audio stream')
  if ('speedRamp' in clip && clip.speedRamp?.audioPolicy === 'mute') {
    throw new Error('Speed-ramp audio выключен для выбранного clip')
  }
  const speed = clip.speed ?? 1
  if (!Number.isFinite(speed) || speed <= 0) throw new Error('Clip speed is invalid')

  const result: TimelineBeat[] = []
  for (const beat of detection.beats) {
    if (!Number.isFinite(beat.timeSeconds) || !Number.isFinite(beat.strength)) continue
    const sourceTick = Math.round(beat.timeSeconds * COMPOSITION_TIME_BASE)
    if (sourceTick < clip.sourceInTicks || sourceTick >= clip.sourceOutTicks) continue
    const sourceProgress = clip.kind === 'video' && clip.playbackMode?.mode === 'reverse'
      ? clip.sourceOutTicks - sourceTick
      : sourceTick - clip.sourceInTicks
    const localTimelineTick = 'speedRamp' in clip && clip.speedRamp
      ? speedRampTimelineTickAtSourceProgress(
          clip.sourceOutTicks - clip.sourceInTicks,
          speed,
          clip.speedRamp,
          sourceProgress,
        )
      : Math.round(sourceProgress / speed)
    const tick = clip.timelineStartTicks + localTimelineTick
    if (tick < clip.timelineStartTicks || tick >= clipEndTicks(clip)) continue
    const previous = result[result.length - 1]
    if (previous?.tick === tick) {
      if (beat.strength > previous.strength) result[result.length - 1] = { tick, strength: beat.strength }
    } else {
      result.push({ tick, strength: beat.strength })
    }
  }
  return result.sort((left, right) => left.tick - right.tick)
}

function findClip(composition: Composition, id: string): { clip: CompositionClip; muted: boolean } | null {
  for (const track of composition.tracks) {
    const clip = track.clips.find((candidate) => candidate.id === id)
    if (clip) return { clip, muted: track.kind === 'audio' || track.kind === 'video' ? track.muted : false }
  }
  return null
}
