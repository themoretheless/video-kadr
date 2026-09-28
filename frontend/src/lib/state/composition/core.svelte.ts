import * as api from '../../api'
import {
  MAX_SYNC_SAMPLES,
  waveformEnergy
} from '../../audio/sync'
import type {
  WaveformSummary
} from '../../audio/waveform'
import {
  addClip,
  addTrack,
  createComposition,
  findClipLocation
} from '../../composition/commands'
import {
  DEFAULT_COMPOSITION_RENDER_OUTPUT,
  normalizeCompositionRenderOutput
} from '../../composition/payload'
import {
  compositionMarkers,
  type CompositionMarker
} from '../../composition/markers'
import {
  compileMulticamCuts,
  type MulticamGroup
} from '../../composition/multicam'
import {
  clipDurationTicks,
  clipEndTicks,
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_DURATION_TICKS,
  type AudioClip,
  type AudioTrack,
  type Composition,
  type CompositionAudioProperty,
  type CompositionClip,
  type CompositionDeliveryProfile,
  type CompositionQualityTier,
  type CompositionRenderOutput,
  type CompositionMaskProperty,
  type CompositionSource,
  type CompositionTrack,
  type CompositionVideoMask,
  type CompositionVisualProperty,
  type ImageClip,
  type ImageTrack,
  type TextClip,
  type TextTrack,
  type TrackKind,
  type VideoClip,
  type VideoTrack,
  type VisualClip
} from '../../composition/types'
import {
  audioPropertyBounds,
  audioPropertyFallback,
  maskPropertyBounds,
  maskPropertyValue,
  sampleAnimatableValue,
  visualPropertyBounds,
  visualPropertyFallback
} from '../../composition/keyframes'
import {
  compositionDurationTicks,
  normalizeComposition,
  primaryCompositionVideoTrack
} from '../../composition/validation'
import type {
  MediaEntry,
  MediaInfo,
  MediaType,
  ResultInfo
} from '../../types'

const LEGACY_DRAFT_KEY = 'video-kadr:composition-draft:v1'
const DRAFTS_KEY = 'video-kadr:composition-drafts:v2'
export const MODE_KEY = 'video-kadr:editor-mode:v1'
export const DEFAULT_IMAGE_DURATION_TICKS = 5 * COMPOSITION_TIME_BASE
export const DEFAULT_TEXT_DURATION_TICKS = 4 * COMPOSITION_TIME_BASE
export const SNAP_THRESHOLD_PX = 8

export type EditorMode = 'legacy' | 'composition'

export interface CompositionMedia {
  id: string
  url: string
  filename: string
  mediaType: MediaType
  duration: number
  width: number
  height: number
  fps?: number | null
  vcodec?: string | null
  acodec?: string | null
}

interface StoredDraft {
  document: Composition
  media?: Record<string, CompositionMedia>
  projectId?: string | null
  projectRevision?: number | null
  projectName?: string
  exportSettings?: CompositionRenderOutput
  dirty?: boolean
}

interface StoredDraftCollection {
  version: 2
  activeProjectId: string | null
  unsaved?: StoredDraft
  projects: Record<string, StoredDraft>
}

export type CompositionStorage = Pick<Storage, 'getItem' | 'setItem'>

export const DEFAULT_CANVAS = {
  width: 1280,
  height: 720,
  fps: 30,
  backgroundColor: '#000000',
} as const

export let compositionStorageOverride: CompositionStorage | null | undefined
export let compositionProjectsEtag: string | null = null

const restored = readActiveStoredDraft()
const restoredOutput = normalizeCompositionRenderOutput(restored?.exportSettings)

export const editorMode = $state({ value: readStoredMode() })

export const compositionState = $state({
  document: cloneComposition(restored?.document ?? createComposition(DEFAULT_CANVAS)),
  media: { ...(restored?.media ?? {}) } as Record<string, CompositionMedia>,
  projectId: restored?.projectId ?? null as string | null,
  projectRevision: restored?.projectRevision ?? null as number | null,
  projectName: restored?.projectName?.trim() || 'Новая композиция',
  projects: [] as api.CompositionProjectDto[],
  ui: {
    selectedTrackId: null as string | null,
    selectedClipId: null as string | null,
    selectedMarkerId: null as string | null,
    selectedMulticamGroupId: null as string | null,
    zoomPxPerSecond: 84,
    snapEnabled: true,
    message: '',
  },
  history: {
    past: [] as Composition[],
    future: [] as Composition[],
    pastMedia: [] as Array<Record<string, CompositionMedia>>,
    futureMedia: [] as Array<Record<string, CompositionMedia>>,
    pastOutput: [] as CompositionRenderOutput[],
    futureOutput: [] as CompositionRenderOutput[],
  },
  transport: {
    playheadTicks: 0,
    playing: false,
  },
  save: {
    busy: false,
    error: '',
  },
  export: {
    running: false,
    jobId: null as string | null,
    progress: null as number | null,
    stage: null as string | null,
    error: '',
    result: null as ResultInfo | null,
    profile: { ...restoredOutput.profile } as CompositionDeliveryProfile,
    qualityTier: restoredOutput.qualityTier as CompositionQualityTier,
    videoBitrateKbps: restoredOutput.videoBitrateKbps ?? null as number | null,
    targetSizeBytes: restoredOutput.targetSizeBytes ?? null as number | null,
    rangeInTicks: null as number | null,
    rangeOutTicks: null as number | null,
  },
})

export let autosaveTimer: ReturnType<typeof setTimeout> | undefined
export let activeDraftDirty = restored?.dirty ?? false
export let openCompositionRevision = 0

export function bumpOpenCompositionRevision(): void {
  openCompositionRevision += 1
}

export function nextOpenCompositionRevision(): number {
  return ++openCompositionRevision
}

export function isCurrentOpenCompositionRevision(revision: number): boolean {
  return revision === openCompositionRevision
}

export function setProjectWriteBusy(value: boolean): void {
  projectWriteBusy = value
}

export function setCompositionProjectsEtag(etag: string | null): void {
  compositionProjectsEtag = etag
}

export function setActiveDraftDirty(value: boolean): void {
  activeDraftDirty = value
}
export let projectWriteBusy = false
export const UNSAVED_GUARD_MESSAGE = 'Сначала сохраните текущую композицию'
export const PROJECT_BUSY_MESSAGE = 'Дождитесь завершения сохранения проекта'

export function compositionRenderOutput(): CompositionRenderOutput {
  return {
    profile: { ...compositionState.export.profile },
    qualityTier: compositionState.export.qualityTier,
    ...(compositionState.export.videoBitrateKbps === null
      ? {}
      : { videoBitrateKbps: compositionState.export.videoBitrateKbps }),
    ...(compositionState.export.targetSizeBytes === null
      ? {}
      : { targetSizeBytes: compositionState.export.targetSizeBytes }),
  }
}

export function clearCompositionExportRange(): void {
  compositionState.export.rangeInTicks = null
  compositionState.export.rangeOutTicks = null
  compositionState.export.result = null
  compositionState.export.error = ''
}

export function selectCompositionClip(trackId: string | null, clipId: string | null): void {
  compositionState.ui.selectedTrackId = trackId
  compositionState.ui.selectedClipId = clipId
}

export function selectedCompositionTrack(): CompositionTrack | null {
  const id = compositionState.ui.selectedTrackId
  return compositionState.document.tracks.find((track) => track.id === id) ?? null
}

export function compositionDuration(): number {
  return compositionDurationTicks(compositionState.document)
}

export function compositionMarkerList(): readonly CompositionMarker[] {
  return compositionMarkers(compositionState.document)
}

export function compositionMulticamGroups(): readonly MulticamGroup[] {
  return compositionState.document.multicamGroups ?? []
}

export function addVideo(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
  let next = document
  let track = next.tracks.find((candidate): candidate is VideoTrack => candidate.kind === 'video' && !candidate.locked)
  if (!track) {
    const id = makeId('video-track')
    next = addTrack(next, makeTrack('video', id, nextTrackName(next, 'Видео')))
    track = next.tracks.find((candidate): candidate is VideoTrack => candidate.id === id)!
  }
  const start = track.clips.reduce((end, clip) => Math.max(end, clipEndTicks(clip)), 0)
  const clipId = makeId('video-clip')
  const clip: VideoClip = {
    id: clipId,
    kind: 'video',
    sourceId: source.id,
    timelineStartTicks: start,
    sourceInTicks: 0,
    sourceOutTicks: source.durationTicks,
    speed: 1,
    transform: { x: 0, y: 0, width: next.canvas.width, height: next.canvas.height, fit: 'contain' },
    opacity: 1,
    rotationDegrees: 0,
    blendMode: 'normal',
    sourceAudioEnabled: source.hasAudio,
    audioGain: 1,
    audioPan: 0,
  }
  return { document: addClip(next, track.id, clip), clipId }
}

export function addAudio(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
  let next = document
  const start = compositionState.transport.playheadTicks
  const videoEnd = primaryVideoEnd(next)
  const maxDuration = videoEnd > start ? videoEnd - start : source.durationTicks
  const duration = Math.min(source.durationTicks, maxDuration)
  if (duration <= 0) throw new Error('После плейхеда нет места для аудиоклипа')
  let track = findAvailableTrack(next, 'audio', start, start + duration) as AudioTrack | undefined
  if (!track) {
    const id = makeId('audio-track')
    next = addTrack(next, makeTrack('audio', id, nextTrackName(next, 'Аудио')))
    track = next.tracks.find((candidate): candidate is AudioTrack => candidate.id === id)!
  }
  const clipId = makeId('audio-clip')
  const clip: AudioClip = {
    id: clipId,
    kind: 'audio',
    sourceId: source.id,
    timelineStartTicks: start,
    sourceInTicks: 0,
    sourceOutTicks: duration,
    speed: 1,
    gain: 1,
    pan: 0,
    reversed: false,
    fadeInTicks: 0,
    fadeOutTicks: 0,
  }
  return { document: addClip(next, track.id, clip), clipId }
}

export function addImage(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
  let next = document
  const start = compositionState.transport.playheadTicks
  const duration = Math.min(DEFAULT_IMAGE_DURATION_TICKS, MAX_COMPOSITION_DURATION_TICKS - start)
  if (duration <= 0) throw new Error('После плейхеда нет места для изображения')
  let track = findAvailableTrack(next, 'image', start, start + duration) as ImageTrack | undefined
  if (!track) {
    const id = makeId('image-track')
    next = addTrack(next, makeTrack('image', id, nextTrackName(next, 'Изображение')), 0)
    track = next.tracks.find((candidate): candidate is ImageTrack => candidate.id === id)!
  }
  const clipId = makeId('image-clip')
  const scale = Math.min(next.canvas.width / source.width, next.canvas.height / source.height, 1)
  const clip: ImageClip = {
    id: clipId,
    kind: 'image',
    sourceId: source.id,
    timelineStartTicks: start,
    durationTicks: duration,
    transform: {
      x: 0,
      y: 0,
      width: Math.max(1, Math.round(source.width * scale)),
      height: Math.max(1, Math.round(source.height * scale)),
      fit: 'contain',
    },
    opacity: 1,
    rotationDegrees: 0,
    blendMode: 'normal',
  }
  return { document: addClip(next, track.id, clip), clipId }
}

export function rebuildMulticamDocument(
  document: Composition,
  group: MulticamGroup,
  previousGroup?: MulticamGroup,
): Composition {
  const compiled = compileMulticamCuts(document, group)
  /* Ephemeral membership checks — not reactive state. */
  /* eslint-disable svelte/prefer-svelte-reactivity */
  const previousVideoIds = new Set(previousGroup?.switches.map((change) => change.clipId) ?? [])
  const currentVideoIds = new Set(group.switches.map((change) => change.clipId))
  const ownedVideoIds = new Set([...previousVideoIds, ...currentVideoIds])
  /* eslint-enable svelte/prefer-svelte-reactivity */
  const videoTrack = document.tracks.find((track) => track.id === group.videoTrackId)
  if (!videoTrack || videoTrack.kind !== 'video') throw new Error('Multicam program video track недоступна')
  for (const clip of videoTrack.clips) {
    if (currentVideoIds.has(clip.id) && !previousVideoIds.has(clip.id) && previousGroup) {
      throw new Error(`Multicam output id ${clip.id} конфликтует с чужим clip`)
    }
  }

  /* eslint-disable svelte/prefer-svelte-reactivity */
  const previousAudioIds = new Set(previousGroup?.audioClipId ? [previousGroup.audioClipId] : [])
  const currentAudioIds = new Set(group.audioClipId ? [group.audioClipId] : [])
  const ownedAudioIds = new Set([...previousAudioIds, ...currentAudioIds])
  /* eslint-enable svelte/prefer-svelte-reactivity */
  const tracks = document.tracks.map((track) => {
    if (track.id === compiled.videoTrackId && track.kind === 'video') {
      const clips = [
        ...track.clips.filter((clip) => !ownedVideoIds.has(clip.id)),
        ...compiled.videoClips,
      ].sort(compareCompositionClips)
      return withTrackClips(track, clips)
    }
    if (compiled.audioTrackId && track.id === compiled.audioTrackId && track.kind === 'audio') {
      const clips = [
        ...track.clips.filter((clip) => !ownedAudioIds.has(clip.id)),
        ...(compiled.audioClip ? [compiled.audioClip] : []),
      ].sort(compareCompositionClips)
      return withTrackClips(track, clips)
    }
    return track
  })
  return { ...document, tracks }
}

function compareCompositionClips(left: CompositionClip, right: CompositionClip): number {
  return left.timelineStartTicks - right.timelineStartTicks || left.id.localeCompare(right.id)
}

export function resampleWaveformEnergy(summary: WaveformSummary, sampleRateHz: number): Float32Array {
  const source = waveformEnergy(summary.buckets)
  if (!source.length || !Number.isFinite(summary.durationSeconds) || summary.durationSeconds <= 0) {
    throw new Error('Waveform summary не содержит samples')
  }
  const count = Math.min(MAX_SYNC_SAMPLES, Math.max(1, Math.round(summary.durationSeconds * sampleRateHz)))
  return Float32Array.from({ length: count }, (_, index) => {
    const position = Math.min(source.length - 1, (index / Math.max(1, count - 1)) * (source.length - 1))
    const left = Math.floor(position)
    const right = Math.min(source.length - 1, left + 1)
    const fraction = position - left
    return source[left]! * (1 - fraction) + source[right]! * fraction
  })
}

export function updateClip(clipId: string, update: (clip: CompositionClip) => CompositionClip): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.track.locked) throw new Error('Дорожка заблокирована')
  const tracks = compositionState.document.tracks.map((track) => {
    if (track.id !== location.track.id) return track
    return withTrackClips(track, track.clips.map((clip) => clip.id === clipId ? update(clip) : clip))
  })
  commitDocument({ ...compositionState.document, tracks })
}

export function requireVisualAutomation(clipId: string): { readonly clip: VisualClip; readonly track: CompositionTrack } {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind === 'audio') throw new Error('Keyframes доступны только для visual clips')
  return { clip: location.clip, track: location.track }
}

export function requireVisualOverlay(clipId: string): { readonly clip: VisualClip; readonly track: CompositionTrack } {
  return requireVisualAutomation(clipId)
}

export function requireAudioAutomationClip(
  clipId: string,
): { readonly clip: VideoClip | AudioClip; readonly track: CompositionTrack } {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind !== 'video' && location.clip.kind !== 'audio') {
    throw new Error('Audio keyframes доступны только для video/audio clips')
  }
  if (location.clip.kind === 'video') {
    const primary = primaryCompositionVideoTrack(compositionState.document)
    if (location.track.kind !== 'video' || location.track.id !== primary?.id) {
      throw new Error('Embedded audio keyframes доступны только на основной видеодорожке')
    }
    if (!compositionState.document.sources[location.clip.sourceId]?.hasAudio) {
      throw new Error('Video source не содержит audio stream')
    }
  }
  return { clip: location.clip, track: location.track }
}

export function updateAudioAnimationValue(
  clipId: string,
  property: CompositionAudioProperty,
  value: import('../../composition/types.js').CompositionAnimatableValue | undefined,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'video' && clip.kind !== 'audio') return clip
    const audioAnimation = { ...(clip.audioAnimation ?? {}) }
    if (value) audioAnimation[property] = value
    else delete audioAnimation[property]
    if (Object.keys(audioAnimation).length) return { ...clip, audioAnimation }
    const { audioAnimation: removed, ...withoutAudioAnimation } = clip
    void removed
    return withoutAudioAnimation
  })
}

export function requireVideoMask(
  clipId: string,
  maskId: string,
): { readonly clip: VideoClip; readonly mask: CompositionVideoMask } {
  const location = requireVisualOverlay(clipId)
  if (location.clip.kind !== 'video') throw new Error('Masks доступны только для video overlay')
  const mask = location.clip.masks?.find((candidate) => candidate.id === maskId)
  if (!mask) throw new Error('Mask не найдена')
  return { clip: location.clip, mask }
}

export function updateMaskAnimationValue(
  clipId: string,
  maskId: string,
  property: CompositionMaskProperty,
  value: import('../../composition/types.js').CompositionAnimatableValue,
): void {
  requireVideoMask(clipId, maskId)
  updateClip(clipId, (clip) => clip.kind === 'video'
    ? {
        ...clip,
        masks: (clip.masks ?? []).map((mask) => mask.id === maskId ? { ...mask, [property]: value } : mask),
      }
    : clip)
}

export type CompositionAutomationTarget =
  | { readonly kind: 'visual'; readonly clipId: string }
  | { readonly kind: 'audio'; readonly clipId: string }
  | { readonly kind: 'mask'; readonly clipId: string; readonly maskId: string }

export type CompositionAutomationProperty =
  CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty

type ResolvedAutomation = readonly [
  clip: CompositionClip,
  current: import('../../composition/types.js').CompositionAnimatableValue | undefined,
  fallback: number,
  minimum: number,
  maximum: number,
  write: (value: import('../../composition/types.js').CompositionAnimatableValue | undefined) => void,
]

export function resolveAutomation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
): ResolvedAutomation {
  if (target.kind === 'visual') {
    const visualProperty = property as CompositionVisualProperty
    const { clip } = requireVisualAutomation(target.clipId)
    const bounds = visualPropertyBounds(visualProperty)
    return [
      clip,
      clip.animation?.[visualProperty],
      visualPropertyFallback(compositionState.document, clip, visualProperty),
      bounds.minimum,
      bounds.maximum,
      (value) => updateVisualAnimationValue(target.clipId, visualProperty, value),
    ]
  }
  if (target.kind === 'audio') {
    const { clip } = requireAudioAutomationClip(target.clipId)
    const audioProperty = property as CompositionAudioProperty
    const bounds = audioPropertyBounds(audioProperty)
    return [
      clip,
      clip.audioAnimation?.[audioProperty],
      audioPropertyFallback(clip, audioProperty),
      bounds.minimum,
      bounds.maximum,
      (value) => updateAudioAnimationValue(target.clipId, audioProperty, value),
    ]
  }
  const { clip, mask } = requireVideoMask(target.clipId, target.maskId)
  const maskProperty = property as CompositionMaskProperty
  const bounds = maskPropertyBounds(maskProperty)
  const current = maskPropertyValue(mask, maskProperty)
  return [
    clip,
    current,
    sampleAnimatableValue(current, 0),
    bounds.minimum,
    bounds.maximum,
    (value) => updateMaskAnimationValue(target.clipId, target.maskId, maskProperty, value!),
  ]
}

export function sampleAtPlayhead(resolved: ResolvedAutomation): number {
  const localTicks = clampTick(
    compositionState.transport.playheadTicks - resolved[0].timelineStartTicks,
    clipDurationTicks(resolved[0]),
  )
  return sampleAnimatableValue(resolved[1]!, localTicks)
}

export function updateVisualAnimationValue(
  clipId: string,
  property: CompositionVisualProperty,
  value: import('../../composition/types.js').CompositionAnimatableValue | undefined,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind === 'audio') return clip
    const animation = { ...(clip.animation ?? {}) }
    if (value) animation[property] = value
    else delete animation[property]
    return { ...clip, animation }
  })
}

function withTrackClips(track: CompositionTrack, clips: readonly CompositionClip[]): CompositionTrack {
  switch (track.kind) {
    case 'video': return { ...track, clips: clips as VideoClip[] }
    case 'audio': return { ...track, clips: clips as AudioClip[] }
    case 'image': return { ...track, clips: clips as ImageClip[] }
    case 'text': return { ...track, clips: clips as TextClip[] }
  }
}

export function findAvailableTrack(document: Composition, kind: TrackKind, start: number, end: number): CompositionTrack | undefined {
  return document.tracks.find(
    (track) =>
      track.kind === kind &&
      !track.locked &&
      track.clips.every((clip) => end <= clip.timelineStartTicks || start >= clipEndTicks(clip)),
  )
}

export function makeTrack(kind: TrackKind, id: string, name: string): CompositionTrack {
  const common = { id, name, kind, locked: false, clips: [] as const }
  switch (kind) {
    case 'video': return { ...common, kind, hidden: false, muted: false, transitions: [] } as VideoTrack
    case 'audio': return { ...common, kind, muted: false, solo: false } as AudioTrack
    case 'image': return { ...common, kind, hidden: false } as ImageTrack
    case 'text': return { ...common, kind, hidden: false } as TextTrack
  }
}

export function commitDocument(
  document: Composition,
  media: Record<string, CompositionMedia> = compositionState.media,
  output: CompositionRenderOutput = compositionRenderOutput(),
): void {
  const normalized = normalizeComposition(document)
  const nextMedia = cloneCompositionMedia(media)
  const nextOutput: CompositionRenderOutput = {
    profile: { ...output.profile },
    qualityTier: output.qualityTier,
    ...(output.videoBitrateKbps === undefined ? {} : { videoBitrateKbps: output.videoBitrateKbps }),
    ...(output.targetSizeBytes === undefined ? {} : { targetSizeBytes: output.targetSizeBytes }),
  }
  if (
    JSON.stringify(normalized) === JSON.stringify(compositionState.document) &&
    JSON.stringify(nextMedia) === JSON.stringify(compositionState.media) &&
    JSON.stringify(nextOutput) === JSON.stringify(compositionRenderOutput())
  ) return
  compositionState.history.past.push(cloneComposition(compositionState.document))
  compositionState.history.pastMedia.push(cloneCompositionMedia(compositionState.media))
  compositionState.history.pastOutput.push(compositionRenderOutput())
  if (compositionState.history.past.length > 100) {
    compositionState.history.past.shift()
    compositionState.history.pastMedia.shift()
    compositionState.history.pastOutput.shift()
  }
  compositionState.history.future = []
  compositionState.history.futureMedia = []
  compositionState.history.futureOutput = []
  compositionState.document = cloneComposition(normalized)
  compositionState.media = nextMedia
  compositionState.export.profile = { ...nextOutput.profile }
  compositionState.export.qualityTier = nextOutput.qualityTier
  compositionState.export.videoBitrateKbps = nextOutput.videoBitrateKbps ?? null
  compositionState.export.targetSizeBytes = nextOutput.targetSizeBytes ?? null
  compositionState.export.result = null
  compositionState.export.error = ''
  stopAtDuration()
  scheduleAutosave()
}

export function replaceDocument(document: Composition, keepHistory: boolean, autosave = true): void {
  const normalized = normalizeComposition(document)
  if (keepHistory) {
    compositionState.history.past.push(cloneComposition(compositionState.document))
    compositionState.history.pastMedia.push(cloneCompositionMedia(compositionState.media))
    compositionState.history.pastOutput.push(compositionRenderOutput())
  } else {
    compositionState.history.past = []
    compositionState.history.pastMedia = []
    compositionState.history.pastOutput = []
  }
  compositionState.history.future = []
  compositionState.history.futureMedia = []
  compositionState.history.futureOutput = []
  compositionState.document = cloneComposition(normalized)
  compositionState.ui.selectedTrackId = null
  compositionState.ui.selectedClipId = null
  compositionState.ui.selectedMarkerId = null
  compositionState.ui.selectedMulticamGroupId = null
  compositionState.transport.playheadTicks = 0
  compositionState.transport.playing = false
  if (autosave) scheduleAutosave()
}

export function cloneCompositionMedia(media: Record<string, CompositionMedia>): Record<string, CompositionMedia> {
  return Object.fromEntries(Object.entries(media).map(([id, item]) => [id, { ...item }]))
}

export function repairSelection(): void {
  const clipId = compositionState.ui.selectedClipId
  if (clipId) {
    try {
      const location = findClipLocation(compositionState.document, clipId)
      compositionState.ui.selectedTrackId = location.track.id
    } catch {
      selectCompositionClip(null, null)
    }
  }
  if (compositionState.ui.selectedMarkerId && !compositionMarkerList().some((marker) => marker.id === compositionState.ui.selectedMarkerId)) {
    compositionState.ui.selectedMarkerId = null
  }
  if (
    compositionState.ui.selectedMulticamGroupId &&
    !compositionMulticamGroups().some((group) => group.id === compositionState.ui.selectedMulticamGroupId)
  ) {
    compositionState.ui.selectedMulticamGroupId = null
  }
}

export function stopAtDuration(): void {
  const duration = compositionDuration()
  if (compositionState.transport.playheadTicks > duration) {
    compositionState.transport.playheadTicks = duration
  }
  if (!duration) compositionState.transport.playing = false
}

export function firstFreeStart(track: CompositionTrack, requested: number, duration: number, excludeId: string): number {
  let cursor = requested
  for (const clip of track.clips) {
    if (clip.id === excludeId || clipEndTicks(clip) <= cursor) continue
    if (cursor + duration <= clip.timelineStartTicks) return cursor
    cursor = clipEndTicks(clip)
  }
  return cursor
}

function primaryVideoEnd(document: Composition): number {
  const track = document.tracks.find((candidate) => candidate.kind === 'video' && !candidate.hidden)
  return track?.clips.reduce((end, clip) => Math.max(end, clipEndTicks(clip)), 0) ?? 0
}

export function snapThresholdTicks(): number {
  return Math.max(1, Math.round((SNAP_THRESHOLD_PX / compositionState.ui.zoomPxPerSecond) * COMPOSITION_TIME_BASE))
}

export function rememberMedia(media: MediaInfo, mediaType: MediaType): void {
  compositionState.media = {
    ...compositionState.media,
    [media.id]: {
      id: media.id,
      url: media.url,
      filename: media.filename,
      mediaType,
      duration: media.duration,
      width: media.width,
      height: media.height,
      fps: media.fps ?? null,
      vcodec: media.vcodec ?? null,
      acodec: media.acodec ?? null,
    },
  }
  scheduleAutosave()
}

export function isMediaEntry(value: MediaEntry | CompositionSource): value is MediaEntry {
  return value.kind === 'source' || value.kind === 'output'
}

export function nextTrackName(document: Composition, label: string): string {
  const count = document.tracks.filter((track) => track.name.startsWith(label)).length
  return `${label} ${count + 1}`
}

export function makeId(prefix: string): string {
  const uuid = globalThis.crypto?.randomUUID?.()
  return `${prefix}-${uuid ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`}`
}

export function autoBeatMarkerId(tick: number, reserved: string[]): string {
  const base = `auto-beat-${tick}`
  let id = base
  let suffix = 2
  while (reserved.includes(id)) {
    id = `${base}-${suffix}`
    suffix += 1
  }
  reserved.push(id)
  return id
}

export function validFps(value?: number | null): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 && value <= 60 ? value : 30
}

export function evenDimension(value: number, fallback: number, max: number): number {
  const integer = positiveInteger(value) || fallback
  return Math.max(16, Math.min(integer - (integer % 2), max))
}

export function positiveNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0
}

export function positiveInteger(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? Math.round(value) : 0
}

export function clampNumber(value: number, min: number, max: number): number {
  return Number.isFinite(value) ? Math.max(min, Math.min(value, max)) : min
}

export function clampTick(value: number, max: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.round(Math.max(0, Math.min(value, max)))
}

export function cloneComposition(document: Composition): Composition {
  return JSON.parse(JSON.stringify(document)) as Composition
}

export function getStorage(): CompositionStorage | null {
  if (compositionStorageOverride !== undefined) return compositionStorageOverride
  if (typeof window === 'undefined') return null
  try {
    return window.localStorage
  } catch {
    return null
  }
}

function readStoredMode(): EditorMode {
  return getStorage()?.getItem(MODE_KEY) === 'composition' ? 'composition' : 'legacy'
}

export function readActiveStoredDraft(): StoredDraft | null {
  const drafts = readStoredDraftCollection()
  if (!drafts) return null
  return drafts.activeProjectId === null
    ? drafts.unsaved ?? null
    : drafts.projects[drafts.activeProjectId] ?? drafts.unsaved ?? null
}

export function readStoredDraftCollection(): StoredDraftCollection | null {
  const storage = getStorage()
  if (!storage) return null
  try {
    const parsed = JSON.parse(storage.getItem(DRAFTS_KEY) ?? 'null') as Partial<StoredDraftCollection> | null
    if (parsed?.version === 2 && parsed.projects && typeof parsed.projects === 'object') {
      const projects = Object.create(null) as Record<string, StoredDraft>
      for (const [id, value] of Object.entries(parsed.projects)) {
        const draft = normalizeStoredDraft(value, id)
        if (draft) projects[id] = draft
      }
      const unsaved = normalizeStoredDraft(parsed.unsaved, null)
      const activeProjectId = typeof parsed.activeProjectId === 'string' && projects[parsed.activeProjectId]
        ? parsed.activeProjectId
        : null
      return { version: 2, activeProjectId, ...(unsaved ? { unsaved } : {}), projects }
    }
  } catch {
    // Fall through to the v1 migration.
  }

  try {
    const legacy = normalizeStoredDraft(JSON.parse(storage.getItem(LEGACY_DRAFT_KEY) ?? 'null'))
    if (!legacy) return emptyStoredDraftCollection()
    const projects = Object.create(null) as Record<string, StoredDraft>
    const activeProjectId = legacy.projectId ?? null
    const migrated: StoredDraftCollection = activeProjectId === null
      ? { version: 2, activeProjectId: null, unsaved: legacy, projects }
      : { version: 2, activeProjectId, projects: Object.assign(projects, { [activeProjectId]: legacy }) }
    writeStoredDraftCollection(storage, migrated)
    return migrated
  } catch {
    return emptyStoredDraftCollection()
  }
}

function normalizeStoredDraft(value: unknown, projectId?: string | null): StoredDraft | null {
  if (!value || typeof value !== 'object') return null
  const draft = value as StoredDraft & { export?: unknown }
  try {
    const resolvedProjectId = projectId === undefined
      ? typeof draft.projectId === 'string' && draft.projectId ? draft.projectId : null
      : projectId
    const normalized: StoredDraft = {
      document: normalizeComposition(draft.document),
      media: cloneValidCompositionMedia(draft.media),
      projectId: resolvedProjectId,
      projectRevision: typeof draft.projectRevision === 'number' && draft.projectRevision > 0
        ? Math.floor(draft.projectRevision)
        : null,
      projectName: typeof draft.projectName === 'string' && draft.projectName.trim()
        ? draft.projectName.slice(0, 256)
        : 'Новая композиция',
      exportSettings: normalizeCompositionRenderOutput(draft.exportSettings ?? draft.export),
      dirty: typeof draft.dirty === 'boolean' ? draft.dirty : resolvedProjectId !== null,
    }
    if (typeof draft.dirty !== 'boolean' && resolvedProjectId === null) {
      normalized.dirty = !isBlankStoredDraft(normalized)
    }
    return normalized
  } catch {
    return null
  }
}

function cloneValidCompositionMedia(value: unknown): Record<string, CompositionMedia> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return {}
  return Object.fromEntries(Object.entries(value).filter((entry): entry is [string, CompositionMedia] => (
    !!entry[1] && typeof entry[1] === 'object'
  )).map(([id, media]) => [id, { ...media }]))
}

function emptyStoredDraftCollection(): StoredDraftCollection {
  return { version: 2, activeProjectId: null, projects: Object.create(null) as Record<string, StoredDraft> }
}

export function createBlankStoredDraft(): StoredDraft {
  return {
    document: createComposition(DEFAULT_CANVAS),
    media: {},
    projectId: null,
    projectRevision: null,
    projectName: 'Новая композиция',
    exportSettings: DEFAULT_COMPOSITION_RENDER_OUTPUT,
    dirty: false,
  }
}

function isBlankStoredDraft(draft: StoredDraft): boolean {
  return draft.projectId === null &&
    (draft.projectName?.trim() || 'Новая композиция') === 'Новая композиция' &&
    Object.keys(draft.media ?? {}).length === 0 &&
    JSON.stringify(draft.document) === JSON.stringify(createComposition(DEFAULT_CANVAS)) &&
    JSON.stringify(normalizeCompositionRenderOutput(draft.exportSettings)) ===
      JSON.stringify(DEFAULT_COMPOSITION_RENDER_OUTPUT)
}

export function scheduleAutosave(): void {
  activeDraftDirty = true
  const storage = getStorage()
  if (!storage) return
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = setTimeout(() => {
    autosaveTimer = undefined
    persistCompositionDraft(storage)
  }, 300)
}

export function persistCompositionDraft(storage: CompositionStorage): void {
  const draft: StoredDraft = {
    document: cloneComposition(compositionState.document),
    media: cloneCompositionMedia(compositionState.media),
    projectId: compositionState.projectId,
    projectRevision: compositionState.projectRevision,
    projectName: compositionState.projectName,
    exportSettings: compositionRenderOutput(),
    dirty: activeDraftDirty,
  }
  const drafts = readStoredDraftCollection() ?? emptyStoredDraftCollection()
  if (compositionState.projectId === null) drafts.unsaved = draft
  else drafts.projects[compositionState.projectId] = draft
  drafts.activeProjectId = compositionState.projectId
  writeStoredDraftCollection(storage, drafts)
}

function writeStoredDraftCollection(storage: CompositionStorage, drafts: StoredDraftCollection): void {
  try {
    storage.setItem(DRAFTS_KEY, JSON.stringify(drafts))
  } catch {
    // A full storage quota should never make the editor itself unusable.
  }
}

export function persistCurrentDraftNow(clearUnsaved = false): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  const storage = getStorage()
  if (!storage) return
  const drafts = readStoredDraftCollection() ?? emptyStoredDraftCollection()
  if (clearUnsaved) delete drafts.unsaved
  const draft: StoredDraft = {
    document: cloneComposition(compositionState.document),
    media: cloneCompositionMedia(compositionState.media),
    projectId: compositionState.projectId,
    projectRevision: compositionState.projectRevision,
    projectName: compositionState.projectName,
    exportSettings: compositionRenderOutput(),
    dirty: activeDraftDirty,
  }
  if (compositionState.projectId === null) drafts.unsaved = draft
  else drafts.projects[compositionState.projectId] = draft
  drafts.activeProjectId = compositionState.projectId
  writeStoredDraftCollection(storage, drafts)
}

export function activateStoredDraft(draft: StoredDraft): void {
  const output = normalizeCompositionRenderOutput(draft.exportSettings)
  compositionState.projectId = draft.projectId ?? null
  compositionState.projectRevision = draft.projectRevision ?? null
  compositionState.projectName = draft.projectName?.trim() || 'Новая композиция'
  compositionState.media = cloneValidCompositionMedia(draft.media)
  compositionState.export.profile = { ...output.profile }
  compositionState.export.qualityTier = output.qualityTier
  compositionState.export.videoBitrateKbps = output.videoBitrateKbps ?? null
  compositionState.export.targetSizeBytes = output.targetSizeBytes ?? null
  compositionState.export.result = null
  compositionState.export.error = ''
  clearCompositionExportRange()
  replaceDocument(draft.document, false, false)
  activeDraftDirty = draft.dirty ?? true
}

export function removeStoredProjectDraft(id: string): void {
  const storage = getStorage()
  if (!storage) return
  const drafts = readStoredDraftCollection() ?? emptyStoredDraftCollection()
  delete drafts.projects[id]
  if (drafts.activeProjectId === id) drafts.activeProjectId = null
  writeStoredDraftCollection(storage, drafts)
}

export { DEFAULT_COMPOSITION_RENDER_OUTPUT } from '../../composition/payload.js'

export function setCompositionStorageForTests(
  storage: CompositionStorage | null | undefined,
): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  compositionStorageOverride = storage
}

export function resetCompositionForTests(): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  bumpOpenCompositionRevision()
  setProjectWriteBusy(false)
  setCompositionProjectsEtag(null)
  activateStoredDraft(createBlankStoredDraft())
  setActiveDraftDirty(false)
  compositionState.save.busy = false
  compositionState.save.error = ''
}

export function reloadCompositionDraftForTests(): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  bumpOpenCompositionRevision()
  activateStoredDraft(readActiveStoredDraft() ?? createBlankStoredDraft())
  compositionState.save.busy = false
  compositionState.save.error = ''
}

export function flushCompositionAutosaveForTests(): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  const storage = getStorage()
  if (storage) persistCompositionDraft(storage)
}
