import {
  findClipLocation,
  addTrack,
  addClip,
  moveClip,
  trimClip,
  slipClip,
  splitClip,
  deleteClip,
  duplicateClip,
  reorderTrack,
  upsertTransition,
  deleteTransition,
  collectSnapTargets,
  snapClipStart,
  snapTick,
  removeSilenceFromClip
} from '../../composition/commands.js'
import {
  clipDurationTicks,
  clipEndTicks,
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_DURATION_TICKS
} from '../../composition/types.js'
import {
  cloneAnimatableValue,
  constantAnimatable,
  deleteKeyframe,
  keyframeTickAtLocalTime,
  sampleAnimatableValue,
  sliceAudioAnimation,
  sliceVisualAnimation,
  sliceVideoMasks,
  updateInterpolation,
  updateKeyframe,
  upsertKeyframe
} from '../../composition/keyframes.js'
import {
  magnetizeTrack,
  rippleDeleteClip
} from '../../composition/timelineProductivity.js'
import {
  videoSourceTickAtTimelineTick
} from '../../composition/playback.js'
import {
  cloneCompositionSpeedRamp,
  minimumCompositionSpeed
} from '../../composition/speedRamp.js'
import {
  cuesToTextClips,
  formatSrt,
  parseTimecodedText,
  textClipsToCues
} from '../../subtitles/srt.js'
import type {
  SourceRange
} from '../../audio/silence.js'
import type {
  AudioClip,
  AudioTrack,
  Composition,
  CompositionBlendMode,
  CompositionChromaKey,
  CompositionFrameInterpolation,
  CompositionInterpolation,
  CompositionMaskShape,
  CompositionPlaybackMode,
  CompositionSpeedRamp,
  CompositionStabilization,
  CompositionTrack,
  CompositionTransition,
  CompositionTransitionKind,
  CompositionVideoMask,
  CompositionVideoStyleEffect,
  CompositionVisualAnimation,
  CompositionVisualProperty,
  TextClip,
  TextTrack,
  TrackKind,
  VideoClip
} from '../../composition/types.js'
import {
  DEFAULT_TEXT_DURATION_TICKS,
  clampNumber,
  clampTick,
  commitDocument,
  compositionState,
  findAvailableTrack,
  firstFreeStart,
  makeId,
  makeTrack,
  nextTrackName,
  selectCompositionClip,
  selectedCompositionTrack,
  snapThresholdTicks,
  updateClip,
  resolveAutomation,
  sampleAtPlayhead,
  requireVisualAutomation,
  requireVisualOverlay
} from './core.svelte.js'
import type {
  CompositionAutomationProperty,
  CompositionAutomationTarget
} from './core.svelte.js'

export function addTextToComposition(text = 'Текст'): string {
  let document = compositionState.document
  const start = compositionState.transport.playheadTicks
  const duration = Math.min(DEFAULT_TEXT_DURATION_TICKS, MAX_COMPOSITION_DURATION_TICKS - start)
  if (duration <= 0) throw new Error('Плейхед находится за пределом композиции')
  const track = findAvailableTrack(document, 'text', start, start + duration)
  let trackId = track?.id
  if (!trackId) {
    trackId = makeId('text-track')
    document = addTrack(document, makeTrack('text', trackId, nextTrackName(document, 'Текст')))
  }
  const clipId = makeId('text')
  const clip: TextClip = {
    id: clipId,
    kind: 'text',
    timelineStartTicks: start,
    durationTicks: duration,
    text: text.trim() || 'Текст',
    x: 0,
    y: 0,
    opacity: 1,
    rotationDegrees: 0,
    style: { fontSizePx: 56, color: '#ffffff', backgroundColor: '#00000000', align: 'center' },
  }
  commitDocument(addClip(document, trackId, clip))
  selectCompositionClip(trackId, clipId)
  return clipId
}


export function addCompositionTrack(kind: TrackKind): string {
  const labels: Record<TrackKind, string> = {
    video: 'Видео overlay',
    audio: 'Аудио',
    image: 'Изображение',
    text: 'Текст',
  }
  const id = makeId(`${kind}-track`)
  const index = kind === 'audio' ? compositionState.document.tracks.length : 0
  commitDocument(addTrack(
    compositionState.document,
    makeTrack(kind, id, nextTrackName(compositionState.document, labels[kind])),
    index,
  ))
  selectCompositionClip(id, null)
  return id
}


export function importSrtToComposition(serialized: string, requestedTrackId?: string): number {
  const cues = parseTimecodedText(serialized)
  if (!cues.length) throw new Error('Файл не содержит субтитров')
  let document = compositionState.document
  const candidateTrack = requestedTrackId
    ? document.tracks.find((track) => track.id === requestedTrackId)
    : selectedCompositionTrack()
  if (requestedTrackId && !candidateTrack) throw new Error('Выбранная text-дорожка не найдена')
  const selectedTrack = candidateTrack?.kind === 'text' ? candidateTrack : undefined
  if (requestedTrackId && !selectedTrack) throw new Error('Для SRT нужна text-дорожка')
  let trackId: string
  if (selectedTrack) {
    if (selectedTrack.locked) throw new Error('Выбранная text-дорожка заблокирована')
    trackId = selectedTrack.id
  } else {
    trackId = makeId('subtitle-track')
    document = addTrack(document, makeTrack('text', trackId, nextTrackName(document, 'Субтитры')), 0)
  }

  const clips = cuesToTextClips(cues, (_cue, index) => makeId(`subtitle-${index + 1}`))
  for (const clip of clips) document = addClip(document, trackId, clip)
  commitDocument(document)
  selectCompositionClip(trackId, clips[0]!.id)
  return clips.length
}


export function exportSelectedTextTrackSrt(): { filename: string; text: string } {
  const selected = selectedCompositionTrack()
  const track = selected?.kind === 'text'
    ? selected
    : compositionState.document.tracks.find((candidate): candidate is TextTrack => candidate.kind === 'text')
  if (!track) throw new Error('В композиции нет text-дорожки')
  if (!track.clips.length) throw new Error('В выбранной text-дорожке нет субтитров')
  const base = (compositionState.projectName.trim() || track.name)
    .replace(/[^\p{L}\p{N}._-]+/gu, '-')
    .replace(/^-+|-+$/g, '')
    .slice(0, 80) || 'subtitles'
  return { filename: `${base}.srt`, text: formatSrt(textClipsToCues(track.clips)) }
}


export function moveCompositionClip(
  clipId: string,
  targetTrackId: string,
  proposedStartTicks: number,
  snap = compositionState.ui.snapEnabled,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  const start = Math.max(0, Math.round(proposedStartTicks))
  const snapped = snap
    ? snapClipStart(
        start,
        clipDurationTicks(location.clip),
        collectSnapTargets(compositionState.document, {
          playheadTicks: compositionState.transport.playheadTicks,
          excludeClipId: clipId,
        }),
        snapThresholdTicks(),
      ).timelineStartTicks
    : start
  commitDocument(moveClip(compositionState.document, clipId, targetTrackId, snapped))
  selectCompositionClip(targetTrackId, clipId)
}


export function trimCompositionClip(
  clipId: string,
  proposedStartTicks: number,
  proposedEndTicks: number,
  snap = compositionState.ui.snapEnabled,
): void {
  let start = Math.max(0, Math.round(proposedStartTicks))
  let end = Math.max(start + 1, Math.round(proposedEndTicks))
  if (snap) {
    const targets = collectSnapTargets(compositionState.document, {
      playheadTicks: compositionState.transport.playheadTicks,
      excludeClipId: clipId,
    })
    start = snapTick(start, targets, snapThresholdTicks()).valueTicks
    end = snapTick(end, targets, snapThresholdTicks()).valueTicks
  }
  commitDocument(trimClip(compositionState.document, clipId, start, end))
}


export function slipCompositionClip(clipId: string, sourceDeltaTicks: number): void {
  commitDocument(slipClip(compositionState.document, clipId, Math.round(sourceDeltaTicks)))
}


export function splitSelectedCompositionClip(): string | null {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return null
  const clip = findClipLocation(compositionState.document, clipId).clip
  const at = compositionState.transport.playheadTicks
  if (at <= clip.timelineStartTicks || at >= clipEndTicks(clip)) return null
  const rightId = makeId(`${clip.kind}-clip`)
  commitDocument(splitClip(compositionState.document, clipId, at, rightId))
  const location = findClipLocation(compositionState.document, rightId)
  selectCompositionClip(location.track.id, rightId)
  return rightId
}


export function deleteSelectedCompositionClip(): void {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return
  commitDocument(deleteClip(compositionState.document, clipId))
  selectCompositionClip(null, null)
}


export function rippleDeleteSelectedCompositionClip(): void {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return
  commitDocument(rippleDeleteClip(compositionState.document, clipId))
  selectCompositionClip(null, null)
}


export function removeSilenceFromSelectedCompositionClip(
  audibleSourceRanges: readonly SourceRange[],
): number {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) throw new Error('Сначала выберите video или audio clip')
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video' && location.clip.kind !== 'audio') {
    throw new Error('Silence removal доступен для video и audio clips')
  }
  const clip = location.clip
  const ranges = audibleSourceRanges
    .map(({ start, end }) => ({
      start: Math.round(start * COMPOSITION_TIME_BASE),
      end: Math.round(end * COMPOSITION_TIME_BASE),
    }))
    .filter(({ start, end }) =>
      Math.min(end, clip.sourceOutTicks) > Math.max(start, clip.sourceInTicks),
    )
  const replacementIds = Array.from(
    { length: Math.max(0, ranges.length - 1) },
    () => makeId(`${clip.kind}-clip`),
  )
  commitDocument(removeSilenceFromClip(compositionState.document, clipId, ranges, replacementIds))
  selectCompositionClip(location.track.id, clipId)
  return ranges.length
}


export function magnetizeCompositionTrack(trackId: string, anchorTicks = 0): void {
  commitDocument(magnetizeTrack(compositionState.document, trackId, anchorTicks))
  compositionState.ui.selectedTrackId = trackId
}


export function duplicateSelectedCompositionClip(): string | null {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return null
  const location = findClipLocation(compositionState.document, clipId)
  const duration = clipDurationTicks(location.clip)
  const start = firstFreeStart(location.track, clipEndTicks(location.clip), duration, clipId)
  const duplicateId = makeId(`${location.clip.kind}-clip`)
  commitDocument(
    duplicateClip(compositionState.document, clipId, {
      id: duplicateId,
      timelineStartTicks: start,
    }),
  )
  selectCompositionClip(location.track.id, duplicateId)
  return duplicateId
}


export function reorderCompositionTrack(trackId: string, toIndex: number): void {
  commitDocument(reorderTrack(compositionState.document, trackId, toIndex))
}


export function toggleCompositionTrackFlag(
  trackId: string,
  flag: 'locked' | 'muted' | 'hidden' | 'solo',
): void {
  const tracks = compositionState.document.tracks.map((track) => {
    if (track.id !== trackId || !(flag in track)) return track
    return { ...track, [flag]: !track[flag as keyof typeof track] } as CompositionTrack
  })
  commitDocument({ ...compositionState.document, tracks })
}


export function updateCompositionClipTarget(clipId: string, targetTrackId: string): void {
  const clip = findClipLocation(compositionState.document, clipId).clip
  moveCompositionClip(clipId, targetTrackId, clip.timelineStartTicks, false)
}


export function updateCompositionText(clipId: string, text: string): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'text') return clip
    return { ...clip, text: text.slice(0, 512) || ' ' }
  })
}


export function updateCompositionClipOpacity(clipId: string, opacity: number): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'image' && clip.kind !== 'text') return clip
    return { ...clip, opacity: clampNumber(opacity, 0, 1) }
  })
}


export function updateCompositionClipGain(clipId: string, gain: number): void {
  updateClip(clipId, (clip) => {
    if (clip.kind === 'audio') return { ...clip, gain: clampNumber(gain, 0, 16) }
    if (clip.kind === 'video') return { ...clip, audioGain: clampNumber(gain, 0, 16) }
    return clip
  })
}


export function updateCompositionVideoAudio(
  clipId: string,
  patch: Partial<{ sourceAudioEnabled: boolean; audioPan: number }>,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video') return clip
    if (clip.playbackMode?.mode === 'freeze' && patch.sourceAudioEnabled === true) {
      throw new Error('Freeze-frame clip не может использовать встроенный звук')
    }
    return {
      ...clip,
      sourceAudioEnabled: patch.sourceAudioEnabled ?? clip.sourceAudioEnabled,
      audioPan: patch.audioPan === undefined ? clip.audioPan ?? 0 : clampNumber(patch.audioPan, -1, 1),
    }
  })
}


export function detachCompositionVideoAudio(clipId: string): string {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video') throw new Error('Отделить звук можно только от video clip')
  if (location.track.locked) throw new Error('Video-дорожка заблокирована')
  const clip = location.clip
  const source = compositionState.document.sources[clip.sourceId]
  if (!source?.hasAudio) throw new Error('Исходник не содержит аудио')
  if (!clip.sourceAudioEnabled) throw new Error('Встроенный звук уже выключен')
  if (clip.playbackMode?.mode === 'freeze') throw new Error('Freeze frame не содержит непрерывного аудио')
  if (clip.speedRamp?.audioPolicy === 'mute') {
    throw new Error('Speed ramp настроен на mute audio')
  }

  let document: Composition = {
    ...compositionState.document,
    tracks: compositionState.document.tracks.map((track) => track.id !== location.track.id ? track : {
      ...track,
      clips: track.kind === 'video'
        ? track.clips.map((candidate) => candidate.id === clipId ? { ...candidate, sourceAudioEnabled: false } : candidate)
        : track.clips,
    } as CompositionTrack),
  }
  const end = clipEndTicks(clip)
  let audioTrack = findAvailableTrack(document, 'audio', clip.timelineStartTicks, end) as AudioTrack | undefined
  if (!audioTrack) {
    const trackId = makeId('detached-audio-track')
    document = addTrack(document, makeTrack('audio', trackId, nextTrackName(document, 'Отделённый звук')))
    audioTrack = document.tracks.find((track): track is AudioTrack => track.id === trackId)!
  }
  const audioId = makeId('detached-audio')
  const audioClip: AudioClip = {
    id: audioId,
    kind: 'audio',
    sourceId: clip.sourceId,
    timelineStartTicks: clip.timelineStartTicks,
    sourceInTicks: clip.sourceInTicks,
    sourceOutTicks: clip.sourceOutTicks,
    speed: clip.speed,
    ...(clip.speedRamp ? { speedRamp: structuredClone(clip.speedRamp) } : {}),
    gain: clip.audioGain,
    pan: clip.audioPan ?? 0,
    reversed: clip.playbackMode?.mode === 'reverse',
    ...(clip.audioAnimation ? { audioAnimation: structuredClone(clip.audioAnimation) } : {}),
    fadeInTicks: 0,
    fadeOutTicks: 0,
  }
  document = addClip(document, audioTrack.id, audioClip)
  commitDocument(document)
  selectCompositionClip(audioTrack.id, audioId)
  return audioId
}


export function updateCompositionPlaybackMode(
  clipId: string,
  playbackMode: CompositionPlaybackMode,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video') throw new Error('Playback mode доступен только для video clips')
  if (playbackMode.mode === 'freeze') {
    if (
      !Number.isSafeInteger(playbackMode.sourceTick) ||
      playbackMode.sourceTick < location.clip.sourceInTicks ||
      playbackMode.sourceTick >= location.clip.sourceOutTicks
    ) {
      throw new Error('Freeze source tick должен находиться внутри source range клипа')
    }
    if ((location.clip.frameInterpolation ?? 'duplicate') === 'optical_flow') {
      throw new Error('Freeze frame нельзя совмещать с optical flow')
    }
    if (location.clip.stabilization?.mode === 'deshake') {
      throw new Error('Freeze frame нельзя совмещать с deshake stabilization')
    }
    if (location.clip.speedRamp) throw new Error('Freeze frame нельзя совмещать со speed ramp')
  }
  updateClip(clipId, (clip) => clip.kind === 'video'
    ? {
        ...clip,
        playbackMode: { ...playbackMode },
        ...(playbackMode.mode === 'freeze' ? { sourceAudioEnabled: false } : {}),
      }
    : clip)
}


export function freezeCompositionClipAtPlayhead(clipId: string): number {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video') throw new Error('Freeze frame доступен только для video clips')
  const playhead = compositionState.transport.playheadTicks
  if (playhead < location.clip.timelineStartTicks || playhead >= clipEndTicks(location.clip)) {
    throw new Error('Плейхед должен находиться внутри выбранного клипа')
  }
  const sourceTick = videoSourceTickAtTimelineTick(location.clip, playhead)
  updateCompositionPlaybackMode(clipId, { mode: 'freeze', sourceTick })
  return sourceTick
}


export function updateCompositionFrameInterpolation(
  clipId: string,
  frameInterpolation: CompositionFrameInterpolation,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video' || location.track.kind !== 'video') {
    throw new Error('Frame interpolation доступна только для video clips')
  }
  if (frameInterpolation === 'optical_flow') {
    if (location.track.hidden) throw new Error('Optical flow доступен только на видимой video-дорожке')
    if (minimumCompositionSpeed(location.clip.speed ?? 1, location.clip.speedRamp) >= 1) {
      throw new Error('Optical flow требует хотя бы один speed ramp участок меньше 1x')
    }
    if (location.clip.playbackMode?.mode === 'freeze') throw new Error('Freeze frame нельзя совмещать с optical flow')
  }
  updateClip(clipId, (clip) => clip.kind === 'video' ? { ...clip, frameInterpolation } : clip)
}


export function updateCompositionStabilization(
  clipId: string,
  stabilization: CompositionStabilization,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video' || location.track.kind !== 'video') {
    throw new Error('Stabilization доступна только для video clips')
  }
  if (stabilization.mode === 'deshake') {
    if (location.track.hidden) throw new Error('Deshake доступен только на видимой video-дорожке')
    if (location.clip.playbackMode?.mode === 'freeze') {
      throw new Error('Deshake stabilization нельзя совмещать с freeze frame')
    }
  }
  updateClip(clipId, (clip) => clip.kind === 'video'
    ? { ...clip, stabilization: { ...stabilization } }
    : clip)
}


export function updateCompositionClipSpeed(clipId: string, speed: number): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'audio') return clip
    const oldDuration = clipDurationTicks(clip)
    const normalized = clampNumber(speed, 0.05, 16)
    const speedRamp = clip.speedRamp
      ? {
          ...cloneCompositionSpeedRamp(clip.speedRamp)!,
          points: clip.speedRamp.points.map((point, index) => index === 0 ? { ...point, speed: normalized } : { ...point }),
        }
      : undefined
    const updated = {
      ...clip,
      speed: normalized,
      ...(speedRamp ? { speedRamp } : {}),
    }
    const duration = clipDurationTicks(updated)
    const retimed = {
      ...updated,
      ...(updated.audioAnimation ? {
        audioAnimation: sliceAudioAnimation(updated.audioAnimation, 0, duration, oldDuration),
      } : {}),
      ...(clip.kind === 'audio' ? {
        fadeInTicks: Math.min(clip.fadeInTicks ?? 0, duration),
        fadeOutTicks: Math.min(clip.fadeOutTicks ?? 0, duration),
      } : {}),
    }
    if (retimed.kind !== 'video') return retimed
    return {
      ...retimed,
      ...(retimed.animation ? { animation: sliceVisualAnimation(retimed.animation, 0, duration, oldDuration) } : {}),
      ...(retimed.masks ? { masks: sliceVideoMasks(retimed.masks, 0, duration, oldDuration) } : {}),
    }
  })
}


export function updateCompositionSpeedRamp(
  clipId: string,
  speedRamp: CompositionSpeedRamp | undefined,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video' && location.clip.kind !== 'audio') {
    throw new Error('Speed ramp доступна только для video/audio clips')
  }
  if (speedRamp && location.clip.kind === 'video') {
    if (location.clip.playbackMode?.mode === 'freeze') {
      throw new Error('Freeze frame нельзя совмещать со speed ramp')
    }
    if (
      location.track.kind === 'video' &&
      (location.track.transitions ?? []).some(
        (transition) => transition.fromClipId === clipId || transition.toClipId === clipId,
      )
    ) {
      throw new Error('Удалите transition, прежде чем включать speed ramp на endpoint clip')
    }
  }
  const oldDuration = clipDurationTicks(location.clip)
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'audio') return clip
    let updated: VideoClip | AudioClip
    if (speedRamp) {
      updated = {
        ...clip,
        speedRamp: {
          interpolation: speedRamp.interpolation,
          points: speedRamp.points.map((point) => ({ ...point })),
          audioPolicy: speedRamp.audioPolicy ?? 'preserve_pitch',
        },
      }
    } else {
      const { speedRamp: removed, ...withoutSpeedRamp } = clip
      void removed
      updated = withoutSpeedRamp
    }
    const duration = clipDurationTicks(updated)
    if (updated.kind === 'audio') {
      return {
        ...updated,
        ...(updated.audioAnimation ? {
          audioAnimation: sliceAudioAnimation(updated.audioAnimation, 0, duration, oldDuration),
        } : {}),
        fadeInTicks: Math.min(updated.fadeInTicks ?? 0, duration),
        fadeOutTicks: Math.min(updated.fadeOutTicks ?? 0, duration),
      }
    }
    return {
      ...updated,
      ...(updated.audioAnimation ? {
        audioAnimation: sliceAudioAnimation(updated.audioAnimation, 0, duration, oldDuration),
      } : {}),
      ...(updated.animation ? { animation: sliceVisualAnimation(updated.animation, 0, duration, oldDuration) } : {}),
      ...(updated.masks ? { masks: sliceVideoMasks(updated.masks, 0, duration, oldDuration) } : {}),
    }
  })
}


export function updateCompositionVisualTransform(
  clipId: string,
  patch: Partial<{ x: number; y: number; width: number; height: number }>,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind === 'text') {
      return {
        ...clip,
        x: patch.x === undefined ? clip.x : clampNumber(patch.x, -32_768, 32_768),
        y: patch.y === undefined ? clip.y : clampNumber(patch.y, -32_768, 32_768),
      }
    }
    if (clip.kind !== 'video' && clip.kind !== 'image') return clip
    return {
      ...clip,
      transform: {
        ...clip.transform,
        x: patch.x === undefined ? clip.transform.x : clampNumber(patch.x, -32_768, 32_768),
        y: patch.y === undefined ? clip.transform.y : clampNumber(patch.y, -32_768, 32_768),
        width: patch.width === undefined ? clip.transform.width : clampNumber(patch.width, 1, 131_072),
        height: patch.height === undefined ? clip.transform.height : clampNumber(patch.height, 1, 131_072),
      },
    }
  })
}


export function updateCompositionRotation(clipId: string, rotationDegrees: number): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'image' && clip.kind !== 'text') return clip
    return { ...clip, rotationDegrees: clampNumber(rotationDegrees, -3_600, 3_600) }
  })
}


export function updateCompositionBlendMode(clipId: string, blendMode: CompositionBlendMode): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'image') return clip
    return { ...clip, blendMode }
  })
}


export function updateCompositionChromaKey(clipId: string, chromaKey: CompositionChromaKey): void {
  updateClip(clipId, (clip) => clip.kind === 'video' ? { ...clip, chromaKey: { ...chromaKey } } : clip)
}


export function updateCompositionVideoEffects(clipId: string, videoEffects: readonly CompositionVideoStyleEffect[]): void {
  updateClip(clipId, (clip) => clip.kind === 'video'
    ? { ...clip, videoEffects: videoEffects.map((effect) => ({ ...effect })) }
    : clip)
}


export type { CompositionAutomationTarget, CompositionAutomationProperty } from './core.svelte.js'

export function setCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  value?: number,
  timelineTicks = compositionState.transport.playheadTicks,
): void {
  const resolved = resolveAutomation(target, property)
  const [clip, current, fallback, minimum, maximum, write] = resolved
  const duration = clipDurationTicks(clip)
  const localTicks = Math.round(timelineTicks - clip.timelineStartTicks)
  if (localTicks < 0 || localTicks > duration) throw new Error('Плейхед должен находиться внутри выбранного clip')
  const sampled = value ?? sampleAnimatableValue(current ?? constantAnimatable(fallback), localTicks)
  const timeBase = current?.mode === 'keyframes' ? current.track.timeBase : COMPOSITION_TIME_BASE
  write(upsertKeyframe(
    current,
    keyframeTickAtLocalTime(localTicks, timeBase),
    clampNumber(sampled, minimum, maximum),
    fallback,
  ))
}


export function updateCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  originalTick: number,
  tick: number,
  value: number,
): void {
  const resolved = resolveAutomation(target, property)
  const [clip, current, , minimum, maximum, write] = resolved
  if (!current || current.mode !== 'keyframes') throw new Error('У параметра нет keyframe track')
  const maximumTick = Math.floor((clipDurationTicks(clip) * current.track.timeBase) / COMPOSITION_TIME_BASE)
  write(updateKeyframe(
    current,
    originalTick,
    clampTick(tick, maximumTick),
    clampNumber(value, minimum, maximum),
  ))
}


export function deleteCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  tick: number,
): void {
  const resolved = resolveAutomation(target, property)
  const current = resolved[1]
  if (!current) return
  const next = deleteKeyframe(current, tick)
  resolved[5](next ?? (target.kind === 'mask' ? constantAnimatable(sampleAtPlayhead(resolved)) : undefined))
}


export function setCompositionAutomationInterpolation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  interpolation: Exclude<CompositionInterpolation, 'ease_in_out_cubic'>,
): void {
  const resolved = resolveAutomation(target, property)
  const current = resolved[1]
  if (!current) throw new Error('Сначала добавьте ключевой кадр')
  resolved[5](updateInterpolation(current, interpolation))
}


export function clearCompositionAutomation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
): void {
  const resolved = resolveAutomation(target, property)
  resolved[5](target.kind === 'mask' && resolved[1]
    ? constantAnimatable(sampleAtPlayhead(resolved))
    : undefined)
}


export function setCompositionVisualKeyframe(clipId: string, property: CompositionVisualProperty, value?: number, timelineTicks?: number): void {
  setCompositionAutomationKeyframe({ kind: 'visual', clipId }, property, value, timelineTicks)
}


export function updateCompositionVisualKeyframe(clipId: string, property: CompositionVisualProperty, originalTick: number, tick: number, value: number): void {
  updateCompositionAutomationKeyframe({ kind: 'visual', clipId }, property, originalTick, tick, value)
}


export function deleteCompositionVisualKeyframe(clipId: string, property: CompositionVisualProperty, tick: number): void {
  deleteCompositionAutomationKeyframe({ kind: 'visual', clipId }, property, tick)
}


export function setCompositionVisualInterpolation(clipId: string, property: CompositionVisualProperty, interpolation: Exclude<CompositionInterpolation, 'ease_in_out_cubic'>): void {
  setCompositionAutomationInterpolation({ kind: 'visual', clipId }, property, interpolation)
}


export function clearCompositionVisualAnimation(clipId: string, property: CompositionVisualProperty): void {
  clearCompositionAutomation({ kind: 'visual', clipId }, property)
}


export function applyCompositionVisualAnimation(
  clipId: string,
  animation: CompositionVisualAnimation,
): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (!location || location.clip.kind === 'audio') throw new Error('Animation требует visual clip')
  updateClip(clipId, (candidate) => candidate.kind === 'audio' ? candidate : {
    ...candidate,
    animation: { ...(candidate.animation ?? {}), ...animation },
  })
}


export function clearCompositionVisualAnimations(clipId: string): void {
  updateClip(clipId, (clip) => clip.kind === 'audio' ? clip : { ...clip, animation: undefined })
}

/** Apply a completed point track as one undoable X/Y animation edit. */

export function applyCompositionTrackedAnimation(
  clipId: string,
  animation: CompositionVisualAnimation,
): void {
  requireVisualAutomation(clipId)
  const x = animation.x
  const y = animation.y
  if (!x || !y || x.mode !== 'keyframes' || y.mode !== 'keyframes') {
    throw new Error('Tracking должен вернуть парные X/Y keyframe tracks')
  }
  const xTrack = x.track
  const yTrack = y.track
  if (
    xTrack.timeBase !== yTrack.timeBase ||
    xTrack.keyframes.length !== yTrack.keyframes.length ||
    xTrack.keyframes.some((keyframe, index) => keyframe.tick !== yTrack.keyframes[index]?.tick)
  ) {
    throw new Error('Tracking X/Y keyframes должны иметь одинаковые ticks')
  }
  updateClip(clipId, (clip) => {
    if (clip.kind === 'audio') return clip
    return {
      ...clip,
      animation: {
        ...(clip.animation ?? {}),
        x: cloneAnimatableValue(x),
        y: cloneAnimatableValue(y),
      },
    }
  })
}


export function addCompositionVideoMask(
  clipId: string,
  shape: CompositionMaskShape,
): string {
  const location = requireVisualOverlay(clipId)
  if (location.clip.kind !== 'video') throw new Error('Masks доступны только для video overlay')
  const id = makeId('mask')
  const mask: CompositionVideoMask = {
    id,
    shape,
    rotationDegrees: constantAnimatable(0),
    x: constantAnimatable(0.5),
    y: constantAnimatable(0.5),
    width: constantAnimatable(0.8),
    height: constantAnimatable(0.8),
    feather: 0,
    inverted: false,
  }
  updateClip(clipId, (clip) => clip.kind === 'video' ? { ...clip, masks: [...(clip.masks ?? []), mask] } : clip)
  return id
}


export function updateCompositionVideoMask(
  clipId: string,
  maskId: string,
  patch: Partial<{
    shape: CompositionMaskShape
    rotationDegrees: number
    x: number
    y: number
    width: number
    height: number
    feather: number
    inverted: boolean
  }>,
): void {
  const { clip: selectedClip } = requireVisualOverlay(clipId)
  const localTicks = clampTick(
    compositionState.transport.playheadTicks - selectedClip.timelineStartTicks,
    clipDurationTicks(selectedClip),
  )
  const patchValue = (
    current: CompositionVideoMask['x'],
    value: number | undefined,
    minimum: number,
    maximum: number,
  ): CompositionVideoMask['x'] => {
    if (value === undefined) return current
    const next = clampNumber(value, minimum, maximum)
    if (current.mode !== 'keyframes') return constantAnimatable(next)
    return upsertKeyframe(
      current,
      keyframeTickAtLocalTime(localTicks, current.track.timeBase),
      next,
      sampleAnimatableValue(current, 0),
    )
  }
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video') return clip
    if (!(clip.masks ?? []).some((mask) => mask.id === maskId)) throw new Error('Mask не найдена')
    return {
      ...clip,
      masks: (clip.masks ?? []).map((mask) => mask.id !== maskId ? mask : {
        ...mask,
        shape: patch.shape ?? mask.shape,
        rotationDegrees: patchValue(mask.rotationDegrees ?? constantAnimatable(0), patch.rotationDegrees, -180, 180),
        x: patchValue(mask.x, patch.x, 0, 1),
        y: patchValue(mask.y, patch.y, 0, 1),
        width: patchValue(mask.width, patch.width, 0.000_001, 2),
        height: patchValue(mask.height, patch.height, 0.000_001, 2),
        feather: patch.feather === undefined ? mask.feather : clampNumber(patch.feather, 0, 1),
        inverted: patch.inverted ?? mask.inverted,
      }),
    }
  })
}


export function deleteCompositionVideoMask(clipId: string, maskId: string): void {
  requireVisualOverlay(clipId)
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video') return clip
    if (!(clip.masks ?? []).some((mask) => mask.id === maskId)) return clip
    return { ...clip, masks: (clip.masks ?? []).filter((mask) => mask.id !== maskId) }
  })
}


export function updateCompositionAudioMix(
  clipId: string,
  patch: Partial<{ pan: number; reversed: boolean; fadeInTicks: number; fadeOutTicks: number; voiceEffect: import('$lib/composition/types.js').CompositionVoiceEffect; pitchSemitones: number; toneDb: number; crossfadeInTicks: number; ducking: import('$lib/composition/types.js').CompositionAudioDucking | undefined }>,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'audio') return clip
    const duration = clipDurationTicks(clip)
    return {
      ...clip,
      pan: patch.pan === undefined ? clip.pan ?? 0 : clampNumber(patch.pan, -1, 1),
      reversed: patch.reversed ?? clip.reversed ?? false,
      fadeInTicks: patch.fadeInTicks === undefined ? clip.fadeInTicks ?? 0 : clampTick(patch.fadeInTicks, duration),
      fadeOutTicks: patch.fadeOutTicks === undefined ? clip.fadeOutTicks ?? 0 : clampTick(patch.fadeOutTicks, duration),
      voiceEffect: patch.voiceEffect ?? clip.voiceEffect ?? 'none',
      pitchSemitones: patch.pitchSemitones === undefined ? clip.pitchSemitones ?? 0 : clampNumber(patch.pitchSemitones, -12, 12),
      toneDb: patch.toneDb === undefined ? clip.toneDb ?? 0 : clampNumber(patch.toneDb, -12, 12),
      crossfadeInTicks: patch.crossfadeInTicks === undefined ? clip.crossfadeInTicks ?? 0 : clampTick(patch.crossfadeInTicks, duration),
      ducking: Object.hasOwn(patch, 'ducking') ? patch.ducking : clip.ducking,
    }
  })
}


export function updateCompositionTextStyle(clipId: string, patch: Partial<TextClip['style']>): void {
  updateClip(clipId, (clip) => clip.kind === 'text' ? { ...clip, style: { ...clip.style, ...patch } } : clip)
}


export function applyCompositionTextStyleToTrack(clipId: string): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'text' || location.track.kind !== 'text') {
    throw new Error('Для применения стиля нужна text-дорожка')
  }
  if (location.track.locked) throw new Error('Text-дорожка заблокирована')
  const style = { ...location.clip.style }
  commitDocument({
    ...compositionState.document,
    tracks: compositionState.document.tracks.map((track) => track.id === location.track.id && track.kind === 'text'
      ? { ...track, clips: track.clips.map((clip) => ({ ...clip, style: { ...style } })) }
      : track),
  })
}


export function setCompositionTransition(
  trackId: string,
  fromClipId: string,
  toClipId: string,
  kind: CompositionTransitionKind,
  durationTicks: number,
  transitionId?: string,
): string {
  const transition: CompositionTransition = {
    id: transitionId ?? makeId('transition'),
    fromClipId,
    toClipId,
    kind,
    durationTicks: Math.max(1, Math.round(durationTicks)),
  }
  commitDocument(upsertTransition(compositionState.document, trackId, transition))
  return transition.id
}


export function removeCompositionTransition(trackId: string, transitionId: string): void {
  commitDocument(deleteTransition(compositionState.document, trackId, transitionId))
}

