import { captureNativeCaptionTracks } from './nativeCaptionClipboard'
import { nativeClipStart } from './nativeClipMove'
import { exactCaptionTicks, CAPTION_SOURCE_TICKS_PER_MICROSECOND as UNIT } from './captionTime'
import type { NativeNestedProject } from '$lib/api'
import { captureNativeClipContent, prepareNativeClipCopy } from './nativeClipCopy'
import { orderedTimelineIds } from './timelineOrder'
import { isStableId } from './validation'

export function captureNativeClipboard(project: NativeNestedProject, from: string, ids: readonly string[], allocate: () => string): NativeNestedProject {
  const source=project.compositions[from]
  if (!source || !ids.length || ids.length>512 || new Set(ids).size!==ids.length) throw new Error('Выберите от 1 до 512 разных клипов')
  const ordinary=new Set(source.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)))
  const mediaIds=ids.filter(id=>ordinary.has(id)), captionIds=ids.filter(id=>!ordinary.has(id))
  const captionTracks=captureNativeCaptionTracks(project,from,captionIds)
  const content=mediaIds.length?captureNativeClipContent(project,from,mediaIds,allocate).node:{...source,instances:[],content:{...source.content,tracks:[],metadata:{}}}
  const node={...content,content:{...content.content,captionTracks}}
  if (node.content.tracks.reduce((n,track)=>n+track.clips.length,0)+captionTracks.reduce((n,track)=>n+track.clips.length,0)>512) throw new Error('Слишком много клипов')
  // A detached JSON snapshot survives edits/deletions and does not retain Svelte proxies.
  const referenced = new Set(lutReferences(node))
  return JSON.parse(JSON.stringify({ schemaVersion: 1, root: 'clipboard', compositions: { clipboard: node }, ...(project.luts ? { luts: project.luts.filter(entry=>referenced.has(entry.id)) } : {}) })) as NativeNestedProject
}

function canonical(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`
  if (value && typeof value === 'object') return `{${Object.entries(value).sort(([a],[b])=>a.localeCompare(b)).map(([key,entry])=>`${JSON.stringify(key)}:${canonical(entry)}`).join(',')}}`
  return JSON.stringify(value)
}

function lutReferences(value: unknown): string[] {
  if (!value || typeof value !== 'object') return []
  const record=value as Record<string,unknown>, parameters=record.parameters as Record<string,unknown> | undefined
  const own=record.kind==='lumetri' && parameters ? [parameters.inputLutLibrary,parameters.lookLutLibrary].filter((entry): entry is string=>typeof entry==='string') : []
  return [...own,...Object.values(record).flatMap(lutReferences)]
}
function rebindLuts(value: unknown, mapping: ReadonlyMap<string,string>): unknown {
  if (Array.isArray(value)) return value.map(entry=>rebindLuts(entry,mapping))
  if (!value || typeof value!=='object') return value
  const record=Object.fromEntries(Object.entries(value).map(([key,entry])=>[key,rebindLuts(entry,mapping)]))
  if (record.kind==='lumetri' && record.parameters && typeof record.parameters==='object') {
    const parameters=record.parameters as Record<string,unknown>
    for (const key of ['inputLutLibrary','lookLutLibrary']) if (typeof parameters[key]==='string') parameters[key]=mapping.get(parameters[key]) ?? parameters[key]
  }
  return record
}
export function prepareNativeClipboardPaste(project: NativeNestedProject, clipboard: NativeNestedProject, to: string, startUs: number, targets: Readonly<Record<string,string>>, allocate: () => string) {
  const captured=clipboard.compositions[clipboard.root]
  if (!captured) throw new Error('Буфер клипов повреждён')
  const luts=[...(project.luts ?? [])], mapping=new Map<string,string>()
  const referenced=new Set(lutReferences(captured)), reserved=new Set([...luts.map(entry=>entry.id),...referenced])
  for (const id of referenced) {
    const entry=clipboard.luts?.find(item=>item.id===id), existing=luts.find(item=>item.id===id)
    let target=id
    if (existing && (!entry || canonical(existing.lut)!==canonical(entry.lut))) {
      target=allocate()
      if (!isStableId(target) || reserved.has(target)) throw new Error('Нужен уникальный идентификатор таблицы цвета')
      reserved.add(target);mapping.set(id,target)
    }
    if (entry && (!existing || target!==id)) luts.push({...entry,id:target})
  }
  const base: NativeNestedProject = { ...project, ...(luts.length ? { luts } : {}) }
  const node = rebindLuts(captured,mapping) as typeof captured
  const mediaStarts=node.content.tracks.flatMap(track=>track.clips.map(nativeClipStart))
  const captionStarts=(node.content.captionTracks ?? []).flatMap(track=>track.clips.map(cue=>exactCaptionTicks(cue.timelineStartTick)))
  const starts=[...mediaStarts.map(start=>BigInt(start)*UNIT),...captionStarts]
  if (!starts.length) throw new Error('Буфер клипов пуст')
  if (!Number.isSafeInteger(startUs) || startUs<0) throw new Error('Некорректное время вставки')
  const origin=Number(starts.reduce((a,b)=>a<b?a:b)/UNIT), captionDeltaUs=startUs-origin
  const captions=node.content.captionTracks ?? []
  if (!mediaStarts.length) return {staged:base,moves:[],temporary:'',copies:[],base,captions,captionDeltaUs}
  const sourceId = allocate()
  if (!isStableId(sourceId) || Object.hasOwn(project.compositions,sourceId)) throw new Error('Нужен уникальный идентификатор вставки')
  const mediaNode={...node,content:{...node.content,captionTracks:[]}}
  const combined: NativeNestedProject = { ...base, ...(project.timelineOrder ? { timelineOrder: [...orderedTimelineIds(project),sourceId] } : {}), compositions: { ...project.compositions, [sourceId]: mediaNode } }
  const ids = node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id))
  return { ...prepareNativeClipCopy(combined,sourceId,to,ids,startUs+Math.min(...mediaStarts)-origin,targets,allocate), base,captions,captionDeltaUs }
}
