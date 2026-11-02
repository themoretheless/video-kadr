import { restyleCaptions, type CaptionStylePatch } from '../../composition/restyleCaptions.js'
import { mergeCaptions } from '../../composition/mergeCaptions.js'
import { captionExportCues, formatSubtitleExport, type SubtitleExportFormat } from '../../subtitles/export.js'
import {embeddedAudioMix} from '$lib/composition/embeddedAudioMix'
import {cloneGainSourceKeys} from '$lib/composition/concatGainSourceKeys'
import { replaceClipMedia, setClipMediaVersion } from '../../composition/replaceMedia.js'
import { insertFreezeFrame } from '../../composition/freezeInsert.js'
import { mergeClips } from '../../composition/mergeClips.js'
import { cloneAudioAnimation } from '../../composition/keyframes.js'
import { reattachAudio } from '../../composition/reattachAudio.js'
import { createCompositionLink } from '../../composition/links.js'
import { splitAllAtTick } from '../../composition/splitAll.js'
import {boundaryTransition,type BoundaryTransitionOptions} from '../../composition/boundaryTransition.js'
import { pasteInsertComposition } from '../../composition/clipboardInsert.js'
import { setCompositionPlayhead } from './transport.svelte.js'
import { insertCompositionClips, type InsertPlacement } from '../../composition/globalInsert.js'
import { createGraphicClip, createGraphicTemplateClip, cloneGraphicValue } from '../../composition/graphicLayers.js'
import type { GraphicContent, GraphicLayer } from '../../composition/types.js'
import { cloneDspEffect } from '$lib/composition/dsp'
import { createSolidColorClip,editSolidColorParams,type SolidColorParams } from '../../composition/solidColor.js'
import { SvelteSet } from 'svelte/reactivity'
import { copyColorKeys,pasteColorKeys } from '../../composition/colorClipboard.js'
import { updateKeyframeTangents, editKeyframeTangent, setKeyframeTangentMode } from '../../composition/tangents.js'
import { audioRoleMix } from '../../composition/audioRoles.js'
import { COMPOSITION_VISUAL_PROPERTIES, COMPOSITION_AUDIO_PROPERTIES, COMPOSITION_MASK_PROPERTIES } from '../../composition/types.js'
import { snapshotSelectedKeyframes, planKeyframePaste } from '../../composition/keyframeClipboard.js'
import { updateSelectedKeyframeInterpolation } from '../../composition/keyframes.js'
import { commitAutomationValues, linkedScaleAutomationPatches } from './core.svelte.js'
import { canvasTimeGrid } from '../../composition/timeGrid.js'
import { moveSelectedKeyframeTracks, scaleSelectedKeyframeTracks } from '../../composition/keyframeSelection.js'
import { setClipLocked } from '../../composition/commands.js'
import { moveClipsToNewTrack } from '../../composition/commands.js'
import { setClipsEnabled } from '../../composition/commands.js'
import { parseSubtitleDocument } from '../../subtitles/import.js'
import { linkedClipIds } from '../../composition/links.js'
import {
  findClipLocation,
  addTrack,
  addClip,
  moveClip,
  moveClips,
  moveClipsByTrackOffset,
  trimClip,
  slipClip,
  splitClip,
  deleteClip,
  pasteClips,
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
  visualPropertyFallback,
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
  compositionGapAt,
  rippleDeleteGap,
  magnetizeTrack,
  rippleDeleteClips
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
  compositionSelectionEscapesLink,
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


export function addGraphicToComposition(kind: GraphicContent['kind'] = 'text'): string {
  let document = compositionState.document
  const start = compositionState.transport.playheadTicks
  const duration = Math.min(DEFAULT_TEXT_DURATION_TICKS, MAX_COMPOSITION_DURATION_TICKS - start)
  if (duration <= 0) throw new Error('Плейхед находится за пределом композиции')
  let trackId = findAvailableTrack(document, 'graphic', start, start + duration)?.id
  if (!trackId) {
    trackId = makeId('graphic-track')
    document = addTrack(document, makeTrack('graphic', trackId, nextTrackName(document, 'Графика')), 0)
  }
  const clipId = makeId('graphic')
  commitDocument(addClip(document, trackId, createGraphicClip(document.canvas, clipId, start, duration, kind)))
  selectCompositionClip(trackId, clipId)
  return clipId
}

export function updateCompositionGraphicLayers(clipId: string, layers: readonly GraphicLayer[]): void {
  updateClip(clipId, clip => {
    if (clip.kind !== 'graphic') throw new Error('Для изменения слоёв нужен графический клип')
    return { ...clip, graphic: { layers: cloneGraphicValue(layers) } }
  })
}

export function addColorToComposition(): string {
  let document = compositionState.document
  const start = compositionState.transport.playheadTicks
  const duration = Math.min(DEFAULT_TEXT_DURATION_TICKS, MAX_COMPOSITION_DURATION_TICKS - start)
  if (duration <= 0) throw new Error('Плейхед находится за пределом композиции')
  let trackId = findAvailableTrack(document, 'color', start, start + duration)?.id
  if (!trackId) {
    trackId = makeId('color-track')
    document = addTrack(document, makeTrack('color', trackId, nextTrackName(document, 'Цвет')), 0)
  }
  const clipId = makeId('color')
  commitDocument(addClip(document, trackId, createSolidColorClip(document.canvas, clipId, start, duration)))
  selectCompositionClip(trackId, clipId)
  return clipId
}

export function updateCompositionColorParams(clipId: string, patch: Partial<SolidColorParams>): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'color') throw new Error('Для изменения цвета нужен цветовой слой')
    return { ...clip, ...editSolidColorParams(clip, patch) }
  })
}

/** Commit the complete media/caption insertion as one undoable edit. */
export function insertCompositionPlacements(placements: readonly InsertPlacement[]): void {
  const next = insertCompositionClips(compositionState.document, placements, () => makeId('insert'))
  commitDocument(next)
  const first = placements[0]!
  selectCompositionClip(first.trackId, first.clip.id)
}

export function addCompositionTrack(kind: TrackKind): string {
  const labels: Record<TrackKind, string> = {
    video: 'Видео overlay',
    color: 'Цвет',
    graphic: 'Графика',
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
  const cues = parseSubtitleDocument(serialized)
  if (!cues.length) throw new Error('Файл не содержит субтитров')
  let document = compositionState.document
  const candidateTrack = requestedTrackId
    ? document.tracks.find((track) => track.id === requestedTrackId)
    : selectedCompositionTrack()
  if (requestedTrackId && !candidateTrack) throw new Error('Выбранная text-дорожка не найдена')
  const selectedTrack = candidateTrack?.kind === 'text' ? candidateTrack : undefined
  if (requestedTrackId && !selectedTrack) throw new Error('Для субтитров нужна text-дорожка')
  if (requestedTrackId && selectedTrack?.locked) throw new Error('Выбранная text-дорожка заблокирована')
  const roles = new SvelteSet(document.metadata?.captionTextTracks ?? [])
  if (requestedTrackId) roles.add(requestedTrackId)
  const lanes = document.tracks.filter((track) => track.kind === 'text' && roles.has(track.id))
  const candidates = lanes.filter((track) => !track.locked).map((track) => track.id)
  if (requestedTrackId) {
    candidates.splice(candidates.indexOf(requestedTrackId), 1)
    candidates.unshift(requestedTrackId)
  }
  const clips = cuesToTextClips(
    [...cues].sort((a, b) => a.startTicks - b.startTicks),
    (_cue, index) => makeId(`subtitle-${index + 1}`),
  )
  let firstTrackId = ''
  for (const clip of clips) {
    let trackId = candidates.find((id) => {
      const track = document.tracks.find((track) => track.id === id)!
      return track.clips.every((existing) =>
        clip.timelineStartTicks >= clipEndTicks(existing) ||
        existing.timelineStartTicks >= clipEndTicks(clip))
    })
    if (!trackId) {
      trackId = makeId('subtitle-track')
      const lastLane = document.tracks.reduce((last, track, index) => roles.has(track.id) ? index : last, -1)
      document = addTrack(document, makeTrack('text', trackId, nextTrackName(document, 'Субтитры')), lastLane + 1)
      roles.add(trackId)
      candidates.push(trackId)
    }
    document = addClip(document, trackId, clip)
    if (!firstTrackId) firstTrackId = trackId
  }
  document = { ...document, metadata: { ...document.metadata,
    captionTextTracks: document.tracks.filter((track) => track.kind === 'text' && roles.has(track.id)).map((track) => track.id),
  } }
  commitDocument(document)
  selectCompositionClip(firstTrackId, clips[0]!.id)
  return clips.length
}


/** Merge the current caption selection as one recorded edit. */
export function restyleCompositionCaptions(patch: CaptionStylePatch, ids?: readonly string[] | null): void {
  commitDocument(restyleCaptions(compositionState.document, patch, ids).document)
}

export function mergeSelectedCompositionCaptions(): void {
  const result = mergeCaptions(compositionState.document, compositionState.ui.selectedClipIds)
  commitDocument(result.document)
  selectCompositionClip(result.trackId, result.record.layer_id)
}

/** Read-only caption-role export across every lane, or one explicitly requested lane. */
export function exportCompositionCaptions(format: SubtitleExportFormat, trackId?: string): { filename: string; text: string } {
  const cues = captionExportCues(compositionState.document, trackId)
  if (!cues.length) throw new Error('В композиции нет субтитров для экспорта')
  const base = (compositionState.projectName.trim() || 'subtitles')
    .replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 80) || 'subtitles'
  return { filename: `${base}.${format}`, text: formatSubtitleExport(cues, format) }
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
  const escapeLink = compositionSelectionEscapesLink(clipId)
  const selection = compositionState.ui.selectedClipIds.includes(clipId) ? [...compositionState.ui.selectedClipIds] : null
  commitDocument(planCompositionClipMove(clipId, targetTrackId, proposedStartTicks, snap))
  selectCompositionClip(targetTrackId, clipId, { single: escapeLink })
  if (selection) compositionState.ui.selectedClipIds = selection
}

/** Read-only rehearsal of exactly the move the visible editor will commit. */
export function planCompositionClipMove(
  clipId: string,
  targetTrackId: string,
  proposedStartTicks: number,
  snap = compositionState.ui.snapEnabled,
): Composition {
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
  const escapeLink = compositionSelectionEscapesLink(clipId)
  const selection = compositionState.ui.selectedClipIds.includes(clipId) ? [...compositionState.ui.selectedClipIds] : null
  if (targetTrackId !== location.track.id) {
    const subjects = selection ?? (escapeLink ? [clipId] : linkedClipIds(compositionState.document, clipId))
    return moveClipsByTrackOffset(compositionState.document, subjects, clipId, targetTrackId, snapped, compositionState.document.tracks.map((track) => track.id))
  }
  return selection && selection.length > 1
    ? moveClips(compositionState.document, [clipId, ...selection.filter((id) => id !== clipId)], targetTrackId, snapped)
    : moveClip(compositionState.document, clipId, targetTrackId, snapped, escapeLink)
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
  commitDocument(trimClip(compositionState.document, clipId, start, end, compositionSelectionEscapesLink(clipId)))
}


export function slipCompositionClip(clipId: string, sourceDeltaTicks: number): void {
  commitDocument(slipClip(compositionState.document, clipId, Math.round(sourceDeltaTicks)))
}


export function setCompositionClipMediaVersion(clipId: string, copy: boolean): void {
  commitDocument(setClipMediaVersion(compositionState.document, clipId, copy))
}

export function replaceCompositionClipMedia(clipId: string, sourceId: string, sourceStart?: number, processed = false): void {
  commitDocument(replaceClipMedia(compositionState.document, clipId, sourceId, sourceStart, processed))
}

export function insertCompositionFreezeFrame(durationTicks = COMPOSITION_TIME_BASE): string {
  const id = compositionState.ui.selectedClipId
  if (!id) throw new Error('Выберите видео или изображение')
  const result = insertFreezeFrame(compositionState.document, id, compositionState.transport.playheadTicks, durationTicks, () => makeId('freeze'))
  commitDocument(result.document)
  selectCompositionClip(result.trackId, result.clipId)
  return result.clipId
}

export function mergeSelectedCompositionClips(): void {
  const result = mergeClips(compositionState.document, compositionState.ui.selectedClipIds)
  commitDocument(result.document)
  const id = result.survivors.find(id => findClipLocation(result.document, id).track.id === compositionState.ui.selectedTrackId) ?? result.survivors[0]!
  const location = findClipLocation(result.document, id)
  selectCompositionClip(location.track.id, id)
}

export function splitAllCompositionClips(): void {
  const document = compositionState.document
  const result = splitAllAtTick(document, compositionState.transport.playheadTicks, () => makeId('split-clip'))
  if (result !== document) commitDocument(result)
}

export function splitSelectedCompositionClip(): string | null {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return null
  const clip = findClipLocation(compositionState.document, clipId).clip
  const at = compositionState.transport.playheadTicks
  if (at <= clip.timelineStartTicks || at >= clipEndTicks(clip)) return null
  const rightId = makeId(`${clip.kind}-clip`)
  commitDocument(splitClip(compositionState.document, clipId, at, rightId, compositionSelectionEscapesLink(clipId)))
  const location = findClipLocation(compositionState.document, rightId)
  selectCompositionClip(location.track.id, rightId)
  return rightId
}


export function deleteSelectedCompositionClip(): void {
  if (compositionState.ui.selectedKeyframes.length) {
    deleteCompositionKeyframeSelection(compositionState.ui.selectedKeyframes)
    compositionState.ui.selectedKeyframes = []
    return
  }
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return
  const ids = compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : [clipId]
  let document = compositionState.document
  for (const id of ids) document = deleteClip(document, id)
  commitDocument(document)
  selectCompositionClip(null, null)
}


export function rippleDeleteSelectedCompositionClip(): void {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return
  commitDocument(rippleDeleteClips(compositionState.document, compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : [clipId]))
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


export function duplicateSelectedCompositionClip(destination?: { trackId: string; timelineStartTicks: number }): string | null {
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return null
  const location = findClipLocation(compositionState.document, clipId)
  const duration = clipDurationTicks(location.clip)
  const selected = compositionState.ui.selectedClipIds.includes(clipId) ? compositionState.ui.selectedClipIds : compositionSelectionEscapesLink(clipId) ? [clipId] : linkedClipIds(compositionState.document, clipId)
  const subjectIds = [clipId, ...selected.filter((id) => id !== clipId)]
  const members = subjectIds.map((id) => findClipLocation(compositionState.document, id).clip)
  const proposed = members.length > 1
    ? location.clip.timelineStartTicks + Math.max(...members.map(clipEndTicks)) - Math.min(...members.map((clip) => clip.timelineStartTicks))
    : clipEndTicks(location.clip)
  const start = destination?.timelineStartTicks ?? firstFreeStart(location.track, proposed, duration, clipId)
  const duplicateId = makeId(`${location.clip.kind}-clip`)
  const copies = subjectIds.map((id) => ({ clipId: id, id: id === clipId ? duplicateId : makeId('copy') }))
  commitDocument(
    pasteClips(compositionState.document, copies, { timelineStartTicks: start, ...(destination ? { targetTrackId: destination.trackId } : {}) }),
  )
  selectCompositionClip(destination?.trackId ?? location.track.id, duplicateId, { single: true })
  compositionState.ui.selectedClipIds = copies.map((copy) => copy.id)
  return duplicateId
}


export function raiseSelectedCompositionClips(anchor?: { clipId: string; startTicks: number }): string | null {
  const ids = compositionState.ui.selectedClipIds
  if (!ids.length) return null
  const trackId = makeId('track')
  commitDocument(moveClipsToNewTrack(compositionState.document, ids, trackId, anchor))
  compositionState.ui.selectedTrackId = trackId
  return trackId
}

export function reorderCompositionTrack(trackId: string, toIndex: number): void {
  commitDocument(reorderTrack(compositionState.document, trackId, toIndex))
}


export function toggleCompositionTrackFlag(
  trackId: string,
  flag: 'locked' | 'muted' | 'hidden' | 'solo' | 'syncLock',
): void {
  const tracks = compositionState.document.tracks.map((track) => {
    if (track.id !== trackId) return track
    if (flag === 'syncLock') return { ...track, syncLock: !(track.syncLock ?? true) }
    if (!(flag in track)) return track
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
    if (clip.kind !== 'video' && clip.kind !== 'image' && clip.kind !== 'text' && clip.kind !== 'color') return clip
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


export function reattachCompositionVideoAudio(clipId: string): void {
  const result = reattachAudio(compositionState.document, clipId)
  commitDocument(result.document)
  const location = findClipLocation(result.document, result.videoId)
  selectCompositionClip(location.track.id, result.videoId)
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
    enabled: clip.enabled,
    sourceId: clip.sourceId,
    timelineStartTicks: clip.timelineStartTicks,
    sourceInTicks: clip.sourceInTicks,
    sourceOutTicks: clip.sourceOutTicks,
    ...(clip.timelineDurationTicks !== undefined ? { timelineDurationTicks: clip.timelineDurationTicks } : {}),
    speed: clip.speed,
    ...(clip.speedRamp ? { speedRamp: { ...clip.speedRamp, points: clip.speedRamp.points.map(point => ({ ...point })) } } : {}),
    ...embeddedAudioMix(clip),
    reversed: clip.playbackMode?.mode === 'reverse',
  }
  document = addClip(document, audioTrack.id, audioClip)
  const members = linkedClipIds(document, clipId)
  document = createCompositionLink(document, makeId('detached-audio-link'), [...members, audioId], members.length > 1)
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
    if (location.clip.stabilization?.mode === 'deshake' || location.clip.stabilization?.mode === 'vidstab') {
      throw new Error('Freeze frame нельзя совмещать со stabilization')
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
  if (stabilization.mode === 'deshake' || stabilization.mode === 'vidstab') {
    if (location.track.hidden) throw new Error('Stabilization доступна только на видимой video-дорожке')
    if (location.clip.playbackMode?.mode === 'freeze') {
      throw new Error('Stabilization нельзя совмещать с freeze frame')
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
    if (clip.timelineDurationTicks !== undefined) {
      const oldDerived = clipDurationTicks({ ...clip, timelineDurationTicks: undefined })
      const newDerived = clipDurationTicks({ ...updated, timelineDurationTicks: undefined })
      const ratio = speedRamp ? newDerived / oldDerived : (clip.speed ?? 1) / normalized
      updated.timelineDurationTicks = Math.max(1, Math.round(oldDuration * ratio))
    }
    const duration = clipDurationTicks(updated)
    const retimed = {
      ...updated,
      ...(updated.audioAnimation ? {
        audioAnimation: sliceAudioAnimation(updated.audioAnimation, 0, duration, oldDuration),
      } : {}),
      ...(clip.kind === 'audio' ? {
        fadeInTicks: clip.fadeInTicks ?? 0,
        fadeOutTicks: clip.fadeOutTicks ?? 0,
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
        fadeInTicks: updated.fadeInTicks ?? 0,
        fadeOutTicks: updated.fadeOutTicks ?? 0,
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
    if (clip.kind !== 'video' && clip.kind !== 'image' && clip.kind !== 'color') return clip
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
    if (clip.kind !== 'video' && clip.kind !== 'image' && clip.kind !== 'text' && clip.kind !== 'color') return clip
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

/** Composite Scale UI writes both axes in one commit; raw parameter commands remain independent. */
function writeAutomationEdit(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  value: import('../../composition/types.js').CompositionAnimatableValue | undefined,
  fanOutScale: boolean,
): void {
  const resolved = resolveAutomation(target, property)
  const clip = resolved[0]
  if (fanOutScale && target.kind === 'visual' && clip.kind !== 'audio' && clip.scaleLinked === true && (property === 'scaleX' || property === 'scaleY')) {
    const next = value ?? constantAnimatable(visualPropertyFallback(compositionState.document, clip, 'scaleX'))
    commitAutomationValues([
      { target, property: 'scaleX', value: next },
      { target, property: 'scaleY', value: cloneAnimatableValue(next) },
    ])
  } else resolved[5](value)
}

export function setCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  value?: number,
  timelineTicks = compositionState.transport.playheadTicks,
  fanOutScale = false,
): void {
  const resolved = resolveAutomation(target, property)
  const [clip, current, fallback, minimum, maximum] = resolved
  const duration = clipDurationTicks(clip)
  const localTicks = Math.round(timelineTicks - clip.timelineStartTicks)
  if (localTicks < 0 || localTicks > duration) throw new Error('Плейхед должен находиться внутри выбранного clip')
  const sampled = value ?? sampleAnimatableValue(current ?? constantAnimatable(fallback), localTicks)
  const timeBase = current?.mode === 'keyframes' ? current.track.timeBase : COMPOSITION_TIME_BASE
  writeAutomationEdit(target, property, upsertKeyframe(
    current,
    keyframeTickAtLocalTime(localTicks, timeBase, COMPOSITION_TIME_BASE, current?.mode === 'keyframes' ? current.track : undefined),
    clampNumber(sampled, minimum, maximum),
    fallback,
  ), fanOutScale)
}


export function updateCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  originalTick: number,
  tick: number,
  value: number,
  fanOutScale = false,
): void {
  const resolved = resolveAutomation(target, property)
  const [, current, , minimum, maximum] = resolved
  if (!current || current.mode !== 'keyframes') throw new Error('У параметра нет keyframe track')
  writeAutomationEdit(target, property, updateKeyframe(
    current,
    originalTick,
    tick,
    clampNumber(value, minimum, maximum),
  ), fanOutScale)
}


export interface CompositionKeyframeSubject {
  readonly target: CompositionAutomationTarget
  readonly property: CompositionAutomationProperty
  readonly ticks: readonly number[]
}

export function moveCompositionKeyframeSelection(subjects: readonly CompositionKeyframeSubject[], delta: number): readonly CompositionKeyframeSubject[] {
  const ids = subjects.map((subject) => JSON.stringify([subject.target.kind, subject.target.clipId, subject.target.kind === 'mask' ? subject.target.maskId : '', subject.property]))
  const groups = subjects.map((subject, index) => {
    const resolved = resolveAutomation(subject.target, subject.property)
    if (!resolved[1]) throw new Error('У параметра нет keyframe track')
    return { id: ids[index]!, value: resolved[1], durationTicks: clipDurationTicks(resolved[0]), selectedTicks: subject.ticks }
  })
  const result = moveSelectedKeyframeTracks(groups, delta, canvasTimeGrid(compositionState.document.canvas))
  commitAutomationValues(linkedScaleAutomationPatches(subjects.map((subject, index) => ({ ...subject, value: result.groups[index]!.value }))))
  return subjects.map((subject, index) => ({ ...subject, ticks: result.groups[index]!.selectedTicks }))
}

export function compositionKeyframeSelectionExtent(subjects: readonly CompositionKeyframeSubject[] = compositionState.ui.selectedKeyframes): { first: number; last: number; distinct: number } | null {
  const times = subjects.flatMap((subject) => {
    const resolved = resolveAutomation(subject.target, subject.property)
    if (resolved[1]?.mode !== 'keyframes') return []
    const timeBase = resolved[1].track.timeBase
    return subject.ticks.map((tick) => resolved[0].timelineStartTicks + Math.round(tick * COMPOSITION_TIME_BASE / timeBase))
  })
  if (!times.length) return null
  return { first: Math.min(...times), last: Math.max(...times), distinct: times.filter((tick, index) => times.indexOf(tick) === index).length }
}

export function scaleCompositionKeyframeSelection(subjects: readonly CompositionKeyframeSubject[], anchorTicks: number, factor: number): readonly CompositionKeyframeSubject[] {
  const groups = subjects.map((subject) => {
    const resolved = resolveAutomation(subject.target, subject.property)
    if (!resolved[1]) throw new Error('У параметра нет keyframe track')
    return { id: JSON.stringify([subject.target, subject.property]), value: resolved[1], durationTicks: clipDurationTicks(resolved[0]), timelineStartTicks: resolved[0].timelineStartTicks, selectedTicks: subject.ticks }
  })
  const result = scaleSelectedKeyframeTracks(groups, anchorTicks, factor, canvasTimeGrid(compositionState.document.canvas))
  if (result.factor !== 1) commitAutomationValues(linkedScaleAutomationPatches(subjects.map((subject, index) => ({ ...subject, value: result.groups[index]!.value }))))
  return subjects.map((subject, index) => ({ ...subject, ticks: result.groups[index]!.selectedTicks }))
}

export function moveCompositionAutomationKeyframes(target: CompositionAutomationTarget, property: CompositionAutomationProperty, ticks: readonly number[], delta: number): readonly number[] {
  return moveCompositionKeyframeSelection([{ target, property, ticks }], delta)[0]!.ticks
}

export function setCompositionKeyframeSelectionTangentMode(mode: 'auto' | 'free', subjects: readonly CompositionKeyframeSubject[] = compositionState.ui.selectedKeyframes): void {
  if (!subjects.length) throw new Error('Select keyframes before changing tangents')
  const ids = subjects.map(subject => JSON.stringify([subject.target,subject.property]))
  if (ids.some((id,index) => ids.indexOf(id) !== index)) throw new Error('Duplicate keyframe subjects')
  const patches = subjects.map(subject => {
    const current = resolveAutomation(subject.target,subject.property)[1]
    if (!current) throw new Error('У параметра нет keyframe track')
    return { ...subject, value: setKeyframeTangentMode(current,subject.ticks,mode) }
  })
  commitAutomationValues(linkedScaleAutomationPatches(patches))
}

export function setCompositionKeyframeSelectionInterpolation(interpolation: CompositionInterpolation, subjects: readonly CompositionKeyframeSubject[] = compositionState.ui.selectedKeyframes): void {
  if (!subjects.length) throw new Error('Select keyframes before changing easing')
  const ids = subjects.map((subject) => JSON.stringify([subject.target, subject.property]))
  if (ids.some((id, index) => ids.indexOf(id) !== index)) throw new Error('Duplicate keyframe subjects')
  const patches = subjects.map((subject) => {
    const resolved = resolveAutomation(subject.target, subject.property)
    if (!resolved[1]) throw new Error('У параметра нет keyframe track')
    return { ...subject, value: updateSelectedKeyframeInterpolation(resolved[1], subject.ticks, interpolation) }
  })
  commitAutomationValues(linkedScaleAutomationPatches(patches))
}

export function deleteCompositionKeyframeSelection(subjects: readonly CompositionKeyframeSubject[]): void {
  if (!subjects.length) throw new Error('Select keyframes before deleting')
  const ids = subjects.map((subject) => JSON.stringify([subject.target.kind, subject.target.clipId, subject.target.kind === 'mask' ? subject.target.maskId : '', subject.property]))
  if (ids.some((id, index) => ids.indexOf(id) !== index)) throw new Error('Duplicate keyframe subjects')
  const patches = subjects.map(({ target, property, ticks }) => {
    const resolved = resolveAutomation(target, property)
    const current = resolved[1]
    if (!current || current.mode !== 'keyframes' || !ticks.length || ticks.some((tick) => !current.track.keyframes.some((key) => key.tick === tick))) throw new Error('Selected keyframe no longer exists')
    let next: typeof resolved[1] = current
    if (current.track.keyframes.every((key) => ticks.includes(key.tick))) next = constantAnimatable(current.track.keyframes[current.track.keyframes.length - 1]!.value)
    else for (const tick of ticks.filter((tick, index) => ticks.indexOf(tick) === index)) if (next) next = deleteKeyframe(next, tick)
    return { target, property, value: next ?? (target.kind === 'mask' ? constantAnimatable(sampleAtPlayhead(resolved)) : undefined) }
  })
  commitAutomationValues(linkedScaleAutomationPatches(patches))
}

export function deleteCompositionAutomationKeyframes(target: CompositionAutomationTarget, property: CompositionAutomationProperty, ticks: readonly number[]): void {
  deleteCompositionKeyframeSelection([{ target, property, ticks }])
}

export function deleteCompositionAutomationKeyframe(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  tick: number,
  fanOutScale = false,
): void {
  const resolved = resolveAutomation(target, property)
  const current = resolved[1]
  if (!current) return
  const next = deleteKeyframe(current, tick)
  writeAutomationEdit(target, property, next ?? (target.kind === 'mask' ? constantAnimatable(sampleAtPlayhead(resolved)) : undefined), fanOutScale)
}


export function setCompositionAutomationInterpolation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  interpolation: Exclude<CompositionInterpolation, 'ease_in_out_cubic'>,
  fanOutScale = false,
): void {
  const resolved = resolveAutomation(target, property)
  const current = resolved[1]
  if (!current) throw new Error('Сначала добавьте ключевой кадр')
  writeAutomationEdit(target, property, updateInterpolation(current, interpolation), fanOutScale)
}


export function editCompositionAutomationTangent(
  target: CompositionAutomationTarget, property: CompositionAutomationProperty,
  tick: number, side: 'incoming' | 'outgoing', x: number, y: number, fanOutScale = false,
): void {
  const current = resolveAutomation(target, property)[1]
  if (!current) throw new Error('Сначала добавьте ключевой кадр')
  writeAutomationEdit(target, property, editKeyframeTangent(current, tick, side, x, y), fanOutScale)
}

export function setCompositionAutomationTangents(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  ticks: readonly number[],
  patch: Partial<import('../../composition/types.js').CompositionKeyframeTangents>,
  fanOutScale = false,
): void {
  const current = resolveAutomation(target, property)[1]
  if (!current) throw new Error('Сначала добавьте ключевой кадр')
  writeAutomationEdit(target, property, updateKeyframeTangents(current, ticks, patch), fanOutScale)
}

export function setCompositionAutomationExtrapolation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  side: 'before' | 'after',
  mode: import('../../composition/types.js').CompositionExtrapolationMode,
  fanOutScale = false,
): void {
  const current = resolveAutomation(target, property)[1]
  if (current?.mode !== 'keyframes') throw new Error('Сначала добавьте ключевой кадр')
  writeAutomationEdit(target, property, { ...current, track: { ...current.track, extrapolation: { before: 'hold', after: 'hold', ...current.track.extrapolation, [side]: mode } } }, fanOutScale)
}

export function clearCompositionAutomation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
  fanOutScale = false,
): void {
  const resolved = resolveAutomation(target, property)
  if (!resolved[1]) return
  writeAutomationEdit(target, property, constantAnimatable(sampleAtPlayhead(resolved)), fanOutScale)
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
      keyframeTickAtLocalTime(localTicks, current.track.timeBase, COMPOSITION_TIME_BASE, current.track),
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


export function setCompositionRoleGain(role: import('../../composition/types.js').CompositionAudioRole, gainDb: number): void {
  if (!['dialogue', 'music', 'sfx', 'voiceover'].includes(role) || !Number.isFinite(gainDb)) throw new Error('Invalid audio role gain')
  const document = compositionState.document
  commitDocument({ ...document, audioRoles: { ...document.audioRoles, [role]: { ...audioRoleMix(document, role), gainDb } } })
}

export function setCompositionAudioRole(clipId: string, role: import('../../composition/types.js').CompositionAudioRole): void {
  if (!['dialogue', 'music', 'sfx', 'voiceover'].includes(role)) throw new Error('Unsupported audio role')
  updateClip(clipId, clip => {
    if (clip.kind !== 'audio') throw new Error('Audio role requires an audio clip')
    return (clip.role ?? 'dialogue') === role ? clip : { ...clip, role }
  })
}

export function updateCompositionAudioMix(
  clipId: string,
  patch: Partial<{ dspEffects: readonly import('$lib/composition/types.js').CompositionDspEffect[]; pan: number; reversed: boolean; fadeInTicks: number; fadeOutTicks: number; voiceEffect: import('$lib/composition/types.js').CompositionVoiceEffect; pitchSemitones: number; toneDb: number; crossfadeInTicks: number; crossfadeCurve: NonNullable<import('$lib/composition/types.js').AudioClip['crossfadeCurve']>; ducking: import('$lib/composition/types.js').CompositionAudioDucking | undefined }>,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'audio') return clip
    if (patch.dspEffects !== undefined && clip.locked) throw new Error('Клип заблокирован')
    const duration = clipDurationTicks(clip)
    return {
      ...clip,
      dspEffects: patch.dspEffects === undefined ? clip.dspEffects : patch.dspEffects.map(cloneDspEffect),
      pan: patch.pan === undefined ? clip.pan ?? 0 : clampNumber(patch.pan, -1, 1),
      reversed: patch.reversed ?? clip.reversed ?? false,
      fadeInTicks: patch.fadeInTicks === undefined ? clip.fadeInTicks ?? 0 : clampTick(patch.fadeInTicks, MAX_COMPOSITION_DURATION_TICKS),
      fadeOutTicks: patch.fadeOutTicks === undefined ? clip.fadeOutTicks ?? 0 : clampTick(patch.fadeOutTicks, MAX_COMPOSITION_DURATION_TICKS),
      voiceEffect: patch.voiceEffect ?? clip.voiceEffect ?? 'none',
      pitchSemitones: patch.pitchSemitones === undefined ? clip.pitchSemitones ?? 0 : clampNumber(patch.pitchSemitones, -12, 12),
      toneDb: patch.toneDb === undefined ? clip.toneDb ?? 0 : clampNumber(patch.toneDb, -12, 12),
      crossfadeInTicks: patch.crossfadeInTicks === undefined ? clip.crossfadeInTicks ?? 0 : clampTick(patch.crossfadeInTicks, duration),
      crossfadeCurve: patch.crossfadeCurve ?? clip.crossfadeCurve ?? 'constant_gain',
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
  options?: BoundaryTransitionOptions,
): string {
  const track=compositionState.document.tracks.find(track=>track.id===trackId)
  if(!track||track.kind!=='video')throw new Error('Переход требует видеодорожку')
  const to=track.clips.find(clip=>clip.id===toClipId)
  if(!to)throw new Error('Входящий клип не найден')
  const existing=track.transitions?.find(transition=>transition.id===transitionId)
  const transition=boundaryTransition(transitionId??makeId('transition'),fromClipId,toClipId,kind,durationTicks,to.timelineStartTicks,existing,options)
  commitDocument(upsertTransition(compositionState.document, trackId, transition))
  return transition.id
}


export function removeCompositionTransition(trackId: string, transitionId: string): void {
  commitDocument(deleteTransition(compositionState.document, trackId, transitionId))
}

export function closeCompositionGapAtPlayhead(trackId: string): void {
  const document = compositionState.document
  const track = document.tracks.find((item) => item.id === trackId)
  if (!track) throw new Error('Дорожка не найдена')
  const gap = compositionGapAt(track.clips, compositionState.transport.playheadTicks)
  if (!gap) throw new Error('Установите плейхед в пробел между клипами')
  commitDocument(rippleDeleteGap(document, trackId, gap.start, gap.end))
}

export function closeSelectedCompositionGap(): void {
  const gap = compositionState.ui.selectedGap
  if (!gap) return
  commitDocument(rippleDeleteGap(compositionState.document, gap.trackId, gap.start, gap.end))
}

export function setCompositionClipsEnabled(clipIds: readonly string[], enabled: boolean): void {
  commitDocument(setClipsEnabled(compositionState.document, clipIds, enabled))
}

export function toggleSelectedCompositionClipsEnabled(): void {
  const ids = compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : compositionState.ui.selectedClipId ? [compositionState.ui.selectedClipId] : []
  if (!ids.length) return
  const enabled = ids.some((id) => findClipLocation(compositionState.document, id).clip.enabled === false)
  setCompositionClipsEnabled(ids, enabled)
}

export function setCompositionClipLocked(clipId: string, locked: boolean): void {
  commitDocument(setClipLocked(compositionState.document, clipId, locked))
}

export function copyCompositionKeyframeSelection(subjects: readonly CompositionKeyframeSubject[] = compositionState.ui.selectedKeyframes): boolean {
  if (!subjects.length) return false
  const groups = snapshotSelectedKeyframes(subjects.map((subject) => {
    const resolved = resolveAutomation(subject.target, subject.property)
    if (!resolved[1]) throw new Error('Selected keyframe no longer exists')
    return { parameter: JSON.stringify([subject.target.kind, subject.property]), value: resolved[1], selectedTicks: subject.ticks, timelineStartTicks: resolved[0].timelineStartTicks }
  }))
  if (!groups.length) return false
  compositionState.ui.clipboard = { kind: 'keyframes', groups }
  return true
}

export function pasteCompositionKeyframeClipboard(): boolean {
  const slot = compositionState.ui.clipboard
  if (slot?.kind !== 'keyframes') return false
  const ids = compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : compositionState.ui.selectedClipId ? [compositionState.ui.selectedClipId] : []
  if (!ids.length) throw new Error('Select a clip to paste keyframes onto')
  const addresses: { id: string; parameter: string; target: CompositionAutomationTarget; property: CompositionAutomationProperty; value: import('../../composition/types.js').CompositionAnimatableValue; timelineStartTicks: number }[] = []
  for (const clipId of ids) {
    const { clip } = findClipLocation(compositionState.document, clipId)
    const append = (target: CompositionAutomationTarget, properties: readonly CompositionAutomationProperty[]) => {
      for (const property of properties) {
        const parameter = JSON.stringify([target.kind, property])
        if (!slot.groups.some((group) => group.parameter === parameter)) continue
        const resolved = resolveAutomation(target, property)
        addresses.push({ id: JSON.stringify([target, property]), parameter, target, property, value: resolved[1] ?? constantAnimatable(resolved[2]), timelineStartTicks: clip.timelineStartTicks })
      }
    }
    if (clip.kind !== 'audio') append({ kind: 'visual', clipId }, COMPOSITION_VISUAL_PROPERTIES)
    if (clip.kind === 'audio' || clip.kind === 'video') append({ kind: 'audio', clipId }, COMPOSITION_AUDIO_PROPERTIES)
    if (clip.kind === 'video') for (const mask of clip.masks ?? []) append({ kind: 'mask', clipId, maskId: mask.id }, COMPOSITION_MASK_PROPERTIES)
  }
  const result = planKeyframePaste(slot.groups, addresses, compositionState.transport.playheadTicks)
  if (result.skipped.length) compositionState.ui.message = `Skipped ${result.skipped.join(', ')} — selected clips do not carry it`
  if (!result.entries.length) return false
  const patches = result.entries.map((entry) => {
    const address = addresses.find((candidate) => candidate.id === entry.id)!
    return { target: address.target, property: address.property, value: entry.value, ticks: entry.pastedTicks }
  })
  commitAutomationValues(linkedScaleAutomationPatches(patches))
  const selected: CompositionKeyframeSubject[] = []
  for (const { target, property, ticks: pastedTicks } of patches) {
    const clip = findClipLocation(compositionState.document, target.clipId).clip
    const visibleProperty = target.kind === 'visual' && clip.kind !== 'audio' && clip.scaleLinked && property === 'scaleY' ? 'scaleX' as const : property
    const landed = resolveAutomation(target, visibleProperty)[1]
    const ticks = landed?.mode === 'keyframes' ? pastedTicks.filter(tick => landed.track.keyframes.some(key => key.tick === tick)) : []
    if (!ticks.length) continue
    const previous = selected.find((subject) => JSON.stringify(subject.target) === JSON.stringify(target) && subject.property === visibleProperty)
    if (previous) {
      const combined = [...previous.ticks, ...ticks]
      selected[selected.indexOf(previous)] = { ...previous, ticks: combined.filter((tick, index) => combined.indexOf(tick) === index).sort((a, b) => a - b) }
    }
    else selected.push({ target, property: visibleProperty, ticks })
  }
  compositionState.ui.selectedKeyframes = selected
  return true
}

export function copyCompositionSelection(): boolean {
  if (compositionState.ui.selectedColorKeys.length) {
    const groups = copyColorKeys(compositionState.document,compositionState.ui.selectedColorKeys)
    compositionState.ui.clipboard = { kind: 'color-keyframes',groups }
    return true
  }
  if (copyCompositionKeyframeSelection()) return true
  const clipId = compositionState.ui.selectedClipId
  if (!clipId) return false
  findClipLocation(compositionState.document, clipId)
  compositionState.ui.clipboard = { kind: 'layer', clipId, insertSnapshot: { document: cloneGraphicValue(compositionState.document), clipIds: [...(compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : [clipId])] } }
  return true
}

export function pasteInsertCompositionClipboard(): boolean {
  const slot = compositionState.ui.clipboard
  if (!slot || slot.kind !== 'layer') return false
  const snapshot = slot.insertSnapshot ?? { document: compositionState.document, clipIds: [slot.clipId] }
  const result = pasteInsertComposition(compositionState.document, snapshot.document, snapshot.clipIds, compositionState.transport.playheadTicks, () => makeId('paste-insert'))
  commitDocument(result.document)
  const first = result.placements[0]!
  selectCompositionClip(first.trackId, first.clip.id, { single: true })
  compositionState.ui.selectedClipIds = result.placements.map(({ clip }) => clip.id)
  setCompositionPlayhead(result.end)
  return true
}

export function pasteCompositionClipboard(): boolean {
  const slot = compositionState.ui.clipboard
  if (!slot) return false
  if (slot.kind === 'color-keyframes') {
    const ids = compositionState.ui.selectedColorKeys.length ? [...new SvelteSet(compositionState.ui.selectedColorKeys.map(subject => subject.clipId))] : compositionState.ui.selectedClipIds.length ? compositionState.ui.selectedClipIds : compositionState.ui.selectedClipId ? [compositionState.ui.selectedClipId] : []
    const result = pasteColorKeys(compositionState.document,slot.groups,ids,compositionState.transport.playheadTicks)
    commitDocument(result.document)
    compositionState.ui.selectedColorKeys = result.subjects
    return true
  }
  if (slot.kind === 'keyframes') return pasteCompositionKeyframeClipboard()
  const location = findClipLocation(compositionState.document, slot.clipId)
  const id = makeId(`${location.clip.kind}-clip`)
  const trackId = makeId(`${location.clip.kind}-track`)
  const draft = addTrack(compositionState.document, makeTrack(location.clip.kind, trackId, nextTrackName(compositionState.document, 'Вставка')), location.clip.kind === 'audio' ? compositionState.document.tracks.length : 0)
  const onNewTrack = pasteClips(draft, [{ clipId: slot.clipId, id }], { allowLockedSource: true, timelineStartTicks: compositionState.transport.playheadTicks, targetTrackId: trackId })
  const projected = findClipLocation(onNewTrack, id).clip
  const destination = [...compositionState.document.tracks].reverse().find((track) => track.kind === location.clip.kind && !track.locked && !track.clips.some((clip) => clip.timelineStartTicks < clipEndTicks(projected) && clipEndTicks(clip) > projected.timelineStartTicks))
  commitDocument(destination ? pasteClips(compositionState.document, [{ clipId: slot.clipId, id }], { allowLockedSource: true, timelineStartTicks: compositionState.transport.playheadTicks, targetTrackId: destination.id }) : onNewTrack)
  selectCompositionClip(destination?.id ?? trackId, id, { single: true })
  return true
}

export function updateCompositionGraphicResponsiveTime(clipId: string, responsiveTime: import('../../composition/types').GraphicResponsiveTime | undefined): void {
  if (responsiveTime && (!Object.values(responsiveTime).every(Number.isSafeInteger) || responsiveTime.intro < 0 || responsiveTime.outro < 0 || responsiveTime.designDuration <= 0)) throw new Error('Некорректные защищённые интервалы')
  updateClip(clipId, clip => {
    if (clip.kind !== 'graphic') throw new Error('Нужен графический клип')
    const {responsiveTime: previous, ...rest} = clip
    return responsiveTime ? {...rest,responsiveTime:cloneGraphicValue(responsiveTime)} : rest
  })
}

export function updateCompositionGraphicRoll(clipId: string, roll: import('../../composition/types').GraphicRoll | undefined): void {
  if (roll && (![roll.preroll,roll.easeIn,roll.easeOut,roll.postroll].every(value => Number.isSafeInteger(value) && value >= 0) || !['off','roll','crawlLeft','crawlRight'].includes(roll.mode))) throw new Error('Некорректное движение титров')
  updateClip(clipId, clip => {
    if (clip.kind !== 'graphic') throw new Error('Нужен графический клип')
    const {roll: previous, ...rest} = clip
    return roll ? {...rest,roll:cloneGraphicValue(roll)} : rest
  })
}

export function addGraphicTemplateToComposition(template: import('../../api').NativeGraphicTemplate): string {
  let document=compositionState.document
  const start=compositionState.transport.playheadTicks
  if (start+template.durationUs>MAX_COMPOSITION_DURATION_TICKS) throw new Error('Шаблон выходит за пределы композиции')
  let trackId=findAvailableTrack(document,'graphic',start,start+template.durationUs)?.id
  if (!trackId) {trackId=makeId('graphic-track');document=addTrack(document,makeTrack('graphic',trackId,nextTrackName(document,'Графика')),0)}
  const clipId=makeId('graphic')
  commitDocument(addClip(document,trackId,createGraphicTemplateClip(document.canvas,template,clipId,start)))
  selectCompositionClip(trackId,clipId)
  return clipId
}

export function updateCompositionSourceGainKeys(clipId:string,keys:readonly import('$lib/composition/types').WireConcatParamKey[]):void {
 const copied=cloneGainSourceKeys(keys)
 updateClip(clipId,clip=>{
  if(clip.kind!=='audio'&&clip.kind!=='video')throw new Error('У клипа нет звука')
  if(clip.locked)throw new Error('Клип заблокирован')
  if(copied.length&&clip.audioAnimation?.gain)throw new Error('Сначала отключите анимацию громкости по времени')
  return {...clip,concatGainKeys:copied,...(clip.kind==='video'?{independentAudio:true}:{})}
 })
}


export function updateCompositionAudioStream(clipId:string,stream:number|undefined):void{
 if(stream!==undefined&&(!Number.isInteger(stream)||stream<0||stream>0xffffffff))throw new Error('Неверный индекс аудиопотока')
 updateClip(clipId,clip=>{
  if(clip.kind!=='audio'&&clip.kind!=='video')throw new Error('У клипа нет звука')
  if(clip.locked)throw new Error('Клип заблокирован')
  if(clip.audioStream===stream)return clip
  const changed={...clip,audioStream:stream};if(stream===undefined)delete changed.audioStream
  return changed
 })
}

export async function reactivateCompositionSourceEffect(clipId:string,index:number,validateCommit?:()=>void):Promise<void>{
 const before=compositionState.document,identity=JSON.stringify(before),projectId=compositionState.projectId,revision=compositionState.projectRevision
 const location=findClipLocation(before,clipId)
 if((location.clip.kind!=='video'&&location.clip.kind!=='image')||!Number.isInteger(index))throw new Error('Визуальный эффект не найден')
 if(location.track.locked||location.clip.locked)throw new Error('Клип или дорожка заблокированы')
 const effect=location.clip.effects?.[index];if(!effect)throw new Error('Визуальный эффект не найден')
 const check=()=>{if(compositionState.document!==before||JSON.stringify(before)!==identity||compositionState.projectId!==projectId||compositionState.projectRevision!==revision)throw new Error('Проект изменился во время преобразования');if(compositionState.save.busy)throw new Error('Дождитесь сохранения проекта');validateCommit?.()}
 check()
 const {upgradeConcatEffectLinks,evaluateNestedFrame}=await import('$lib/api')
 let native:import('$lib/composition/types').WireVideoEffect
 if(effect.kind==='concat_bypass'){
  const response=await upgradeConcatEffectLinks([{...effect.link,enabled:true}],undefined,true);check()
  const converted=response.nativeEffects?.[0]
  if(response.links.length!==1||!converted||converted.kind==='concat_bypass'||converted.kind==='native_bypass')throw new Error(`Эффект ${effect.link.id} пока нельзя включить: нативная реализация отсутствует`)
  native=converted
 }else if(effect.kind==='native_bypass')native=effect.effect
 else native={kind:'native_bypass',effect}
 const replace=<T extends {readonly id:string;readonly effects?:readonly import('$lib/composition/types').WireVideoEffect[]}>(clip:T):T=>clip.id===clipId?{...clip,effects:clip.effects!.map((effect,i)=>i===index?native:effect)}:clip
 const tracks=before.tracks.map(track=>track.id===location.track.id&&(track.kind==='video'||track.kind==='image')?track.kind==='video'?{...track,clips:track.clips.map(replace)}:{...track,clips:track.clips.map(replace)}:track)
 const candidate={...before,tracks}
 const {buildCompositionWireDocument}=await import('$lib/composition/payload')
 await evaluateNestedFrame({schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(candidate),instances:[]}}},0);check()
 commitDocument(candidate)
}

export async function editCompositionConcatClipKeys(clipId:string,property:string,keys:readonly import('$lib/composition/types').WireConcatParamKey[],validateCommit?:()=>void):Promise<void>{
 const before=compositionState.document,identity=JSON.stringify(before),projectId=compositionState.projectId,revision=compositionState.projectRevision
 const location=findClipLocation(before,clipId),clip=location.clip
 if((clip.kind!=='video'&&clip.kind!=='image')||!clip.concatAnimation)throw new Error('Исходная анимация Concat не найдена')
 if(location.track.locked||clip.locked)throw new Error('Клип или дорожка заблокированы')
 const bounds:Record<string,readonly[number,number]>={scale:[0.05,8],offsetX:[-3,3],offsetY:[-3,3],rotation:[-Infinity,Infinity],opacity:[0,1]},range=bounds[property]
 if(!range||keys.some(key=>!key.derived&&(key.value<range[0]||key.value>range[1])))throw new Error('Недопустимые ключи свойства Concat')
 const animation={...clip.concatAnimation,keys:{...clip.concatAnimation.keys,[property]:keys.map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))}}
 const {validConcatClipAnimation}=await import('$lib/composition/concatClipAnimation');if(!validConcatClipAnimation(animation))throw new Error('Недопустимые ключи свойства Concat')
 const check=()=>{if(compositionState.document!==before||JSON.stringify(before)!==identity||compositionState.projectId!==projectId||compositionState.projectRevision!==revision)throw new Error('Проект изменился во время проверки');if(compositionState.save.busy)throw new Error('Дождитесь сохранения проекта');validateCommit?.()}
 check()
 const replace=<T extends {readonly id:string}>(candidate:T):T=>candidate.id===clipId?{...candidate,concatAnimation:animation}:candidate
 const tracks=before.tracks.map(track=>track.id===location.track.id&&(track.kind==='video'||track.kind==='image')?track.kind==='video'?{...track,clips:track.clips.map(replace)}:{...track,clips:track.clips.map(replace)}:track)
 const candidate={...before,tracks}
 const {buildCompositionWireDocument}=await import('$lib/composition/payload'),{evaluateNestedFrame}=await import('$lib/api')
 await evaluateNestedFrame({schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(candidate),instances:[]}}},0);check();commitDocument(candidate)
}
