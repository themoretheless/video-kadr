import {nativeTransitionSpan} from './nativeTransitionSpan'
import {nearestConcatGroupShift} from './concatMovePlacement'
import {nativeClipEnd} from './nativeClipInsert'
import { expandNativeClipLinks } from './nativeClipLinks'
import type { NativeClipMove, NativeNestedProject } from '$lib/api'
import type { WireCompositionTrack } from './types'
import { isStableId } from './validation'
export function nativeClipStart(clip: WireCompositionTrack['clips'][number]): number {
  return 'placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick
}
export function defaultNativeTargetTrack(project: NativeNestedProject, from: string, to: string, sourceTrackId: string): string {
  const tracks = project.compositions[from]?.content.tracks ?? []
  const source = tracks.find(track => track.id === sourceTrackId)
  if (!source) return ''
  const index = tracks.filter(track => track.kind === source.kind).findIndex(track => track.id === sourceTrackId)
  return project.compositions[to]?.content.tracks.filter(track => track.kind === source.kind)[index]?.id ?? ''
}
export function nativeClipMoveBatch(project: NativeNestedProject, from: string, to: string, ids: readonly string[], startUs: number, targets: Readonly<Record<string, string>>, allocate: () => string): NativeClipMove[] {
  const source = project.compositions[from], destination = project.compositions[to]
  if (!source || !destination) throw new Error('Таймлайн не найден')
  if (!ids.length || ids.length > 512 || new Set(ids).size !== ids.length) throw new Error('Выберите от 1 до 512 разных клипов')
  if (!Number.isSafeInteger(startUs) || startUs < 0) throw new Error('Некорректная позиция переноса')
  const linkedIds = expandNativeClipLinks(project, from, ids)
  const selected = source.content.tracks.flatMap(track => track.clips.filter(clip => linkedIds.includes(clip.id)).map(clip => ({ track, clip })))
  if (selected.length !== linkedIds.length) throw new Error('Выбранный клип не найден')
  const origin = Math.min(...selected.map(({ clip }) => nativeClipStart(clip)))
  for(const track of source.content.tracks)if(track.kind==='video')for(const transition of track.transitions){
    const endpoints=[transition.fromClipId,transition.toClipId].filter((id):id is string=>Boolean(id))
    if(endpoints.some(id=>linkedIds.includes(id))&&endpoints.some(id=>!linkedIds.includes(id)))throw new Error('Для переноса перехода выберите оба его клипа')
  }
  const renderedSpan=(track:WireCompositionTrack,clip:WireCompositionTrack['clips'][number]):readonly[number,number]=>{
    let start=nativeClipStart(clip),end=nativeClipEnd(clip)
    if(track.kind==='video')for(const transition of track.transitions){
      if(transition.fromClipId!==clip.id&&transition.toClipId!==clip.id)continue
      const span=nativeTransitionSpan(transition);if(span){start=Math.min(start,span[0]);end=Math.max(end,span[1])}
    }
    return [start,end]
  }
  const newTracks = new Map<string, string>()
  const reserved = new Set(destination.content.tracks.map(track => track.id))
  const moves = selected.map(({ track, clip }) => {
    if (track.locked) throw new Error('Исходная дорожка заблокирована')
    const choice = targets[track.id] ?? defaultNativeTargetTrack(project, from, to, track.id)
    let toTrackId = choice
    if (!choice) {
      if (!newTracks.has(track.id)) {
        const id = allocate()
        if (!isStableId(id) || reserved.has(id)) throw new Error('Нужен уникальный идентификатор новой дорожки')
        reserved.add(id); newTracks.set(track.id, id)
      }
      toTrackId = newTracks.get(track.id)!
    } else {
      const target = destination.content.tracks.find(item => item.id === choice)
      if (!target || target.kind !== track.kind) throw new Error('Тип целевой дорожки не совпадает')
      if (target.locked) throw new Error('Целевая дорожка заблокирована')
    }
    const start = startUs + nativeClipStart(clip) - origin
    if (!Number.isSafeInteger(start) || start < 0) throw new Error('Позиция переноса выходит за точный диапазон')
    return { fromComposition: from, fromTrackId: track.id, clipId: clip.id, toComposition: to, toTrackId, startUs: start, createTrack: !choice }
  })
  const moving=moves.map((move,index)=>{
    const {track,clip}=selected[index]!,[start,end]=renderedSpan(track,clip),delta=move.startUs-nativeClipStart(clip)
    return {trackId:move.toTrackId,overlapGroup:move.fromTrackId,start:start+delta,end:end+delta}
  })
  const staying=destination.content.tracks.flatMap(track=>{
    const spans=track.clips.filter(clip=>from!==to||!linkedIds.includes(clip.id)).map(clip=>{const [start,end]=renderedSpan(track,clip);return {trackId:track.id,start,end}})
    if(track.kind==='video')for(const transition of track.transitions){
      if(from===to&&[transition.fromClipId,transition.toClipId].some(id=>id&&linkedIds.includes(id)))continue
      const span=nativeTransitionSpan(transition);if(span)spans.push({trackId:track.id,start:span[0],end:span[1]})
    }
    return spans
  })
  const shift=nearestConcatGroupShift(moving,staying)
  return shift?moves.map(move=>({...move,startUs:move.startUs+shift})):moves
}
