import * as api from '../api'
import type { TimelineBeat } from '../audio/beatMarkers'
import { estimateWaveformOffset, MAX_SYNC_SAMPLES, waveformEnergy } from '../audio/sync'
import { localWaveformCache } from '../audio/waveformCache'
import type { WaveformSummary } from '../audio/waveform'
import {
  addClip,
  addTrack,
  collectSnapTargets,
  createComposition,
  deleteClip,
  deleteTransition,
  duplicateClip,
  findClipLocation,
  moveClip,
  registerSource,
  reorderTrack,
  snapClipStart,
  snapTick,
  splitClip,
  trimClip,
  upsertTransition,
} from '../composition/commands'
import {
  buildCompositionRenderRequest,
  DEFAULT_COMPOSITION_RENDER_OUTPUT,
  normalizeCompositionRenderOutput,
} from '../composition/payload'
import {
  MAX_COMPOSITION_MARKERS,
  compositionMarkers,
  deleteCompositionMarker,
  replaceCompositionMarkersByOrigin,
  upsertCompositionMarker,
  type CompositionMarker,
} from '../composition/markers'
import { magnetizeTrack, rippleDeleteClip } from '../composition/timelineProductivity'
import { relinkCompositionSource as relinkCompositionDocumentSource } from '../composition/relink'
import {
  compileMulticamCuts,
  MAX_MULTICAM_ANGLES,
  MIN_MULTICAM_ANGLES,
  recordMulticamSwitch,
  sourceTickAtGroupStartFromOffset,
  type MulticamAngle,
  type MulticamGroup,
} from '../composition/multicam'
import {
  clipDurationTicks,
  clipEndTicks,
  COMPOSITION_DELIVERY_PROFILE_OPTIONS,
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_DURATION_TICKS,
  type AudioClip,
  type AudioTrack,
  type Composition,
  type CompositionAudioProperty,
  type CompositionClip,
  type CompositionDeliveryProfile,
  type CompositionDeliveryProfileId,
  type CompositionQualityTier,
  type CompositionRenderOutput,
  type CompositionBlendMode,
  type CompositionChromaKey,
  type CompositionFrameInterpolation,
  type CompositionInterpolation,
  type CompositionMaskShape,
  type CompositionMaskProperty,
  type CompositionPlaybackMode,
  type CompositionSource,
  type CompositionSpeedRamp,
  type CompositionStabilization,
  type CompositionTrack,
  type CompositionTransition,
  type CompositionTransitionKind,
  type CompositionVideoMask,
  type CompositionVisualAnimation,
  type CompositionVisualProperty,
  type ImageClip,
  type ImageTrack,
  type TextClip,
  type TextTrack,
  type TrackKind,
  type VideoClip,
  type VideoTrack,
  type VisualClip,
  compositionDeliveryProfileOption,
} from '../composition/types'
import {
  audioPropertyBounds,
  audioPropertyFallback,
  cloneAnimatableValue,
  constantAnimatable,
  deleteKeyframe,
  keyframeTickAtLocalTime,
  maskPropertyBounds,
  sampleAnimatableValue,
  sliceAudioAnimation,
  sliceVisualAnimation,
  sliceVideoMasks,
  updateInterpolation,
  updateKeyframe,
  upsertKeyframe,
  visualPropertyBounds,
  visualPropertyFallback,
} from '../composition/keyframes'
import { videoSourceTickAtTimelineTick } from '../composition/playback'
import { cloneCompositionSpeedRamp, minimumCompositionSpeed } from '../composition/speedRamp'
import {
  assertValidComposition,
  compositionDurationTicks,
  compositionRenderUnavailableReason,
  compositionUsesFreezeFrame,
  compositionUsesOpticalFlow,
  compositionUsesReversePlayback,
  compositionUsesStabilization,
  compositionUsesSpeedRamp,
  normalizeComposition,
  primaryCompositionVideoTrack,
} from '../composition/validation'
import {
  cuesToTextClips,
  formatSrt,
  parseSrt,
  textClipsToCues,
} from '../subtitles/srt'
import type { Capabilities, MediaEntry, MediaInfo, MediaType, ResultInfo } from '../types'

const LEGACY_DRAFT_KEY = 'video-kadr:composition-draft:v1'
const DRAFTS_KEY = 'video-kadr:composition-drafts:v2'
const MODE_KEY = 'video-kadr:editor-mode:v1'
const DEFAULT_IMAGE_DURATION_TICKS = 5 * COMPOSITION_TIME_BASE
const DEFAULT_TEXT_DURATION_TICKS = 4 * COMPOSITION_TIME_BASE
const SNAP_THRESHOLD_PX = 8

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

type CompositionStorage = Pick<Storage, 'getItem' | 'setItem'>

const DEFAULT_CANVAS = {
  width: 1280,
  height: 720,
  fps: 30,
  backgroundColor: '#000000',
} as const

let compositionStorageOverride: CompositionStorage | null | undefined

const restored = readActiveStoredDraft()
const restoredOutput = normalizeCompositionRenderOutput(restored?.exportSettings)

export const editorMode = $state({ value: readStoredMode() })

export const compositionState = $state({
  document: cloneComposition(restored?.document ?? createComposition(DEFAULT_CANVAS)),
  media: { ...(restored?.media ?? {}) } as Record<string, CompositionMedia>,
  projectId: restored?.projectId ?? null as string | null,
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
  },
})

let autosaveTimer: ReturnType<typeof setTimeout> | undefined
let activeDraftDirty = restored?.dirty ?? false
let openCompositionRevision = 0
let projectWriteBusy = false
const UNSAVED_GUARD_MESSAGE = 'Сначала сохраните текущую композицию'
const PROJECT_BUSY_MESSAGE = 'Дождитесь завершения сохранения проекта'

export function setEditorMode(mode: EditorMode): void {
  editorMode.value = mode
  getStorage()?.setItem(MODE_KEY, mode)
  if (mode === 'legacy') compositionState.transport.playing = false
}

export function newComposition(): void {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    return
  }
  if (compositionState.projectId === null && activeDraftDirty) {
    persistCurrentDraftNow()
    compositionState.save.error = UNSAVED_GUARD_MESSAGE
    return
  }

  openCompositionRevision += 1
  persistCurrentDraftNow()
  const unsaved = readStoredDraftCollection()?.unsaved
  activateStoredDraft(unsaved ?? createBlankStoredDraft())
  compositionState.save.busy = false
  compositionState.save.error = ''
  persistCurrentDraftNow()
}

export function compositionRenderOutput(): CompositionRenderOutput {
  return {
    profile: { ...compositionState.export.profile },
    qualityTier: compositionState.export.qualityTier,
  }
}

export function updateCompositionExportSettings(
  patch: Partial<CompositionRenderOutput>,
): void {
  const profile = patch.profile ?? compositionState.export.profile
  const qualityTier = patch.qualityTier ?? compositionState.export.qualityTier
  compositionDeliveryProfileOption(profile)
  if (qualityTier !== 'high' && qualityTier !== 'medium' && qualityTier !== 'compact') {
    throw new Error('Неизвестный quality tier для composition export')
  }
  commitDocument(compositionState.document, compositionState.media, {
    profile: { ...profile },
    qualityTier,
  })
}

export function setCompositionDeliveryProfile(profileId: CompositionDeliveryProfileId): void {
  const option = COMPOSITION_DELIVERY_PROFILE_OPTIONS.find((candidate) => candidate.id === profileId)
  if (!option) throw new Error(`Неизвестный delivery profile ${profileId}`)
  updateCompositionExportSettings({ profile: option.profile })
}

export function setCompositionProjectName(name: string): void {
  compositionState.projectName = name.slice(0, 256)
  scheduleAutosave()
}

export function replaceCompositionDocument(document: Composition): void {
  replaceDocument(normalizeComposition(document), false)
}

export function openCompositionDocumentAsNew(document: Composition, name = 'Новая композиция'): void {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    throw new Error(PROJECT_BUSY_MESSAGE)
  }
  document = normalizeComposition(document)
  persistCurrentDraftNow()
  const storedUnsaved = readStoredDraftCollection()?.unsaved
  if (
    (compositionState.projectId === null && activeDraftDirty) ||
    (compositionState.projectId !== null && storedUnsaved?.dirty)
  ) {
    compositionState.save.error = UNSAVED_GUARD_MESSAGE
    throw new Error(UNSAVED_GUARD_MESSAGE)
  }

  openCompositionRevision += 1
  activateStoredDraft({
    document,
    media: {},
    projectId: null,
    projectName: name.trim().slice(0, 256) || 'Новая композиция',
    exportSettings: DEFAULT_COMPOSITION_RENDER_OUTPUT,
    dirty: true,
  })
  persistCurrentDraftNow()
}

export function selectCompositionClip(trackId: string | null, clipId: string | null): void {
  compositionState.ui.selectedTrackId = trackId
  compositionState.ui.selectedClipId = clipId
}

export function selectedCompositionClip(): CompositionClip | null {
  const id = compositionState.ui.selectedClipId
  if (!id) return null
  try {
    return findClipLocation(compositionState.document, id).clip
  } catch {
    return null
  }
}

export function selectedCompositionTrack(): CompositionTrack | null {
  const id = compositionState.ui.selectedTrackId
  return compositionState.document.tracks.find((track) => track.id === id) ?? null
}

export function compositionDuration(): number {
  return compositionDurationTicks(compositionState.document)
}

export function setCompositionPlayhead(ticks: number): void {
  const duration = compositionDuration()
  compositionState.transport.playheadTicks = clampTick(ticks, Math.max(duration, 0))
  if (duration > 0 && compositionState.transport.playheadTicks >= duration) {
    compositionState.transport.playing = false
  }
}

export function compositionMarkerList(): readonly CompositionMarker[] {
  return compositionMarkers(compositionState.document)
}

export function addCompositionMarkerAtPlayhead(label?: string): string {
  const markers = compositionMarkerList()
  const id = makeId('marker')
  const palette = ['#f59e0b', '#22c55e', '#38bdf8', '#a78bfa', '#fb7185'] as const
  const marker: CompositionMarker = {
    id,
    tick: compositionState.transport.playheadTicks,
    label: label?.trim() || `Маркер ${markers.length + 1}`,
    color: palette[markers.length % palette.length],
  }
  commitDocument(upsertCompositionMarker(compositionState.document, marker))
  compositionState.ui.selectedMarkerId = id
  return id
}

/** Replace the generated Auto Beat marker family as one history/autosave edit. */
export function replaceCompositionAutoBeatMarkers(
  beats: readonly TimelineBeat[],
  estimatedBpm: number | null,
): number {
  if (estimatedBpm !== null && (!Number.isFinite(estimatedBpm) || estimatedBpm <= 0 || estimatedBpm > 1_000)) {
    throw new Error('Auto Beat BPM должен быть положительным числом')
  }
  const uniqueByTick: TimelineBeat[] = []
  for (const beat of beats) {
    if (!Number.isSafeInteger(beat.tick) || beat.tick < 0 || beat.tick > MAX_COMPOSITION_DURATION_TICKS) {
      throw new Error('Auto Beat marker tick выходит за пределы композиции')
    }
    if (!Number.isFinite(beat.strength) || beat.strength < 0) {
      throw new Error('Auto Beat marker strength должна быть неотрицательным числом')
    }
    const existingIndex = uniqueByTick.findIndex((candidate) => candidate.tick === beat.tick)
    if (existingIndex === -1) uniqueByTick.push({ ...beat })
    else if (beat.strength > uniqueByTick[existingIndex]!.strength) uniqueByTick[existingIndex] = { ...beat }
  }
  const normalized = uniqueByTick.sort((left, right) => left.tick - right.tick)
  const manualIds = compositionMarkerList()
    .filter((marker) => marker.origin !== 'auto_beat')
    .map((marker) => marker.id)
  if (manualIds.length + normalized.length > MAX_COMPOSITION_MARKERS) {
    throw new Error(`Marker limit is ${MAX_COMPOSITION_MARKERS}`)
  }
  const bpmLabel = estimatedBpm === null ? '' : ` · ≈ ${Number(estimatedBpm.toFixed(1))} BPM`
  const markers = normalized.map((beat, index): CompositionMarker => ({
    id: autoBeatMarkerId(beat.tick, manualIds),
    tick: beat.tick,
    label: `Auto Beat ${index + 1}${bpmLabel}`,
    color: '#f97316',
    origin: 'auto_beat',
  }))
  commitDocument(replaceCompositionMarkersByOrigin(compositionState.document, 'auto_beat', markers))
  return markers.length
}

export function updateCompositionMarker(
  markerId: string,
  patch: Partial<{ tick: number; label: string; color: string | null }>,
): void {
  const marker = compositionMarkerList().find((candidate) => candidate.id === markerId)
  if (!marker) throw new Error(`Marker ${markerId} не найден`)
  const color = patch.color === null ? undefined : patch.color ?? marker.color
  commitDocument(upsertCompositionMarker(compositionState.document, {
    ...marker,
    tick: patch.tick === undefined ? marker.tick : Math.round(patch.tick),
    label: patch.label ?? marker.label,
    ...(color === undefined ? { color: undefined } : { color }),
  }))
  compositionState.ui.selectedMarkerId = markerId
}

export function removeCompositionMarker(markerId: string): void {
  commitDocument(deleteCompositionMarker(compositionState.document, markerId))
  if (compositionState.ui.selectedMarkerId === markerId) compositionState.ui.selectedMarkerId = null
}

export function seekCompositionMarker(markerId: string): void {
  const marker = compositionMarkerList().find((candidate) => candidate.id === markerId)
  if (!marker) throw new Error(`Marker ${markerId} не найден`)
  compositionState.transport.playheadTicks = marker.tick
  compositionState.transport.playing = false
  compositionState.ui.selectedMarkerId = markerId
}

export interface CompositionMulticamAngleInput {
  readonly sourceId: string
  readonly label: string
  readonly sourceTickAtGroupStart: number
}

export interface CompositionMulticamSyncEstimate {
  readonly sourceId: string
  readonly sourceTickAtGroupStart: number
  readonly candidateStartOffsetSeconds: number
  readonly correlation: number
  readonly confidence: number
}

export interface MulticamWaveformCachePort {
  load(url: string): Promise<WaveformSummary>
}

export interface CreateCompositionMulticamOptions {
  readonly name: string
  readonly angles: readonly CompositionMulticamAngleInput[]
  readonly audioSourceId: string
  readonly timelineStartTicks?: number
  readonly durationTicks?: number
}

export function compositionMulticamGroups(): readonly MulticamGroup[] {
  return compositionState.document.multicamGroups ?? []
}

export async function estimateCompositionMulticamSync(
  sourceIds: readonly string[],
  cache: MulticamWaveformCachePort = localWaveformCache,
): Promise<readonly CompositionMulticamSyncEstimate[]> {
  const uniqueSourceIds = sourceIds.filter((sourceId, index) => sourceIds.indexOf(sourceId) === index)
  if (uniqueSourceIds.length < MIN_MULTICAM_ANGLES || uniqueSourceIds.length > MAX_MULTICAM_ANGLES) {
    throw new Error(`Для waveform sync выберите ${MIN_MULTICAM_ANGLES}–${MAX_MULTICAM_ANGLES} video sources`)
  }
  const inputs = uniqueSourceIds.map((sourceId) => {
    const source = compositionState.document.sources[sourceId]
    const media = compositionState.media[sourceId]
    if (!source || source.kind !== 'video') throw new Error(`Multicam source ${sourceId} не является video`)
    if (!source.hasAudio) throw new Error(`У angle ${sourceId} нет audio для waveform sync`)
    if (!media?.url) throw new Error(`Локальное медиа для angle ${sourceId} недоступно`)
    return { sourceId, media }
  })
  const summaries = await Promise.all(inputs.map(({ media }) => cache.load(media.url)))
  const maximumDuration = Math.max(...summaries.map((summary) => summary.durationSeconds))
  const minimumDuration = Math.min(...summaries.map((summary) => summary.durationSeconds))
  if (!Number.isFinite(maximumDuration) || !Number.isFinite(minimumDuration) || minimumDuration <= 0) {
    throw new Error('Waveform decoder вернул некорректную длительность')
  }
  const sampleRateHz = Math.max(1, Math.min(200, Math.floor(MAX_SYNC_SAMPLES / maximumDuration)))
  const envelopes = summaries.map((summary) => resampleWaveformEnergy(summary, sampleRateHz))
  const maxOffsetSeconds = Math.min(30, Math.max(0.1, minimumDuration / 2))
  const minOverlapSeconds = Math.min(2, Math.max(0.1, minimumDuration / 2))
  const offsets = inputs.map(({ sourceId }, index) => {
    if (index === 0) return { sourceId, candidateStartOffsetSeconds: 0, correlation: 1, confidence: 1 }
    const estimate = estimateWaveformOffset(envelopes[0]!, envelopes[index]!, {
      sampleRateHz,
      maxOffsetSeconds,
      minOverlapSeconds,
    })
    if (!estimate) throw new Error(`Waveform sync не нашёл надёжное совпадение для ${sourceId}`)
    return { sourceId, ...estimate }
  })
  const groupStartShiftSeconds = Math.max(0, ...offsets.map((offset) => offset.candidateStartOffsetSeconds))
  return offsets.map((offset) => ({
    ...offset,
    sourceTickAtGroupStart: sourceTickAtGroupStartFromOffset(
      offset.candidateStartOffsetSeconds,
      COMPOSITION_TIME_BASE,
      groupStartShiftSeconds,
    ),
  }))
}

export function createCompositionMulticamGroup(options: CreateCompositionMulticamOptions): string {
  if (options.angles.length < MIN_MULTICAM_ANGLES || options.angles.length > MAX_MULTICAM_ANGLES) {
    throw new Error(`Multicam требует ${MIN_MULTICAM_ANGLES}–${MAX_MULTICAM_ANGLES} angles`)
  }
  const sourceIds: string[] = []
  const normalizedInputs = options.angles.map((angle) => {
    const source = compositionState.document.sources[angle.sourceId]
    if (!source || source.kind !== 'video') throw new Error(`Multicam source ${angle.sourceId} не является video`)
    if (sourceIds.includes(source.id)) throw new Error('Multicam angles должны использовать разные sources')
    sourceIds.push(source.id)
    const label = angle.label.trim().slice(0, 128)
    if (!label) throw new Error(`Angle ${angle.sourceId} требует label`)
    if (!Number.isSafeInteger(angle.sourceTickAtGroupStart) || angle.sourceTickAtGroupStart < 0) {
      throw new Error(`Angle ${angle.sourceId} имеет некорректный offset`)
    }
    return { source, label, sourceTickAtGroupStart: angle.sourceTickAtGroupStart }
  })
  const audioInputIndex = normalizedInputs.findIndex((angle) => angle.source.id === options.audioSourceId)
  if (audioInputIndex < 0 || !normalizedInputs[audioInputIndex]!.source.hasAudio) {
    throw new Error('Выберите master audio angle с доступным звуком')
  }
  const timelineStartTicks = options.timelineStartTicks ?? compositionState.transport.playheadTicks
  if (!Number.isSafeInteger(timelineStartTicks) || timelineStartTicks < 0 || timelineStartTicks >= MAX_COMPOSITION_DURATION_TICKS) {
    throw new Error('Некорректное начало multicam group')
  }
  const commonCoverage = Math.min(...normalizedInputs.map(
    (angle) => angle.source.durationTicks - angle.sourceTickAtGroupStart,
  ))
  const durationTicks = Math.min(
    options.durationTicks ?? commonCoverage,
    commonCoverage,
    MAX_COMPOSITION_DURATION_TICKS - timelineStartTicks,
  )
  if (!Number.isSafeInteger(durationTicks) || durationTicks <= 0) {
    throw new Error('У выбранных angles нет общей длительности после offsets')
  }

  let document = compositionState.document
  const normalizedName = options.name.trim().slice(0, 128) || `Multicam ${compositionMulticamGroups().length + 1}`
  const groupId = makeId('multicam')
  const videoTrackId = makeId('multicam-video-track')
  const audioTrackId = makeId('multicam-audio-track')
  const audioClipId = makeId('multicam-audio-clip')
  document = addTrack(document, {
    id: videoTrackId,
    kind: 'video',
    name: `${normalizedName} · Program`.slice(0, 128),
    locked: false,
    hidden: false,
    muted: true,
    transitions: [],
    clips: [],
  })
  document = addTrack(document, {
    id: audioTrackId,
    kind: 'audio',
    name: `${normalizedName} · Master audio`.slice(0, 128),
    locked: false,
    muted: false,
    solo: false,
    clips: [],
  })
  const angles: MulticamAngle[] = normalizedInputs.map((angle) => ({
    id: makeId('multicam-angle'),
    label: angle.label,
    sourceId: angle.source.id,
    sourceTickAtGroupStart: angle.sourceTickAtGroupStart,
  }))
  const group: MulticamGroup = {
    id: groupId,
    name: normalizedName,
    timelineStartTicks,
    durationTicks,
    videoTrackId,
    audioTrackId,
    audioClipId,
    audioAngleId: angles[audioInputIndex]!.id,
    angles,
    switches: [{
      id: makeId('multicam-switch'),
      clipId: makeId('multicam-cut'),
      timelineTick: timelineStartTicks,
      angleId: angles[0]!.id,
    }],
  }
  document = rebuildMulticamDocument(
    { ...document, multicamGroups: [...(document.multicamGroups ?? []), group] },
    group,
  )
  commitDocument(document)
  compositionState.ui.selectedMulticamGroupId = groupId
  selectCompositionClip(videoTrackId, group.switches[0]!.clipId)
  return groupId
}

export function rebuildCompositionMulticamGroup(groupId: string): void {
  const group = compositionMulticamGroups().find((candidate) => candidate.id === groupId)
  if (!group) throw new Error(`Multicam group ${groupId} не найдена`)
  commitDocument(rebuildMulticamDocument(compositionState.document, group, group))
  compositionState.ui.selectedMulticamGroupId = groupId
}

export function recordCompositionMulticamSwitch(groupId: string, angleId: string, timelineTick = compositionState.transport.playheadTicks): string {
  const previous = compositionMulticamGroups().find((candidate) => candidate.id === groupId)
  if (!previous) throw new Error(`Multicam group ${groupId} не найдена`)
  const change = {
    id: makeId('multicam-switch'),
    clipId: makeId('multicam-cut'),
    timelineTick: Math.round(timelineTick),
    angleId,
  }
  const next = recordMulticamSwitch(previous, change)
  const groups = compositionMulticamGroups().map((candidate) => candidate.id === groupId ? next : candidate)
  const document = rebuildMulticamDocument({ ...compositionState.document, multicamGroups: groups }, next, previous)
  commitDocument(document)
  compositionState.ui.selectedMulticamGroupId = groupId
  selectCompositionClip(next.videoTrackId, change.clipId)
  return change.clipId
}

export function seekComposition(seconds: number): void {
  setCompositionPlayhead(compositionState.transport.playheadTicks + seconds * COMPOSITION_TIME_BASE)
}

export function toggleCompositionPlayback(): void {
  const duration = compositionDuration()
  if (!duration) return
  if (!compositionState.transport.playing && compositionState.transport.playheadTicks >= duration) {
    compositionState.transport.playheadTicks = 0
  }
  compositionState.transport.playing = !compositionState.transport.playing
}

export function setCompositionZoom(pxPerSecond: number): void {
  compositionState.ui.zoomPxPerSecond = Math.max(24, Math.min(320, Math.round(pxPerSecond)))
}

export function toggleCompositionSnapping(): void {
  compositionState.ui.snapEnabled = !compositionState.ui.snapEnabled
}

export function undoComposition(): void {
  const previous = compositionState.history.past.pop()
  if (!previous) return
  const previousMedia = compositionState.history.pastMedia.pop() ?? cloneCompositionMedia(compositionState.media)
  const previousOutput = compositionState.history.pastOutput.pop() ?? compositionRenderOutput()
  compositionState.history.future.push(cloneComposition(compositionState.document))
  compositionState.history.futureMedia.push(cloneCompositionMedia(compositionState.media))
  compositionState.history.futureOutput.push(compositionRenderOutput())
  compositionState.document = cloneComposition(previous)
  compositionState.media = previousMedia
  compositionState.export.profile = { ...previousOutput.profile }
  compositionState.export.qualityTier = previousOutput.qualityTier
  repairSelection()
  stopAtDuration()
  scheduleAutosave()
}

export function redoComposition(): void {
  const next = compositionState.history.future.pop()
  if (!next) return
  const nextMedia = compositionState.history.futureMedia.pop() ?? cloneCompositionMedia(compositionState.media)
  const nextOutput = compositionState.history.futureOutput.pop() ?? compositionRenderOutput()
  compositionState.history.past.push(cloneComposition(compositionState.document))
  compositionState.history.pastMedia.push(cloneCompositionMedia(compositionState.media))
  compositionState.history.pastOutput.push(compositionRenderOutput())
  compositionState.document = cloneComposition(next)
  compositionState.media = nextMedia
  compositionState.export.profile = { ...nextOutput.profile }
  compositionState.export.qualityTier = nextOutput.qualityTier
  repairSelection()
  stopAtDuration()
  scheduleAutosave()
}

export function addMediaInfoToComposition(media: MediaInfo): string {
  const kind = inferMediaType(media)
  const source = compositionSourceFromMediaInfo(media, kind)
  let document = compositionState.document
  if (!Object.hasOwn(document.sources, source.id)) document = registerSource(document, source)
  rememberMedia(media, kind)

  let clipId: string
  if (kind === 'video') {
    const hasVideo = document.tracks.some(
      (track) => track.kind === 'video' && track.clips.length > 0,
    )
    if (!hasVideo) {
      document = {
        ...document,
        canvas: {
          ...document.canvas,
          width: evenDimension(media.width, DEFAULT_CANVAS.width, 3840),
          height: evenDimension(media.height, DEFAULT_CANVAS.height, 2160),
          fps: validFps(media.fps),
        },
      }
      assertValidComposition(document)
    }
    const result = addVideo(document, source)
    document = result.document
    clipId = result.clipId
  } else if (kind === 'audio') {
    const result = addAudio(document, source)
    document = result.document
    clipId = result.clipId
  } else {
    const result = addImage(document, source)
    document = result.document
    clipId = result.clipId
  }

  commitDocument(document)
  const location = findClipLocation(compositionState.document, clipId)
  selectCompositionClip(location.track.id, clipId)
  setEditorMode('composition')
  return clipId
}

/**
 * Atomically publish a completed microphone recording as an audio source and
 * place its clip at the current playhead. A malformed upload response cannot
 * create a video/image clip or leave a half-registered media binding behind.
 */
export function addVoiceoverMediaInfoToComposition(media: MediaInfo): string {
  if (media.mediaType !== 'audio') {
    throw new Error('Загруженная голосовая запись не распознана как аудио')
  }
  const uploadedSource = compositionSourceFromMediaInfo(media, 'audio')
  const existingSource = compositionState.document.sources[uploadedSource.id]
  if (existingSource && existingSource.kind !== 'audio') {
    throw new Error('Идентификатор голосовой записи уже занят другим типом медиа')
  }
  const source = existingSource ?? uploadedSource
  let document = compositionState.document
  if (!existingSource) document = registerSource(document, source)
  const result = addAudio(document, source)
  const mediaBinding: CompositionMedia = {
    id: source.id,
    url: media.url,
    filename: media.filename,
    mediaType: 'audio',
    duration: source.durationTicks / COMPOSITION_TIME_BASE,
    width: 0,
    height: 0,
    fps: media.fps ?? source.fps,
    vcodec: media.vcodec ?? source.vcodec,
    acodec: media.acodec ?? source.acodec,
  }
  commitDocument(result.document, { ...compositionState.media, [source.id]: mediaBinding })
  const location = findClipLocation(compositionState.document, result.clipId)
  selectCompositionClip(location.track.id, result.clipId)
  setEditorMode('composition')
  return result.clipId
}

export function addLibraryEntryToComposition(entry: MediaEntry): string {
  if (entry.kind !== 'source') throw new Error('В композицию можно добавить только исходный медиафайл')
  const mediaType = inferMediaType(entry)
  return addMediaInfoToComposition({
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    mediaType,
    duration: mediaType === 'image' ? 0 : positiveNumber(entry.duration),
    width: mediaType === 'audio' ? 0 : positiveInteger(entry.width),
    height: mediaType === 'audio' ? 0 : positiveInteger(entry.height),
    title: entry.title,
    fps: entry.fps,
    vcodec: entry.vcodec,
    acodec: entry.acodec,
  })
}

export function syncCompositionLibrary(entries: readonly MediaEntry[], authoritative = false): void {
  let changed = false
  const media = { ...compositionState.media }
  const availableSourceIds: string[] = []
  for (const entry of entries) {
    if (entry.kind !== 'source') continue
    if (!availableSourceIds.includes(entry.id)) availableSourceIds.push(entry.id)
    if (!Object.hasOwn(compositionState.document.sources, entry.id)) continue
    const source = compositionState.document.sources[entry.id]!
    const next: CompositionMedia = {
      id: entry.id,
      url: entry.url,
      filename: entry.filename,
      mediaType: source.kind,
      duration: source.durationTicks / COMPOSITION_TIME_BASE,
      width: source.width,
      height: source.height,
      fps: entry.fps ?? source.fps,
      vcodec: entry.vcodec ?? source.vcodec,
      acodec: entry.acodec ?? source.acodec,
    }
    if (JSON.stringify(media[entry.id]) !== JSON.stringify(next)) {
      media[entry.id] = next
      changed = true
    }
  }
  if (authoritative) {
    for (const id of Object.keys(media)) {
      if (Object.hasOwn(compositionState.document.sources, id) && !availableSourceIds.includes(id)) {
        delete media[id]
        changed = true
      }
    }
  }
  if (changed) {
    compositionState.media = media
    scheduleAutosave()
  }
}

/** Atomically replace a missing source id and its local media binding. */
export function relinkCompositionSource(
  sourceId: string,
  replacement: MediaEntry | CompositionSource,
): void {
  let entry: MediaEntry | null = null
  let replacementSource: CompositionSource
  if (isMediaEntry(replacement)) {
    if (replacement.kind !== 'source') throw new Error('Для relink нужен исходный файл из медиатеки')
    entry = replacement
    replacementSource = compositionSourceFromLibraryEntry(replacement)
  } else {
    replacementSource = { ...replacement }
  }
  const document = relinkCompositionDocumentSource(
    compositionState.document,
    sourceId,
    replacementSource,
  )

  const media = cloneCompositionMedia(compositionState.media)
  const oldMedia = media[sourceId]
  const registeredMedia = media[replacementSource.id]
  const url = entry?.url ?? registeredMedia?.url ?? oldMedia?.url
  const filename = entry?.filename ?? registeredMedia?.filename ?? oldMedia?.filename
  if (sourceId !== replacementSource.id) delete media[sourceId]
  if (url && filename) {
    media[replacementSource.id] = {
      id: replacementSource.id,
      url,
      filename,
      mediaType: replacementSource.kind,
      duration: replacementSource.durationTicks / COMPOSITION_TIME_BASE,
      width: replacementSource.width,
      height: replacementSource.height,
      fps: replacementSource.fps === undefined ? registeredMedia?.fps ?? oldMedia?.fps : replacementSource.fps,
      vcodec: replacementSource.vcodec === undefined ? registeredMedia?.vcodec ?? oldMedia?.vcodec : replacementSource.vcodec,
      acodec: replacementSource.acodec === undefined ? registeredMedia?.acodec ?? oldMedia?.acodec : replacementSource.acodec,
    }
  }
  commitDocument(document, media)
}

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
  const cues = parseSrt(serialized)
  if (!cues.length) throw new Error('SRT не содержит субтитров')
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

export type CompositionAutomationTarget =
  | { readonly kind: 'visual'; readonly clipId: string }
  | { readonly kind: 'audio'; readonly clipId: string }
  | { readonly kind: 'mask'; readonly clipId: string; readonly maskId: string }
export type CompositionAutomationProperty = CompositionVisualProperty | CompositionAudioProperty | CompositionMaskProperty

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

/** Apply a completed point track as one undoable X/Y animation edit. */
export function applyCompositionTrackedAnimation(
  clipId: string,
  animation: CompositionVisualAnimation,
): void {
  requireVisualOverlay(clipId)
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
  shape: Exclude<CompositionMaskShape, 'linear'>,
): string {
  const location = requireVisualOverlay(clipId)
  if (location.clip.kind !== 'video') throw new Error('Masks доступны только для video overlay')
  const id = makeId('mask')
  const mask: CompositionVideoMask = {
    id,
    shape,
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
    shape: Exclude<CompositionMaskShape, 'linear'>
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
  patch: Partial<{ pan: number; fadeInTicks: number; fadeOutTicks: number }>,
): void {
  updateClip(clipId, (clip) => {
    if (clip.kind !== 'audio') return clip
    const duration = clipDurationTicks(clip)
    return {
      ...clip,
      pan: patch.pan === undefined ? clip.pan ?? 0 : clampNumber(patch.pan, -1, 1),
      fadeInTicks: patch.fadeInTicks === undefined ? clip.fadeInTicks ?? 0 : clampTick(patch.fadeInTicks, duration),
      fadeOutTicks: patch.fadeOutTicks === undefined ? clip.fadeOutTicks ?? 0 : clampTick(patch.fadeOutTicks, duration),
    }
  })
}

export function updateCompositionTextStyle(clipId: string, patch: Partial<TextClip['style']>): void {
  updateClip(clipId, (clip) => clip.kind === 'text' ? { ...clip, style: { ...clip.style, ...patch } } : clip)
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

export function getCompositionExportUnavailableReason(capabilities: Capabilities | null): string | null {
  if (!capabilities) return 'Проверяем поддержку composition-v1 на сервере…'
  const feature = capabilities.features?.find((candidate) => candidate.id === 'composition-v1')
  if (!feature) return 'Сервер не объявил поддержку composition-v1.'
  if (!feature.available) return feature.reason?.trim() || 'Composition export недоступен на этом сервере.'
  const renderReason = compositionRenderUnavailableReason(compositionState.document)
  if (renderReason) return renderReason
  const delivery = compositionDeliveryProfileOption(compositionState.export.profile)
  if (delivery.capabilityId) {
    const deliveryCapability = capabilities.features?.find((candidate) => candidate.id === delivery.capabilityId)
    if (!deliveryCapability) {
      return `Сервер не объявил поддержку ${delivery.capabilityId} для ${delivery.label}.`
    }
    if (!deliveryCapability.available) {
      return deliveryCapability.reason?.trim() || `${delivery.label} недоступен на этом сервере.`
    }
  }
  if (compositionUsesOpticalFlow(compositionState.document)) {
    const opticalFlow = capabilities.features?.find((candidate) => candidate.id === 'optical-flow')
    if (!opticalFlow) return 'Сервер не объявил поддержку optical-flow для этого composition request.'
    if (!opticalFlow.available) return opticalFlow.reason?.trim() || 'Optical flow недоступен на этом сервере.'
  }
  if (compositionUsesReversePlayback(compositionState.document)) {
    const reversePlayback = capabilities.features?.find((candidate) => candidate.id === 'reverse-playback')
    if (!reversePlayback) return 'Сервер не объявил поддержку reverse-playback для этого composition request.'
    if (!reversePlayback.available) return reversePlayback.reason?.trim() || 'Reverse playback недоступен на этом сервере.'
  }
  if (compositionUsesFreezeFrame(compositionState.document)) {
    const freezeFrame = capabilities.features?.find((candidate) => candidate.id === 'freeze-frame')
    if (!freezeFrame) return 'Сервер не объявил поддержку freeze-frame для этого composition request.'
    if (!freezeFrame.available) return freezeFrame.reason?.trim() || 'Freeze frame недоступен на этом сервере.'
  }
  if (compositionUsesStabilization(compositionState.document)) {
    const stabilization = capabilities.features?.find((candidate) => candidate.id === 'stabilization')
    if (!stabilization) return 'Сервер не объявил поддержку stabilization для этого composition request.'
    if (!stabilization.available) return stabilization.reason?.trim() || 'Stabilization недоступна на этом сервере.'
  }
  if (compositionUsesSpeedRamp(compositionState.document)) {
    const speedRamp = capabilities.features?.find((candidate) => candidate.id === 'speed-ramp')
    if (!speedRamp) return 'Сервер не объявил поддержку speed-ramp для этого composition request.'
    if (!speedRamp.available) return speedRamp.reason?.trim() || 'Speed ramp недоступна на этом сервере.'
  }
  return null
}

export async function exportComposition(capabilities: Capabilities | null): Promise<void> {
  if (compositionState.export.running) return
  const reason = getCompositionExportUnavailableReason(capabilities)
  if (reason) {
    compositionState.export.error = reason
    return
  }
  compositionState.export.running = true
  compositionState.export.error = ''
  compositionState.export.result = null
  compositionState.export.progress = null
  compositionState.export.stage = 'queued'
  try {
    const request = buildCompositionRenderRequest(compositionState.document, compositionRenderOutput())
    const { jobId } = await api.renderComposition(request)
    compositionState.export.jobId = jobId
    const job = await api.pollJob(jobId, (current) => {
      compositionState.export.progress = typeof current.progress === 'number' ? current.progress : null
      compositionState.export.stage = current.stage ?? null
    })
    compositionState.export.result = job.result as ResultInfo
  } catch (error) {
    compositionState.export.error =
      error instanceof Error && error.message === 'cancelled'
        ? 'Экспорт отменён'
        : error instanceof Error
          ? error.message
          : String(error)
  } finally {
    compositionState.export.running = false
    compositionState.export.jobId = null
    compositionState.export.progress = null
    compositionState.export.stage = null
  }
}

export async function cancelCompositionExport(): Promise<void> {
  if (compositionState.export.jobId) await api.cancelJob(compositionState.export.jobId)
}

export async function loadCompositionProjects(): Promise<void> {
  try {
    compositionState.projects = await api.getCompositionProjects()
    compositionState.save.error = ''
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  }
}

export async function saveCompositionProject(): Promise<void> {
  if (compositionState.save.busy) return
  persistCurrentDraftNow()
  projectWriteBusy = true
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    const body = { name: compositionState.projectName.trim() || undefined, document: cloneComposition(compositionState.document) }
    const project = compositionState.projectId
      ? await api.updateCompositionProject(compositionState.projectId, body)
      : await api.createCompositionProject(body)
    const wasUnsaved = compositionState.projectId === null
    compositionState.projectId = project.id
    compositionState.projectName = project.name
    compositionState.projects = [project, ...compositionState.projects.filter((item) => item.id !== project.id)]
    activeDraftDirty = false
    persistCurrentDraftNow(wasUnsaved)
    compositionState.save.error = ''
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    projectWriteBusy = false
    compositionState.save.busy = false
  }
}

export async function openCompositionProject(id: string): Promise<void> {
  if (projectWriteBusy) {
    compositionState.save.error = PROJECT_BUSY_MESSAGE
    return
  }
  persistCurrentDraftNow()
  const revision = ++openCompositionRevision
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    const project = await api.getCompositionProject(id)
    if (revision !== openCompositionRevision) return
    if (!project) throw new Error('Композиционный проект не найден')
    const local = readStoredDraftCollection()?.projects[project.id]
    activateStoredDraft(local?.dirty ? local : {
      document: normalizeComposition(project.document),
      media: local?.media ?? {},
      projectId: project.id,
      projectName: project.name,
      exportSettings: local?.exportSettings ?? DEFAULT_COMPOSITION_RENDER_OUTPUT,
      dirty: false,
    })
    persistCurrentDraftNow()
  } catch (error) {
    if (revision !== openCompositionRevision) return
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    if (revision === openCompositionRevision) compositionState.save.busy = false
  }
}

export async function deleteCompositionProject(id: string): Promise<void> {
  if (compositionState.save.busy) return
  projectWriteBusy = true
  compositionState.save.busy = true
  compositionState.save.error = ''
  try {
    await api.deleteCompositionProject(id)
    compositionState.projects = compositionState.projects.filter((project) => project.id !== id)
    removeStoredProjectDraft(id)
    if (compositionState.projectId === id) {
      openCompositionRevision += 1
      const unsaved = readStoredDraftCollection()?.unsaved
      activateStoredDraft(unsaved ?? createBlankStoredDraft())
      persistCurrentDraftNow()
    }
  } catch (error) {
    compositionState.save.error = error instanceof Error ? error.message : String(error)
  } finally {
    projectWriteBusy = false
    compositionState.save.busy = false
  }
}

function addVideo(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
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

function addAudio(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
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
    fadeInTicks: 0,
    fadeOutTicks: 0,
  }
  return { document: addClip(next, track.id, clip), clipId }
}

function addImage(document: Composition, source: CompositionSource): { document: Composition; clipId: string } {
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

function rebuildMulticamDocument(
  document: Composition,
  group: MulticamGroup,
  previousGroup?: MulticamGroup,
): Composition {
  const compiled = compileMulticamCuts(document, group)
  const previousVideoIds = new Set(previousGroup?.switches.map((change) => change.clipId) ?? [])
  const currentVideoIds = new Set(group.switches.map((change) => change.clipId))
  const ownedVideoIds = new Set([...previousVideoIds, ...currentVideoIds])
  const videoTrack = document.tracks.find((track) => track.id === group.videoTrackId)
  if (!videoTrack || videoTrack.kind !== 'video') throw new Error('Multicam program video track недоступна')
  for (const clip of videoTrack.clips) {
    if (currentVideoIds.has(clip.id) && !previousVideoIds.has(clip.id) && previousGroup) {
      throw new Error(`Multicam output id ${clip.id} конфликтует с чужим clip`)
    }
  }

  const previousAudioIds = new Set(previousGroup?.audioClipId ? [previousGroup.audioClipId] : [])
  const currentAudioIds = new Set(group.audioClipId ? [group.audioClipId] : [])
  const ownedAudioIds = new Set([...previousAudioIds, ...currentAudioIds])
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

function resampleWaveformEnergy(summary: WaveformSummary, sampleRateHz: number): Float32Array {
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

function updateClip(clipId: string, update: (clip: CompositionClip) => CompositionClip): void {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.track.locked) throw new Error('Дорожка заблокирована')
  const tracks = compositionState.document.tracks.map((track) => {
    if (track.id !== location.track.id) return track
    return withTrackClips(track, track.clips.map((clip) => clip.id === clipId ? update(clip) : clip))
  })
  commitDocument({ ...compositionState.document, tracks })
}

function requireVisualOverlay(clipId: string): { readonly clip: VisualClip; readonly track: CompositionTrack } {
  const location = findClipLocation(compositionState.document, clipId)
  if (location.clip.kind === 'audio') throw new Error('Keyframes доступны только для visual clips')
  const primary = primaryCompositionVideoTrack(compositionState.document)
  if (location.track.kind === 'video' && location.track.id === primary?.id) {
    throw new Error('Основная видеодорожка должна оставаться neutral; используйте overlay')
  }
  return { clip: location.clip, track: location.track }
}

function requireAudioAutomationClip(
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

function updateAudioAnimationValue(
  clipId: string,
  property: CompositionAudioProperty,
  value: import('../composition/types').CompositionAnimatableValue | undefined,
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

function requireVideoMask(
  clipId: string,
  maskId: string,
): { readonly clip: VideoClip; readonly mask: CompositionVideoMask } {
  const location = requireVisualOverlay(clipId)
  if (location.clip.kind !== 'video') throw new Error('Masks доступны только для video overlay')
  const mask = location.clip.masks?.find((candidate) => candidate.id === maskId)
  if (!mask) throw new Error('Mask не найдена')
  if (mask.shape === 'linear') throw new Error('Linear mask не поддерживает authoring keyframes')
  return { clip: location.clip, mask }
}

function updateMaskAnimationValue(
  clipId: string,
  maskId: string,
  property: CompositionMaskProperty,
  value: import('../composition/types').CompositionAnimatableValue,
): void {
  requireVideoMask(clipId, maskId)
  updateClip(clipId, (clip) => clip.kind === 'video'
    ? {
        ...clip,
        masks: (clip.masks ?? []).map((mask) => mask.id === maskId ? { ...mask, [property]: value } : mask),
      }
    : clip)
}

type ResolvedAutomation = readonly [
  clip: CompositionClip,
  current: import('../composition/types').CompositionAnimatableValue | undefined,
  fallback: number,
  minimum: number,
  maximum: number,
  write: (value: import('../composition/types').CompositionAnimatableValue | undefined) => void,
]

function resolveAutomation(
  target: CompositionAutomationTarget,
  property: CompositionAutomationProperty,
): ResolvedAutomation {
  if (target.kind === 'visual') {
    const { clip } = requireVisualOverlay(target.clipId)
    const visualProperty = property as CompositionVisualProperty
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
  const current = mask[maskProperty]
  return [
    clip,
    current,
    sampleAnimatableValue(current, 0),
    bounds.minimum,
    bounds.maximum,
    (value) => updateMaskAnimationValue(target.clipId, target.maskId, maskProperty, value!),
  ]
}

function sampleAtPlayhead(resolved: ResolvedAutomation): number {
  const localTicks = clampTick(
    compositionState.transport.playheadTicks - resolved[0].timelineStartTicks,
    clipDurationTicks(resolved[0]),
  )
  return sampleAnimatableValue(resolved[1]!, localTicks)
}

function updateVisualAnimationValue(
  clipId: string,
  property: CompositionVisualProperty,
  value: import('../composition/types').CompositionAnimatableValue | undefined,
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

function findAvailableTrack(document: Composition, kind: TrackKind, start: number, end: number): CompositionTrack | undefined {
  return document.tracks.find(
    (track) =>
      track.kind === kind &&
      !track.locked &&
      track.clips.every((clip) => end <= clip.timelineStartTicks || start >= clipEndTicks(clip)),
  )
}

function makeTrack(kind: TrackKind, id: string, name: string): CompositionTrack {
  const common = { id, name, kind, locked: false, clips: [] as const }
  switch (kind) {
    case 'video': return { ...common, kind, hidden: false, muted: false, transitions: [] } as VideoTrack
    case 'audio': return { ...common, kind, muted: false, solo: false } as AudioTrack
    case 'image': return { ...common, kind, hidden: false } as ImageTrack
    case 'text': return { ...common, kind, hidden: false } as TextTrack
  }
}

function commitDocument(
  document: Composition,
  media: Record<string, CompositionMedia> = compositionState.media,
  output: CompositionRenderOutput = compositionRenderOutput(),
): void {
  const normalized = normalizeComposition(document)
  const nextMedia = cloneCompositionMedia(media)
  const nextOutput: CompositionRenderOutput = {
    profile: { ...output.profile },
    qualityTier: output.qualityTier,
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
  compositionState.export.result = null
  compositionState.export.error = ''
  stopAtDuration()
  scheduleAutosave()
}

function replaceDocument(document: Composition, keepHistory: boolean, autosave = true): void {
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

function cloneCompositionMedia(media: Record<string, CompositionMedia>): Record<string, CompositionMedia> {
  return Object.fromEntries(Object.entries(media).map(([id, item]) => [id, { ...item }]))
}

function repairSelection(): void {
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

function stopAtDuration(): void {
  const duration = compositionDuration()
  if (compositionState.transport.playheadTicks > duration) {
    compositionState.transport.playheadTicks = duration
  }
  if (!duration) compositionState.transport.playing = false
}

function firstFreeStart(track: CompositionTrack, requested: number, duration: number, excludeId: string): number {
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

function snapThresholdTicks(): number {
  return Math.max(1, Math.round((SNAP_THRESHOLD_PX / compositionState.ui.zoomPxPerSecond) * COMPOSITION_TIME_BASE))
}

export function compositionSourceFromMediaInfo(media: MediaInfo, kind = inferMediaType(media)): CompositionSource {
  const durationTicks = kind === 'image' ? 0 : Math.round(positiveNumber(media.duration) * COMPOSITION_TIME_BASE)
  if (kind !== 'image' && durationTicks <= 0) throw new Error('У файла нет корректной длительности')
  const width = kind === 'audio' ? 0 : positiveInteger(media.width)
  const height = kind === 'audio' ? 0 : positiveInteger(media.height)
  if (kind !== 'audio' && (!width || !height)) throw new Error('У файла нет корректных размеров')
  return {
    id: media.id,
    kind,
    durationTicks,
    width,
    height,
    hasAudio: kind === 'audio' || (kind === 'video' && media.acodec != null),
    fps: media.fps ?? null,
    vcodec: media.vcodec ?? null,
    acodec: media.acodec ?? null,
  }
}

export function compositionSourceFromLibraryEntry(entry: MediaEntry): CompositionSource {
  const kind = inferMediaType(entry)
  return compositionSourceFromMediaInfo({
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    mediaType: kind,
    duration: kind === 'image' ? 0 : positiveNumber(entry.duration),
    width: kind === 'audio' ? 0 : positiveInteger(entry.width),
    height: kind === 'audio' ? 0 : positiveInteger(entry.height),
    title: entry.title,
    fps: entry.fps,
    vcodec: entry.vcodec,
    acodec: entry.acodec,
  }, kind)
}

function rememberMedia(media: MediaInfo, mediaType: MediaType): void {
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

export function inferMediaType(media: {
  mediaType?: MediaType | null
  filename: string
  width?: number | null
  height?: number | null
}): MediaType {
  if (media.mediaType === 'video' || media.mediaType === 'audio' || media.mediaType === 'image') {
    return media.mediaType
  }
  const extension = media.filename.split('.').pop()?.toLowerCase()
  if (extension && ['png', 'jpg', 'jpeg', 'webp'].includes(extension)) return 'image'
  if (extension && ['aac', 'flac', 'm4a', 'mp3', 'ogg', 'opus', 'wav'].includes(extension)) return 'audio'
  return (media.width ?? 0) > 0 && (media.height ?? 0) > 0 ? 'video' : 'audio'
}

function isMediaEntry(value: MediaEntry | CompositionSource): value is MediaEntry {
  return value.kind === 'source' || value.kind === 'output'
}

function nextTrackName(document: Composition, label: string): string {
  const count = document.tracks.filter((track) => track.name.startsWith(label)).length
  return `${label} ${count + 1}`
}

function makeId(prefix: string): string {
  const uuid = globalThis.crypto?.randomUUID?.()
  return `${prefix}-${uuid ?? `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`}`
}

function autoBeatMarkerId(tick: number, reserved: string[]): string {
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

function validFps(value?: number | null): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 && value <= 60 ? value : 30
}

function evenDimension(value: number, fallback: number, max: number): number {
  const integer = positiveInteger(value) || fallback
  return Math.max(16, Math.min(integer - (integer % 2), max))
}

function positiveNumber(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0
}

function positiveInteger(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? Math.round(value) : 0
}

function clampNumber(value: number, min: number, max: number): number {
  return Number.isFinite(value) ? Math.max(min, Math.min(value, max)) : min
}

function clampTick(value: number, max: number): number {
  if (!Number.isFinite(value)) return 0
  return Math.round(Math.max(0, Math.min(value, max)))
}

function cloneComposition(document: Composition): Composition {
  return JSON.parse(JSON.stringify(document)) as Composition
}

function getStorage(): CompositionStorage | null {
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

function readActiveStoredDraft(): StoredDraft | null {
  const drafts = readStoredDraftCollection()
  if (!drafts) return null
  return drafts.activeProjectId === null
    ? drafts.unsaved ?? null
    : drafts.projects[drafts.activeProjectId] ?? drafts.unsaved ?? null
}

function readStoredDraftCollection(): StoredDraftCollection | null {
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

function createBlankStoredDraft(): StoredDraft {
  return {
    document: createComposition(DEFAULT_CANVAS),
    media: {},
    projectId: null,
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

function scheduleAutosave(): void {
  activeDraftDirty = true
  const storage = getStorage()
  if (!storage) return
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = setTimeout(() => {
    autosaveTimer = undefined
    persistCompositionDraft(storage)
  }, 300)
}

function persistCompositionDraft(storage: CompositionStorage): void {
  const draft: StoredDraft = {
    document: cloneComposition(compositionState.document),
    media: cloneCompositionMedia(compositionState.media),
    projectId: compositionState.projectId,
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

function persistCurrentDraftNow(clearUnsaved = false): void {
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
    projectName: compositionState.projectName,
    exportSettings: compositionRenderOutput(),
    dirty: activeDraftDirty,
  }
  if (compositionState.projectId === null) drafts.unsaved = draft
  else drafts.projects[compositionState.projectId] = draft
  drafts.activeProjectId = compositionState.projectId
  writeStoredDraftCollection(storage, drafts)
}

function activateStoredDraft(draft: StoredDraft): void {
  const output = normalizeCompositionRenderOutput(draft.exportSettings)
  compositionState.projectId = draft.projectId ?? null
  compositionState.projectName = draft.projectName?.trim() || 'Новая композиция'
  compositionState.media = cloneValidCompositionMedia(draft.media)
  compositionState.export.profile = { ...output.profile }
  compositionState.export.qualityTier = output.qualityTier
  compositionState.export.result = null
  compositionState.export.error = ''
  replaceDocument(draft.document, false, false)
  activeDraftDirty = draft.dirty ?? true
}

function removeStoredProjectDraft(id: string): void {
  const storage = getStorage()
  if (!storage) return
  const drafts = readStoredDraftCollection() ?? emptyStoredDraftCollection()
  delete drafts.projects[id]
  if (drafts.activeProjectId === id) drafts.activeProjectId = null
  writeStoredDraftCollection(storage, drafts)
}

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
  openCompositionRevision += 1
  projectWriteBusy = false
  activateStoredDraft(createBlankStoredDraft())
  activeDraftDirty = false
  compositionState.save.busy = false
  compositionState.save.error = ''
}

export function reloadCompositionDraftForTests(): void {
  if (autosaveTimer) clearTimeout(autosaveTimer)
  autosaveTimer = undefined
  openCompositionRevision += 1
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
