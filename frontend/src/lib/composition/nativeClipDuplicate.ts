import {nearestConcatGroupShift} from './concatMovePlacement'
import {nativeTransitionSpan as window} from './nativeTransitionSpan'
import {rebaseNativeClipAnimation} from './nativeClipAnimation'
import type { NativeNestedProject } from '$lib/api'
import { expandNativeClipLinks, linkNativeClips } from './nativeClipLinks'
import { nativeClipStart } from './nativeClipMove'
import { nativeClipEnd } from './nativeClipInsert'
import { nativeEffectLayers } from './nativeLayerReferences'
import { CAPTION_SOURCE_TICKS_PER_MICROSECOND } from './captionTime'
import { MAX_COMPOSITION_CLIPS, MAX_COMPOSITION_DURATION_TICKS, type WireCompositionTrack } from './types'
import { isStableId } from './validation'
/** Copy a complete selected group with one common timeline delta and independent authored data.
 * detachedStaging is private preparation only: overlapping originals must be removed
 * before sending this intermediate document to native validation or committing it. */
export function duplicateNativeClips(project: NativeNestedProject, compositionId: string, ids: readonly string[], allocate: () => string, startUs?: number, detachedStaging = false, onCopy?: (original: WireCompositionTrack['clips'][number], copyId: string) => void): NativeNestedProject {
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  if (!selected.size) return project
  const node = project.compositions[compositionId]!
  const originals = node.content.tracks.flatMap<WireCompositionTrack['clips'][number]>(track => track.clips.filter(clip => selected.has(clip.id)))
  const origin = Math.min(...originals.map(nativeClipStart))
  let renderStart = origin, renderEnd = Math.max(...originals.map(nativeClipEnd))
  for (const track of node.content.tracks) if (track.kind === 'video') for (const transition of track.transitions) {
    if (!((transition.fromClipId && selected.has(transition.fromClipId)) || (transition.toClipId && selected.has(transition.toClipId)))) continue
    if ((transition.fromClipId && !selected.has(transition.fromClipId)) || (transition.toClipId && !selected.has(transition.toClipId))) throw new Error('Для копирования перехода выберите оба его клипа')
    const span = window(transition)
    if (span) { renderStart = Math.min(renderStart,span[0]); renderEnd = Math.max(renderEnd,span[1]) }
  }
  if (startUs !== undefined && (!Number.isSafeInteger(startUs) || startUs < 0)) throw new Error('Некорректная позиция копии')
  let delta = startUs === undefined ? (detachedStaging ? 0 : renderEnd-renderStart) : startUs-origin
  if (!Number.isSafeInteger(delta)) throw new Error('Диапазон копии выходит за допустимое время')
  const count = node.content.tracks.reduce((n, track) => n+track.clips.length, 0)+(node.content.captionTracks ?? []).reduce((n, track) => n+track.clips.length, 0)
  if ((detachedStaging ? originals.length : count+originals.length) > MAX_COMPOSITION_CLIPS) throw new Error('Слишком много клипов')
  const reserved = new Set([...node.content.tracks.flatMap(track => track.clips.map(clip => clip.id)), ...(node.content.captionTracks ?? []).flatMap(track => track.clips.map(clip => clip.id)), ...node.content.tracks.flatMap(track => track.kind === 'video' ? track.transitions.map(transition => transition.id) : [])])
  const mint = () => { const id = allocate(); if (!isStableId(id) || reserved.has(id)) throw new Error('Нужен уникальный идентификатор копии'); reserved.add(id); return id }
  const copied = new Map(originals.map(clip => [clip.id, mint()]))
  for (const clip of originals) onCopy?.(clip, copied.get(clip.id)!)
  const layers = nativeEffectLayers(node.content.tracks)
  const renderedClipSpan = (clip: WireCompositionTrack['clips'][number]): readonly [number,number] => {
    let from = nativeClipStart(clip), end = nativeClipEnd(clip)
    for (const track of node.content.tracks) if (track.kind === 'video') for (const transition of track.transitions) {
      if (transition.fromClipId !== clip.id && transition.toClipId !== clip.id) continue
      const span = window(transition)
      if (span) { from = Math.min(from,span[0]); end = Math.max(end,span[1]) }
    }
    return [from,end]
  }
  if(!detachedStaging){
    const moving=node.content.tracks.flatMap(track=>track.clips.filter(clip=>selected.has(clip.id)).map(clip=>{const [start,end]=renderedClipSpan(clip);return {trackId:track.id,overlapGroup:track.id,start:start+delta,end:end+delta}}))
    const staying=node.content.tracks.flatMap(track=>{
      const spans=track.clips.map(clip=>{const [start,end]=renderedClipSpan(clip);return {trackId:track.id,start,end}})
      if(track.kind==='video')for(const transition of track.transitions){const span=window(transition);if(span)spans.push({trackId:track.id,start:span[0],end:span[1]})}
      return spans
    })
    delta+=nearestConcatGroupShift(moving,staying)
    if(!Number.isSafeInteger(delta)||renderStart+delta<0||renderEnd+delta>MAX_COMPOSITION_DURATION_TICKS)throw new Error('Диапазон копии выходит за допустимое время')
  }
  for (const clip of originals) if ('effects' in clip) for (const effect of clip.effects ?? []) {
    const field = effect.kind === 'track_matte' ? 'matte' : effect.kind === 'gradient_wipe' || effect.kind === 'compound_blur' ? 'layer' : null
    if (!field || !('parameters' in effect)) continue
    const raw = (effect.parameters as unknown as Record<string, unknown>)[field]
    if (raw === undefined || raw === 0) continue
    if (typeof raw !== 'number' || !Number.isInteger(raw) || raw < 0) throw new Error('Некорректная ссылка эффекта на слой')
    const control = layers[raw-1]
    if (!control) continue
    const [from,end] = renderedClipSpan(clip)
    if (control.clips.some(source => { const [a,b] = renderedClipSpan(source); return a<end && b>from && !selected.has(source.id) })) throw new Error(`Выберите также используемые клипы дорожки ${control.name}`)
    if (!detachedStaging && control.clips.some(source => { const [a,b] = renderedClipSpan(source); return a<end+delta && b>from+delta })) throw new Error('Исходная управляющая дорожка перекрывает время копии')
  }
  for (const track of node.content.tracks) if (track.kind === 'audio') {
    const ordered = [...track.clips].sort((a,b) => nativeClipStart(a)-nativeClipStart(b))
    for (let index = 1; index < ordered.length; index++) {
      const previous = ordered[index-1]!, clip = ordered[index]!
      if (clip.crossfadeInTicks > 0 && selected.has(previous.id) !== selected.has(clip.id)) throw new Error('Для копирования аудиоперехода выберите оба его клипа')
    }
  }
  const tracks = node.content.tracks.map(track => {
    const additions = track.clips.filter(clip => selected.has(clip.id)).map(clip => {
      if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
      const detached = JSON.parse(JSON.stringify(clip)) as typeof clip
      const next = 'graphic' in detached && detached.concatTreatment ? {...detached,effects:rebaseNativeClipAnimation(detached,delta).effects} : detached
      const from = nativeClipStart(clip)+delta, end = nativeClipEnd(clip)+delta
      if (!Number.isSafeInteger(from) || !Number.isSafeInteger(end) || from < 0 || end > MAX_COMPOSITION_DURATION_TICKS) throw new Error('Позиция копии выходит за допустимый диапазон')
      if (!detachedStaging && track.clips.some(other => from < nativeClipEnd(other) && end > nativeClipStart(other))) throw new Error('На дорожке нет места для копии')
      return 'placement' in next ? { ...next, id: copied.get(clip.id)!, placement: { ...next.placement, timelineStartTick: from } } : 'durationTicks' in next ? { ...next, id: copied.get(clip.id)!, timelineStartTick: from } : { ...next, id: copied.get(clip.id)!, timelineStartTick: from, timelineEndTick: end }
    })
    if (!additions.length) return track
    if (track.kind !== 'video') return { ...track, clips: [...track.clips, ...additions] } as WireCompositionTrack
    const transitions = track.transitions.filter(t => (t.fromClipId && selected.has(t.fromClipId)) || (t.toClipId && selected.has(t.toClipId))).map(transition => {
      if ((transition.fromClipId && !copied.has(transition.fromClipId)) || (transition.toClipId && !copied.has(transition.toClipId))) throw new Error('Для копирования перехода выберите оба его клипа')
      const next = JSON.parse(JSON.stringify(transition)) as typeof transition
      return { ...next, id: mint(), ...(next.fromClipId ? { fromClipId: copied.get(next.fromClipId)! } : {}), ...(next.toClipId ? { toClipId: copied.get(next.toClipId)! } : {}), ...(next.timeline ? { timeline: { ...next.timeline, startTick: String(BigInt(next.timeline.startTick)+BigInt(delta)*CAPTION_SOURCE_TICKS_PER_MICROSECOND) } } : {}) }
    })
    const oldWindows = track.transitions.map(window).filter((span): span is readonly [number,number] => span !== null)
    const newWindows = transitions.map(window).filter((span): span is readonly [number,number] => span !== null)
    if (!detachedStaging && (oldWindows.some(([from,end]) => additions.some(clip => nativeClipStart(clip)<end && nativeClipEnd(clip)>from)) || newWindows.some(([from,end]) => track.clips.some(clip => nativeClipStart(clip)<end && nativeClipEnd(clip)>from) || oldWindows.some(([a,b]) => from<b && end>a)))) throw new Error('Переходы перекрывают время копии')
    return { ...track, clips: [...track.clips, ...additions], transitions: [...track.transitions, ...transitions] } as WireCompositionTrack
  })
  const metadata = { ...node.content.metadata }
  for (const field of ['clipNames','mediaVersions'] as const) {
    const values = metadata[field]
    if (values) Object.assign(metadata, { [field]: Object.fromEntries([...Object.entries(values), ...[...copied].filter(([id]) => Object.hasOwn(values,id)).map(([id, copy]) => [copy, JSON.parse(JSON.stringify(values[id]))])]) })
  }
  const end = Math.max(...tracks.flatMap(track => track.clips.map(nativeClipEnd)))
  let result = { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, ...(node.durationUs !== undefined || renderEnd+delta > end ? { durationUs: Math.max(node.durationUs ?? 0, end, renderEnd+delta) } : {}), content: { ...node.content, tracks, metadata } } } }
  for (const members of Object.values(node.content.metadata?.clipLinks ?? {})) if (members.some(id => selected.has(id))) result = linkNativeClips(result, compositionId, members.map(id => copied.get(id)!))
  return result
}
