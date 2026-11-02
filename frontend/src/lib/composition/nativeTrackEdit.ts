import { nativeLaneMembers } from './nativeLaneGroups'
import { removeNativeClips } from './nativeClipDelete'
import { rebindNativeLayerReferences } from './nativeLayerReferences'
import type { NativeNestedProject } from '$lib/api'
import type { WireCompositionTrack } from './types'
import { MAX_COMPOSITION_TRACKS } from './types'
import { isStableId } from './validation'
export type NativeTrackKind = WireCompositionTrack['kind']
export type NativeTrackFlag = 'hidden' | 'muted' | 'solo' | 'locked'
export type NativeTrackEdit = { kind: 'add'; id: string; trackKind: NativeTrackKind; name: string } | { kind: 'remove'; id: string; removeClips: boolean } | { kind: 'rename'; id: string; name: string } | { kind: 'flag'; id: string; flag: NativeTrackFlag; value: boolean }
export function editNativeTrack(project: NativeNestedProject, compositionId: string, edit: NativeTrackEdit): NativeNestedProject {
  const node = project.compositions[compositionId]
  if (!Object.hasOwn(project.compositions, compositionId) || !node) throw new Error('Таймлайн не найден')
  const content = node.content, tracks = [...content.tracks]
  let metadata = content.metadata
  const name = (value: string): string => { const text = value.trim(); if (!text || text.length > 128) throw new Error('Название дорожки: от 1 до 128 символов'); return text }
  if (edit.kind === 'add') {
    if (!['video', 'audio', 'image', 'text', 'color', 'graphic'].includes(edit.trackKind)) throw new Error('Неизвестный тип дорожки')
    if (!isStableId(edit.id) || tracks.some(track => track.id === edit.id) || content.captionTracks?.some(track => track.id === edit.id)) throw new Error('Нужен уникальный идентификатор дорожки')
    if (tracks.length + (content.captionTracks?.length ?? 0) >= MAX_COMPOSITION_TRACKS) throw new Error('Достигнут предел дорожек')
    const base = { id: edit.id, name: name(edit.name), locked: false, clips: [] }
    const track: WireCompositionTrack = edit.trackKind === 'video' ? { ...base, kind: 'video', hidden: false, muted: false, transitions: [] } : edit.trackKind === 'audio' ? { ...base, kind: 'audio', muted: false, solo: false } : { ...base, kind: edit.trackKind, hidden: false }
    tracks.push(track)
  } else {
    const index = tracks.findIndex(track => track.id === edit.id)
    if (index < 0) throw new Error('Дорожка не найдена')
    const track = tracks[index]!
    const members = new Set(nativeLaneMembers(content, edit.id))
    const rows = tracks.filter(lane => members.has(lane.id))
    if (edit.kind === 'flag') {
      if (!(edit.flag in track)) throw new Error('Этот флаг недоступен для дорожки')
      const affected = edit.flag === 'hidden' || edit.flag === 'locked' ? members : new Set([track.id])
      if (tracks.filter(lane => affected.has(lane.id)).every(lane => lane[edit.flag as keyof typeof lane] === edit.value)) return project
      for (let i = 0; i < tracks.length; i++) if (affected.has(tracks[i]!.id)) tracks[i] = { ...tracks[i]!, [edit.flag]: edit.value }
    } else if (edit.kind === 'rename') {
      if (rows.some(lane => lane.locked)) throw new Error('Дорожка заблокирована')
      const text = name(edit.name)
      if (rows.every(lane => lane.name === text)) return project
      for (let i = 0; i < tracks.length; i++) if (members.has(tracks[i]!.id)) tracks[i] = { ...tracks[i]!, name: text }
    } else {
      if (rows.some(lane => lane.locked)) throw new Error('Дорожка заблокирована')
      const clipIds = rows.flatMap(lane => lane.clips.map(clip => clip.id))
      if (clipIds.length && !edit.removeClips) throw new Error('Подтвердите удаление клипов дорожки')
      const cleared = removeNativeClips(project, compositionId, clipIds)
      const contentAfter = cleared.compositions[compositionId]!.content
      tracks.splice(0, tracks.length, ...contentAfter.tracks.filter(lane => !members.has(lane.id)))
      metadata = contentAfter.metadata
      if (metadata) {
        const removed = new Set(clipIds)
        metadata = { ...metadata, ...(metadata.concatLaneGroups ? { concatLaneGroups: Object.fromEntries(Object.entries(metadata.concatLaneGroups).filter(([, ids]) => !ids.some(id => members.has(id)))) } : {}), clipNames: Object.fromEntries(Object.entries(metadata.clipNames ?? {}).filter(([id]) => !removed.has(id))), mediaVersions: Object.fromEntries(Object.entries(metadata.mediaVersions ?? {}).filter(([id]) => !removed.has(id))) }
      }
    }
  }
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...content, tracks: edit.kind === 'remove' ? rebindNativeLayerReferences(content.tracks, tracks) : tracks, ...(metadata ? { metadata } : {}) } } } }
}
