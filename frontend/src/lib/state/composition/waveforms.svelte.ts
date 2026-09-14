import {
  localWaveformCache
} from '../../audio/waveformCache.js'
import {
  estimateWaveformOffset,
  MAX_SYNC_SAMPLES
} from '../../audio/sync.js'
import {
  MAX_MULTICAM_ANGLES,
  MIN_MULTICAM_ANGLES,
  recordMulticamSwitch,
  sourceTickAtGroupStartFromOffset,
  type MulticamAngle,
  type MulticamGroup
} from '../../composition/multicam.js'
import {
  addTrack
} from '../../composition/commands.js'
import {
  COMPOSITION_TIME_BASE,
  MAX_COMPOSITION_DURATION_TICKS
} from '../../composition/types.js'
import type {
  WaveformSummary
} from '../../audio/waveform.js'
import {
  commitDocument,
  compositionMulticamGroups,
  compositionState,
  makeId,
  rebuildMulticamDocument,
  resampleWaveformEnergy,
  selectCompositionClip
} from './core.svelte.js'

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

