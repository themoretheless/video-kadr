import { computed, reactive } from 'vue'
import * as api from './api'

import {
  createMulticamGroup,
  activeMulticamAngle,
  correlateMulticamAudio,
  insertOrReplaceMulticamDecision,
  syncMulticamByMarkers,
  syncMulticamByTimecode,
  type MarkerAnchor,
  type MulticamGroup,
  type TimecodeAnchor,
  multicamAngleSourceSeconds,
  multicamOutputSecondsForAngle,
} from './domain/multicam'
import { projectFrameDurationTicks } from './domain/timeline'
import type { ProjectMedia, ProjectMulticamGroup } from './project-schema'
import { executeTimelineCommand, state, timelineState } from './store'

export interface MulticamGroupView {
  id: string
  name: string
  activeAngleId: string
  referenceMediaId: string
  syncMode: 'audio' | 'timecode' | 'marker'
  angles: Array<{
    id: string
    mediaId: string
    name: string
    availability: 'ready' | 'offline' | 'syncing' | 'error'
    offsetTicks: number
  }>
}

const session = reactive({ activeGroupId: null as string | null, busy: false, status: '', error: '' })

function groups(): ProjectMulticamGroup[] { return timelineState.document?.multicamGroups ?? [] }
function activeDecision(group: ProjectMulticamGroup): string {
  const tick = Math.max(0, Math.round(state.playerTime * group.timeBase))
  let active = group.decisions[0]!.angleId
  for (const decision of group.decisions) {
    if (decision.offsetTick > tick) break
    active = decision.angleId
  }
  return active
}

export const multicamState = reactive({
  get groups(): MulticamGroupView[] {
    const document = timelineState.document
    const availability = new Map((document?.media ?? []).map(media => {
      const assetRef = media.assetRef ?? media.id
      const entry = state.library.find(item => item.id === assetRef || item.assetId === assetRef)
      return [media.id, entry?.availability === 'offline' || !entry ? 'offline' : 'ready'] as const
    }))
    return groups().map(group => ({
      id: group.id,
      name: group.name,
      activeAngleId: activeDecision(group),
      referenceMediaId: group.angles.find(angle => angle.id === group.referenceAngleId)?.mediaId ?? '',
      syncMode: group.sync.method,
      angles: group.angles.map(angle => ({
        id: angle.id,
        mediaId: angle.mediaId,
        name: angle.label,
        availability: availability.get(angle.mediaId) ?? 'offline',
        offsetTicks: angle.sourceOriginTick,
      })),
    }))
  },
  get activeGroupId(): string | null {
    return groups().some(group => group.id === session.activeGroupId)
      ? session.activeGroupId : groups()[0]?.id ?? null
  },
  get playing(): boolean { return state.playerPlaying },
  get busy(): boolean { return session.busy },
  get status(): string { return session.status },
  get error(): string { return session.error },
})

export interface MulticamSourceView {
  id: string
  name: string
  durationTicks: number
  fps?: number | null
  availability: 'ready' | 'offline' | 'permission-required'
}

export const projectMulticamSources = computed<MulticamSourceView[]>(() => {
  const document = timelineState.document
  if (!document) return []
  const sequence = document.sequences.find(item => item.id === document.activeSequenceId)
  const timeBase = sequence?.settings.timeBase ?? 1_000_000
  return document.media.filter(media => media.kind === 'video').map(media => {
    const assetRef = media.assetRef ?? media.id
    const library = state.library.find(item => item.id === assetRef || item.assetId === assetRef)
    const rawDuration = media.metadata.duration
    const durationTicks = typeof rawDuration === 'number' && Number.isFinite(rawDuration)
      ? Math.max(0, Math.round(rawDuration * timeBase)) : 0
    const unavailable = library?.availability === 'offline' || !library
      ? 'offline' : library.availability === 'permission-required' ? 'permission-required' : 'ready'
    return {
      id: media.id,
      name: typeof media.metadata.title === 'string' && media.metadata.title.trim()
        ? media.metadata.title.trim() : typeof media.metadata.filename === 'string' ? media.metadata.filename : media.id,
      durationTicks,
      fps: typeof media.metadata.fps === 'number' ? media.metadata.fps : library?.fps,
      availability: unavailable,
    }
  })
})

export const multicamTimeBase = computed(() => {
  const document = timelineState.document
  return document?.sequences.find(item => item.id === document.activeSequenceId)?.settings.timeBase ?? 1_000_000
})

export function selectMulticamGroup(id: string | null): void {
  if (id !== null && !groups().some(group => group.id === id)) throw new Error('Multicam-группа не найдена')
  session.activeGroupId = id
  session.error = ''
}

export async function resolveMulticamAngleSource(groupId: string, angleId: string): Promise<string> {
  const document = timelineState.document
  const group = document?.multicamGroups.find(item => item.id === groupId)
  const angle = group?.angles.find(item => item.id === angleId)
  const media = document?.media.find(item => item.id === angle?.mediaId)
  const assetRef = media?.assetRef ?? media?.id
  const entry = state.library.find(item => item.id === assetRef || item.assetId === assetRef)
  if (!media || !entry) throw new Error('Источник ракурса offline')
  return (await api.resolveLibrarySource(entry, media.contentFingerprint)).url
}

export interface AttachedMulticamProgramTarget {
  groupId: string
  angleId: string
  sourceSeconds: number
  rate: number
}

export interface ResolvedAttachedMulticamProgramTarget extends AttachedMulticamProgramTarget { url: string }

export function attachedMulticamProgramAt(outputSeconds: number): AttachedMulticamProgramTarget | null {
  const document = timelineState.document
  const sequence = document?.sequences.find(item => item.id === document.activeSequenceId)
  const clip = sequence?.tracks.flatMap(track => track.clips).find(item => item.multicamGroupId)
  const group = clip?.multicamGroupId ? document?.multicamGroups.find(item => item.id === clip.multicamGroupId) : undefined
  if (!group || !sequence || !clip || !Number.isFinite(outputSeconds)) return null
  const startSeconds = clip.timelineStartTick / group.timeBase
  const endSeconds = (clip.timelineStartTick + clip.durationTicks) / group.timeBase
  if (outputSeconds < startSeconds || outputSeconds >= endSeconds) return null
  const frameTicks = projectFrameDurationTicks(sequence.settings)
  const localTick = Math.round(outputSeconds * group.timeBase) - clip.timelineStartTick
  const tick = Math.min(clip.sourceOutTick - 1, Math.max(clip.sourceInTick, clip.sourceInTick + localTick))
  const angle = activeMulticamAngle(group, group.decisions, tick, frameTicks)
  return { groupId: group.id, angleId: angle.id, sourceSeconds: multicamAngleSourceSeconds(group, angle.id, tick), rate: angle.rate.numerator / angle.rate.denominator }
}

export function attachedMulticamProgram(): AttachedMulticamProgramTarget | null {
  return attachedMulticamProgramAt(state.playerTime)
}

export async function resolveAttachedMulticamProgramAt(outputSeconds: number): Promise<ResolvedAttachedMulticamProgramTarget | null> {
  const target = attachedMulticamProgramAt(outputSeconds)
  if (!target) return null
  return { ...target, url: await resolveMulticamAngleSource(target.groupId, target.angleId) }
}

export function attachedMulticamOutputSeconds(angleId: string, sourceSeconds: number): number | null {
  const document = timelineState.document
  const sequence = document?.sequences.find(item => item.id === document.activeSequenceId)
  const clip = sequence?.tracks.flatMap(track => track.clips).find(item => item.multicamGroupId)
  const group = clip?.multicamGroupId ? document?.multicamGroups.find(item => item.id === clip.multicamGroupId) : undefined
  return group && clip ? clip.timelineStartTick / group.timeBase
    + multicamOutputSecondsForAngle(group, angleId, sourceSeconds) - clip.sourceInTick / group.timeBase : null
}

export function attachedMulticamSourceSeconds(outputSeconds: number): number | null {
  return attachedMulticamProgramAt(outputSeconds)?.sourceSeconds ?? null
}

export function attachedMulticamOutputBounds(): { start: number; end: number } | null {
  const document = timelineState.document
  const sequence = document?.sequences.find(item => item.id === document.activeSequenceId)
  const clip = sequence?.tracks.flatMap(track => track.clips).find(item => item.multicamGroupId)
  const timeBase = sequence?.settings.timeBase
  return clip && timeBase ? { start: clip.timelineStartTick / timeBase, end: (clip.timelineStartTick + clip.durationTicks) / timeBase } : null
}

function nextId(prefix: string): string {
  return `${prefix}-${crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`}`
}

export interface MulticamCommitInput {
  name: string
  mediaIds: string[]
  syncMode: 'audio' | 'timecode' | 'marker'
  referenceMediaId: string
  markerAnchors?: Record<string, number>
  timecodeAnchors?: Record<string, { startFrame: number; rate: { numerator: number; denominator: number }; dropFrame: boolean }>
}

async function decodeSyncEnvelope(mediaId: string): Promise<{ samples: Float32Array; secondsPerSample: number }> {
  const document = timelineState.document
  const media = document?.media.find(item => item.id === mediaId)
  const assetRef = media?.assetRef ?? media?.id
  const entry = state.library.find(item => item.id === assetRef || item.assetId === assetRef)
  if (!media || !assetRef || !entry) throw new Error(`Источник ${mediaId} offline`)
  return api.extractAudioSyncEnvelope(assetRef, media.contentFingerprint)
}

async function audioMarkerAnchors(base: MulticamGroup): Promise<{ markers: Record<string, number>; confidence: number }> {
  session.status = 'Декодирую аудио ракурсов…'
  const envelopes: Awaited<ReturnType<typeof decodeSyncEnvelope>>[] = []
  for (const angle of base.angles) envelopes.push(await decodeSyncEnvelope(angle.mediaId))
  const referenceIndex = base.angles.findIndex(angle => angle.id === base.referenceAngleId)
  const reference = envelopes[referenceIndex]!
  const matches = envelopes.map((candidate, index) => index === referenceIndex ? { offsetSamples: 0, confidence: 1 }
    : correlateMulticamAudio(reference.samples, candidate.samples, Math.min(450, reference.samples.length - 64, candidate.samples.length - 64)))
  const lags = matches.map(match => match.offsetSamples)
  const minimum = Math.min(...lags)
  const ticksPerEnvelopeSample = base.timeBase * reference.secondsPerSample
  return {
    markers: Object.fromEntries(base.angles.map((angle, index) => [angle.mediaId, Math.round((lags[index]! - minimum) * ticksPerEnvelopeSample)])),
    confidence: Math.min(...matches.map(match => match.confidence)),
  }
}

async function syncedGroup(base: MulticamGroup, input: MulticamCommitInput, media: ProjectMedia[]): Promise<MulticamGroup> {
  if (input.syncMode === 'audio') {
    const audio = await audioMarkerAnchors(base)
    const synced = syncMulticamByMarkers(base, base.angles.map(angle => ({ angleId: angle.id, sourceTick: audio.markers[angle.mediaId]! })), media)
    return { ...synced, sync: { method: 'audio', algorithmVersion: 'webaudio-envelope-correlation-v1', confidence: audio.confidence } }
  }
  if (input.syncMode === 'marker') {
    const anchors: MarkerAnchor[] = base.angles.map(angle => {
      const sourceTick = input.markerAnchors?.[angle.mediaId]
      if (!Number.isSafeInteger(sourceTick) || Number(sourceTick) < 0) throw new Error(`Sync-точка не указана для ${angle.label}`)
      return { angleId: angle.id, sourceTick: Number(sourceTick) }
    })
    return syncMulticamByMarkers(base, anchors, media)
  }
  const anchors: TimecodeAnchor[] = base.angles.map(angle => {
    const anchor = input.timecodeAnchors?.[angle.mediaId]
    if (!anchor) throw new Error(`Таймкод не указан для ${angle.label}`)
    return { angleId: angle.id, ...anchor }
  })
  return syncMulticamByTimecode(base, anchors, media)
}

export async function createMulticam(input: MulticamCommitInput): Promise<MulticamGroupView> {
  const document = timelineState.document
  if (!document) throw new Error('Сначала откройте проект')
  session.busy = true; session.error = ''; session.status = 'Проверяю источники…'
  try {
    const media = input.mediaIds.map(id => document.media.find(item => item.id === id))
    if (media.some(item => !item)) throw new Error('Один из источников отсутствует в проекте')
    const base = createMulticamGroup({
      id: nextId('multicam'), name: input.name, timeBase: document.sequences.find(item => item.id === document.activeSequenceId)?.settings.timeBase ?? 1_000_000,
      media: media as typeof document.media, referenceMediaId: input.referenceMediaId,
    })
    const synced = await syncedGroup(base, input, media as ProjectMedia[])
    if (timelineState.document !== document) throw new Error('Проект изменился во время синхронизации; результат отменён')
    const persisted: ProjectMulticamGroup = { ...synced, decisions: [{ id: nextId('decision'), offsetTick: 0, angleId: synced.referenceAngleId }] }
    const sequence = document.sequences.find(item => item.id === document.activeSequenceId)
    const carrier = sequence?.tracks.flatMap(track => track.clips).find(clip => clip.mediaId === document.primaryMediaId)
    if (!sequence || !carrier) throw new Error('На timeline нет клипа для multicam')
    if (!executeTimelineCommand({ kind: 'batch', commands: [
      { kind: 'set_multicam_groups', groups: [...groups(), persisted] },
      { kind: 'attach_multicam_group', sequenceId: sequence.id, clipId: carrier.id, groupId: persisted.id },
    ] })) throw new Error(timelineState.error)
    session.activeGroupId = persisted.id
    session.status = `Multicam создан · синхронизация: ${input.syncMode}`
    return multicamState.groups.find(item => item.id === persisted.id)!
  } catch (cause) {
    session.error = cause instanceof Error ? cause.message : String(cause)
    throw cause
  } finally { session.busy = false }
}

export async function resyncMulticam(groupId: string, input: MulticamCommitInput): Promise<MulticamGroupView> {
  const document = timelineState.document
  const current = groups().find(group => group.id === groupId)
  if (!document || !current) throw new Error('Multicam-группа не найдена')
  session.busy = true; session.error = ''; session.status = 'Проверяю sync-точки…'
  try {
    const media = current.angles.map(angle => document.media.find(item => item.id === angle.mediaId))
    if (media.some(item => !item)) throw new Error('Один из источников отсутствует в проекте')
    const base = createMulticamGroup({
      id: current.id, name: input.name, timeBase: current.timeBase,
      media: media as ProjectMedia[], referenceMediaId: input.referenceMediaId,
    })
    const synced = await syncedGroup(base, input, media as ProjectMedia[])
    if (timelineState.document !== document) throw new Error('Проект изменился во время синхронизации; результат отменён')
    const decisions = current.decisions.filter(decision => decision.offsetTick < synced.durationTicks)
    const safeDecisions = decisions.length ? decisions : [{ id: nextId('decision'), offsetTick: 0, angleId: synced.referenceAngleId }]
    if (safeDecisions[0]!.offsetTick !== 0) safeDecisions.unshift({ id: nextId('decision'), offsetTick: 0, angleId: synced.referenceAngleId })
    const persisted: ProjectMulticamGroup = { ...synced, decisions: safeDecisions }
    if (!executeTimelineCommand({ kind: 'set_multicam_groups', groups: groups().map(group => group.id === groupId ? persisted : group) })) throw new Error(timelineState.error)
    session.activeGroupId = groupId; session.status = `Синхронизация обновлена · ${input.syncMode}`
    return multicamState.groups.find(group => group.id === groupId)!
  } catch (cause) {
    session.error = cause instanceof Error ? cause.message : String(cause)
    throw cause
  } finally { session.busy = false }
}

export function switchMulticamAngle(groupId: string, angleId: string, options: { live: boolean }): void {
  const document = timelineState.document
  const group = groups().find(item => item.id === groupId)
  const sequence = document?.sequences.find(item => item.id === document.activeSequenceId)
  if (!document || !group || !sequence) throw new Error('Multicam-группа не найдена')
  const frameTicks = projectFrameDurationTicks(sequence.settings)
  const clip = sequence.tracks.flatMap(track => track.clips).find(item => item.multicamGroupId === groupId)
  if (!clip) throw new Error('Multicam-клип не найден на timeline')
  const rawTick = Math.max(clip.sourceInTick, clip.sourceInTick + Math.round(state.playerTime * group.timeBase) - clip.timelineStartTick)
  const lastFrameTick = Math.floor((Math.min(group.durationTicks, clip.sourceOutTick) - 1) / frameTicks) * frameTicks
  const offsetTick = Math.min(lastFrameTick, Math.floor(rawTick / frameTicks) * frameTicks)
  const decision = { id: nextId('decision'), offsetTick, angleId }
  const decisions = insertOrReplaceMulticamDecision(group as MulticamGroup, group.decisions, decision, frameTicks)
  const updated = { ...group, decisions }
  if (!executeTimelineCommand({ kind: 'set_multicam_groups', groups: groups().map(item => item.id === groupId ? updated : item) })) throw new Error(timelineState.error)
  session.status = options.live ? `LIVE: переключено на ${group.angles.find(item => item.id === angleId)?.label ?? angleId}` : 'Ракурс выбран'
}

export const activeMulticamGroup = computed(() => multicamState.groups.find(group => group.id === multicamState.activeGroupId) ?? null)
