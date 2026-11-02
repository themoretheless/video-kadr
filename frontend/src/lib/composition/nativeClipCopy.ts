import type { NativeNestedProject, NativeClipMoveResult } from '$lib/api'
import { duplicateNativeClips } from './nativeClipDuplicate'
import { nativeClipMoveBatch } from './nativeClipMove'
import { removeNativeClips } from './nativeClipDelete'
import { orderedTimelineIds } from './timelineOrder'
import { isStableId } from './validation'

/** Capture independent selected lane content without retaining nested instances/captions. */
export function captureNativeClipContent(project: NativeNestedProject, from: string, ids: readonly string[], allocate: () => string) {
  if (!project.compositions[from]) throw new Error('Таймлайн не найден')
  if (!ids.length) throw new Error('Выберите клипы для копирования')
  const old = project.compositions[from]!
  const originalIds = old.content.tracks.flatMap(track => track.clips.map(clip => clip.id))
  const duplicated = duplicateNativeClips(project, from, ids, allocate, undefined, true)
  const copies = duplicated.compositions[from]!.content.tracks.flatMap(track => track.clips.map(clip => clip.id)).filter(id => !originalIds.includes(id))
  // Removing originals can involve locked unrelated tracks. The scratch document is
  // private: unlock only its original clips, never the authored project or target.
  const scratch = { ...duplicated, compositions: { ...duplicated.compositions, [from]: { ...duplicated.compositions[from]!, content: { ...duplicated.compositions[from]!.content, tracks: duplicated.compositions[from]!.content.tracks.map(track => ({ ...track, locked: false })) } } } }
  const isolated = removeNativeClips(scratch, from, originalIds)
  const node = isolated.compositions[from]!
  return { copies, node: { ...node, instances: [], content: { ...node.content, captionTracks: [] } } }
}

/** Stage independent copies for the native cross-timeline dependency rebinder. */
export function prepareNativeClipCopy(project: NativeNestedProject, from: string, to: string, ids: readonly string[], startUs: number, targets: Readonly<Record<string,string>>, allocate: () => string) {
  if (from === to) throw new Error('Для копирования между таймлайнами выберите другой таймлайн')
  if (!project.compositions[from] || !project.compositions[to]) throw new Error('Таймлайн не найден')
  const { copies, node } = captureNativeClipContent(project, from, ids, allocate)
  const temporary = allocate()
  if (!isStableId(temporary) || Object.hasOwn(project.compositions,temporary)) throw new Error('Нужен уникальный идентификатор временного таймлайна')
  const staged: NativeNestedProject = { ...project, ...(project.timelineOrder ? { timelineOrder: [...orderedTimelineIds(project),temporary] } : {}), compositions: { ...project.compositions, [temporary]: { ...node, instances: [], content: { ...node.content, captionTracks: [] } } } }
  const moves = nativeClipMoveBatch(staged, temporary, to, copies, startUs, targets, allocate)
  return { staged, moves, temporary, copies }
}

export function finishNativeClipCopy(result: NativeClipMoveResult, temporary: string, original: NativeNestedProject, destination: string): NativeNestedProject {
  if (!result.changed || !result.project.compositions[destination] || destination === temporary) throw new Error('Не удалось вставить копии клипов')
  // Native serde may normalize untouched fields; commit only the destination.
  return { ...original, compositions: { ...original.compositions, [destination]: result.project.compositions[destination]! } }
}
