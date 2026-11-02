import { planConcatSourceEdit, type NativeNestedProject } from '$lib/api'
import { applyConcatSourcePart, concatSourceEditInput, type SourceClip } from './concatSourceSplit'
import { expandNativeClipLinks } from './nativeClipLinks'
import { nativeClipEditBounds, trimNativeClipEnds, trimNativeClipStarts } from './nativeClipTrim'
export type SourceTrimParts = ReadonlyMap<string, SourceClip>
/** Non-ripple source edits clamp against original neighboring document bounds. */
export async function trimNativeClipsWithSourcePlans(project: NativeNestedProject, compositionId: string, ids: readonly string[], deltaUs: number, edge: 'start' | 'end'): Promise<NativeNestedProject> {
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректный сдвиг границы')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids))
  if (!selected.size || deltaUs === 0) return project
  const node = project.compositions[compositionId]!, parts = new Map<string, SourceClip>()
  let ordinary = false, changed = false
  for (const track of node.content.tracks) for (const clip of track.clips) {
    if (!selected.has(clip.id)) continue
    if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
    if (!('placement' in clip) || !clip.placement.concatRetime) { ordinary = true; continue }
    if (!('sourceId' in clip)) throw new Error('Неподдерживаемый исходный клип Concat')
    const source = clip as SourceClip, input = concatSourceEditInput(source, node.content.canvas)
    const [start, end] = nativeClipEditBounds(source)
    let floor = 0, ceiling: number | null = null
    for (const other of track.clips) {
      if (other.id === source.id) continue
      const [a, b] = nativeClipEditBounds(other)
      if (a < end && b > start) throw new Error('Исходный клип пересекает соседний клип')
      if (b <= start) floor = Math.max(floor, b)
      if (a >= end) ceiling = ceiling === null ? a : Math.min(ceiling, a)
    }
    const plan = await planConcatSourceEdit({ ...input, command: edge === 'start'
      ? { kind: 'trimStart', delta: deltaUs / 1e6, floor, ripple: false }
      : { kind: 'trimEnd', delta: deltaUs / 1e6, ceiling, ripple: false } })
    if (!plan.changed) { parts.set(clip.id, source); continue }
    const part = plan.parts[0]
    if (plan.parts.length !== 1 || part?.role !== 'clip' || plan.rippleBy !== 0 || !part.placement.concatRetime?.documentClock) throw new Error('Некорректный план обрезки Concat')
    parts.set(clip.id, applyConcatSourcePart(source, part, source.placement.timelineStartTick - part.placement.timelineStartTick))
    changed = true
  }
  if (!ordinary && !changed) return project
  return edge === 'start' ? trimNativeClipStarts(project, compositionId, ids, deltaUs, parts) : trimNativeClipEnds(project, compositionId, ids, deltaUs, parts)
}

/** Synchronous UI preflight; source clamping and candidate validation run on submit. */
export function preflightNativeClipTrim(project: NativeNestedProject, compositionId: string, ids: readonly string[], deltaUs: number, edge: 'start' | 'end'): void {
  if (!Number.isSafeInteger(deltaUs)) throw new Error('Некорректный сдвиг границы')
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids)), node = project.compositions[compositionId]!
  let source = false
  for (const track of node.content.tracks) for (const clip of track.clips) {
    if (!selected.has(clip.id)) continue
    if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
    if ('placement' in clip && clip.placement.concatRetime) {
      if (!('sourceId' in clip)) throw new Error('Неподдерживаемый исходный клип Concat')
      concatSourceEditInput(clip as SourceClip, node.content.canvas)
      source = true
    }
  }
  if (!source) (edge === 'start' ? trimNativeClipStarts : trimNativeClipEnds)(project, compositionId, ids, deltaUs)
}
