import type { NativeNestedProject } from '$lib/api'
import { isStableId } from './validation'
export function expandNativeClipLinks(project: NativeNestedProject, compositionId: string, ids: readonly string[]): string[] {
  const node = project.compositions[compositionId]
  if (!node || !Object.hasOwn(project.compositions, compositionId)) throw new Error('Таймлайн не найден')
  const known = new Set(node.content.tracks.flatMap(track => track.clips.map(clip => clip.id)))
  const selected = new Set(ids)
  if (ids.some(id => !known.has(id))) throw new Error('Выбранный клип не найден')
  for (const members of Object.values(node.content.metadata?.clipLinks ?? {})) {
    if (!members.some(id => selected.has(id))) continue
    for (const id of members) { if (!known.has(id)) throw new Error('Связанный клип не найден'); selected.add(id) }
  }
  return [...selected]
}
export function linkNativeClips(project: NativeNestedProject, compositionId: string, ids: readonly string[]): NativeNestedProject {
  const node = project.compositions[compositionId]
  if (!node || ids.length < 2 || ids.length > 512 || new Set(ids).size !== ids.length) throw new Error('Для связи выберите разные клипы')
  if (expandNativeClipLinks(project, compositionId, ids).length !== ids.length || Object.values(node.content.metadata?.clipLinks ?? {}).some(members => members.some(id => ids.includes(id)))) throw new Error('Клип уже связан с другими клипами')
  assertLinkTracksUnlocked(project, compositionId, ids)
  const groups = node.content.metadata?.clipLinks ?? {}
  const base = `${ids[0]!.slice(0, 110)}-link`
  let group = base, serial = 0
  while (Object.hasOwn(groups, group)) group = `${base}-${serial++}`
  if (!isStableId(group)) throw new Error('Некорректный идентификатор связи')
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, metadata: { ...node.content.metadata, clipLinks: { ...groups, [group]: [...ids] } } } } } }
}

function assertLinkTracksUnlocked(project: NativeNestedProject, compositionId: string, ids: readonly string[]): void {
  const selected = new Set(ids)
  if (project.compositions[compositionId]!.content.tracks.some(track => track.locked && track.clips.some(clip => selected.has(clip.id)))) throw new Error('Дорожка связанного клипа заблокирована')
}
export function unlinkNativeClips(project: NativeNestedProject, compositionId: string, ids: readonly string[]): NativeNestedProject {
  const members = expandNativeClipLinks(project, compositionId, ids)
  assertLinkTracksUnlocked(project, compositionId, members)
  const node = project.compositions[compositionId]!
  const groups = node.content.metadata?.clipLinks ?? {}
  const selected = new Set(ids)
  const retained = Object.fromEntries(Object.entries(groups).filter(([, group]) => !group.some(id => selected.has(id))))
  if (Object.keys(retained).length === Object.keys(groups).length) return project
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, metadata: { ...node.content.metadata, clipLinks: retained } } } } }
}

/** Suppress link expansion for one private operation, then retain authored groups. */
export function prepareNativeLinkEscape(project:NativeNestedProject,compositionId:string,ids:readonly string[]){
 const node=project.compositions[compositionId]
 if(!Object.hasOwn(project.compositions,compositionId)||!node)throw new Error('Composition not found')
 const groups=node.content.metadata?.clipLinks??{},selected=new Set(ids)
 const escaped=Object.entries(groups).filter(([,members])=>members.some(id=>selected.has(id)))
 if(!escaped.length)return {project,restore:(value:NativeNestedProject)=>value}
 const scratch={...project,compositions:{...project.compositions,[compositionId]:{...node,content:{...node.content,metadata:{...node.content.metadata,clipLinks:Object.fromEntries(Object.entries(groups).filter(([id])=>!escaped.some(([key])=>id===key)))}}}}}
 return {project:scratch,restore:(value:NativeNestedProject):NativeNestedProject=>{
  const after=value.compositions[compositionId]!
  return {...value,compositions:{...value.compositions,[compositionId]:{...after,content:{...after.content,metadata:{...after.content.metadata,clipLinks:{...groups,...after.content.metadata?.clipLinks,...Object.fromEntries(escaped)}}}}}}
 }}
}
