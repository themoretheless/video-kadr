import { linkNativeClips } from './nativeClipLinks'
import { editNativeTrack } from './nativeTrackEdit'
import { rebindNativeLayerReferences } from './nativeLayerReferences'
import { speedRampTimelineDurationTicks } from './speedRamp'
import type { NativeNestedProject } from '$lib/api'
import { MAX_COMPOSITION_DURATION_TICKS, MAX_COMPOSITION_CLIPS, MAX_COMPOSITION_SOURCES, type CompositionSource, type WireCompositionTrack, type WireTransform } from './types'
import { isStableId } from './validation'
export interface NativeMediaInsert { trackId: string; clipId: string; startUs: number; sourceInUs: number; durationUs: number; videoAudio?: boolean; automatic?: boolean; newTrackId?: string }
export function nativeClipEnd(clip: WireCompositionTrack['clips'][number]): number {
  if ('placement' in clip) {
    const p = clip.placement
    const derived = p.speedRamp ? speedRampTimelineDurationTicks(p.sourceOutTick - p.sourceInTick, p.speed, p.speedRamp) : Math.round((p.sourceOutTick - p.sourceInTick) / p.speed)
    return p.timelineStartTick + (p.timelineDurationTick ?? derived - (p.sourceTimelineOffsetTick ?? 0))
  }
  return 'durationTicks' in clip ? clip.timelineStartTick + clip.durationTicks : clip.timelineEndTick
}
export function insertNativeMedia(project: NativeNestedProject, compositionId: string, source: CompositionSource, request: NativeMediaInsert): NativeNestedProject {
  const node = project.compositions[compositionId]
  if (!Object.hasOwn(project.compositions, compositionId) || !node) throw new Error('Таймлайн не найден')
  const track = node.content.tracks.find(track => track.id === request.trackId)
  if (!track || !['video', 'audio', 'image'].includes(track.kind)) throw new Error('Выберите дорожку видео, звука или изображений')
  if (track.locked) throw new Error('Дорожка заблокирована')
  if (!isStableId(source.id) || !isStableId(request.clipId) || node.content.tracks.some(track => track.clips.some(clip => clip.id === request.clipId))) throw new Error('Нужны уникальные корректные идентификаторы')
  if ((track.kind === 'video' && source.kind !== 'video') || (track.kind === 'image' && source.kind !== 'image') || (track.kind === 'audio' && (!source.hasAudio || source.kind === 'image'))) throw new Error('Тип медиа не подходит для дорожки')
  const { startUs, sourceInUs, durationUs } = request
  if (![startUs, sourceInUs, durationUs].every(Number.isSafeInteger) || startUs < 0 || sourceInUs < 0 || durationUs <= 0 || startUs + durationUs > MAX_COMPOSITION_DURATION_TICKS) throw new Error('Некорректное время вставки')
  if (source.kind === 'image' ? sourceInUs !== 0 : sourceInUs + durationUs > source.durationTicks) throw new Error('Диапазон выходит за пределы исходного файла')
  if (track.clips.some(clip => { const from = 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick; return startUs < nativeClipEnd(clip) && startUs + durationUs > from })) throw new Error('На дорожке уже есть клип в этом диапазоне')
  if (node.content.tracks.reduce((count, track) => count + track.clips.length, 0) + (node.content.captionTracks?.reduce((count, track) => count + track.clips.length, 0) ?? 0) >= MAX_COMPOSITION_CLIPS) throw new Error('Достигнут предел клипов')
  const wire: CompositionSource = { id: source.id, kind: source.kind, durationTicks: source.durationTicks, width: source.width, height: source.height, hasAudio: source.hasAudio, ...(source.audioChannels !== undefined ? { audioChannels: source.audioChannels } : {}) }
  const existing = node.content.sources[source.id]
  if (existing && (['id', 'kind', 'durationTicks', 'width', 'height', 'hasAudio', 'audioChannels'] as const).some(key => existing[key] !== wire[key])) throw new Error('Метаданные источника отличаются от зарегистрированных')
  if (!existing && Object.keys(node.content.sources).length >= MAX_COMPOSITION_SOURCES) throw new Error('Достигнут предел источников')
  const constant = (value: number) => ({ mode: 'constant' as const, value })
  const fit = source.kind === 'audio' ? 1 : Math.min(node.content.canvas.width / source.width, node.content.canvas.height / source.height)
  const transform: WireTransform = { x: constant(0), y: constant(0), scaleX: constant(fit), scaleY: constant(fit), rotationDegrees: constant(0), anchorX: 0.5, anchorY: 0.5 }
  const placement = { timelineStartTick: startUs, sourceInTick: sourceInUs, sourceOutTick: sourceInUs + durationUs, speed: 1 }
  let changed: WireCompositionTrack
  if (track.kind === 'video') changed = { ...track, clips: [...track.clips, { id: request.clipId, sourceId: source.id, placement, transform, opacity: constant(1), blendMode: 'normal', effects: [], enabled: true, sourceAudioEnabled: source.hasAudio && (request.videoAudio ?? true), audioGain: constant(1), audioPan: constant(0), playbackMode: { mode: 'forward' }, frameInterpolation: 'duplicate', stabilization: { mode: 'disabled' } }] }
  else if (track.kind === 'audio') changed = { ...track, clips: [...track.clips, { id: request.clipId, sourceId: source.id, placement, gain: constant(1), pan: constant(0), reversed: false, fadeInTicks: 0, fadeOutTicks: 0, voiceEffect: 'none', pitchSemitones: 0, toneDb: 0, crossfadeCurve: 'constant_gain', crossfadeInTicks: 0, enabled: true }] }
  else if (track.kind === 'image') changed = { ...track, clips: [...track.clips, { id: request.clipId, sourceId: source.id, timelineStartTick: startUs, durationTicks: durationUs, transform, opacity: constant(1), blendMode: 'normal', effects: [], enabled: true }] }
  else throw new Error('Неподдерживаемая дорожка')
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, ...(node.durationUs !== undefined ? { durationUs: Math.max(node.durationUs, startUs + durationUs) } : {}), content: { ...node.content, sources: { ...node.content.sources, [wire.id]: wire }, tracks: node.content.tracks.map(track => track.id === changed.id ? changed : track) } } } }
}

export function insertNativeMediaAtFirstFree(project: NativeNestedProject, compositionId: string, source: CompositionSource, request: Omit<NativeMediaInsert, 'trackId'> & { newTrackId: string }): NativeNestedProject {
  const insert = (document: NativeNestedProject, kind: 'video' | 'audio' | 'image', clipId: string, newTrackId: string, videoAudio: boolean): NativeNestedProject => {
    const node = document.compositions[compositionId]
    if (!node || !Object.hasOwn(document.compositions, compositionId)) throw new Error('Таймлайн не найден')
    let target = [...node.content.tracks].reverse().find(track => track.kind === kind && !track.locked && !('hidden' in track && track.hidden) && !('muted' in track && track.muted) && track.clips.every(clip => {
      const start = 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
      return request.startUs + request.durationUs <= start || request.startUs >= nativeClipEnd(clip)
    }))
    if (!target) {
      const added = editNativeTrack(document, compositionId, { kind: 'add', id: newTrackId, trackKind: kind, name: kind === 'video' ? 'Видео' : kind === 'audio' ? 'Звук' : 'Изображения' })
      const created = added.compositions[compositionId]!.content.tracks.at(-1)!
      const tracks = rebindNativeLayerReferences(node.content.tracks, [created, ...node.content.tracks])
      document = { ...added, compositions: { ...added.compositions, [compositionId]: { ...added.compositions[compositionId]!, content: { ...added.compositions[compositionId]!.content, tracks } } } }
      target = created
    }
    return insertNativeMedia(document, compositionId, source, { ...request, trackId: target.id, clipId, videoAudio, automatic: false })
  }
  const separateAudio = source.kind === 'video' && source.hasAudio && (request.videoAudio ?? true)
  let result = insert(project, source.kind, request.clipId, request.newTrackId, false)
  if (separateAudio) {
    const node = result.compositions[compositionId]!
    const reserve = (prefix: string, ids: ReadonlySet<string>): string => { const base = `${prefix.slice(0, 110)}-audio`; let candidate = base, serial = 0; while (ids.has(candidate)) candidate = `${base}-${serial++}`; return candidate }
    const clipId = reserve(request.clipId, new Set(node.content.tracks.flatMap(track => track.clips.map(clip => clip.id))))
    const trackId = reserve(request.newTrackId, new Set([...node.content.tracks.map(track => track.id), ...(node.content.captionTracks?.map(track => track.id) ?? [])]))
    result = insert(result, 'audio', clipId, trackId, false)
    result = linkNativeClips(result, compositionId, [request.clipId, clipId])
  }
  return result
}
