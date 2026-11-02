import { planConcatSourceEdit, type NativeNestedProject } from '$lib/api'
import { applyConcatSourcePart, concatSourceEditInput, type SourceClip } from './concatSourceSplit'
import { duplicateNativeClips } from './nativeClipDuplicate'
import { expandNativeClipLinks } from './nativeClipLinks'
import type { WireCompositionTrack } from './types'
/** Copies are private until every retained document clock has been relocated. */
export async function duplicateNativeClipsWithSourcePlans(project: NativeNestedProject, compositionId: string, ids: readonly string[], allocate: () => string, startUs?: number): Promise<NativeNestedProject> {
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids)), node = project.compositions[compositionId]!
  for (const track of node.content.tracks) for (const clip of track.clips) if (selected.has(clip.id) && 'placement' in clip && clip.placement.concatRetime) {
    if (!('sourceId' in clip)) throw new Error('Неподдерживаемый исходный клип Concat')
    concatSourceEditInput(clip as SourceClip, node.content.canvas)
  }
  const origins = new Map<string, SourceClip>()
  const candidate = duplicateNativeClips(project, compositionId, ids, allocate, startUs, false, (original, copyId) => {
    if ('placement' in original && original.placement.concatRetime) origins.set(copyId, original as SourceClip)
  })
  if (!origins.size) return candidate
  const replacements = new Map<string, SourceClip>()
  for (const track of candidate.compositions[compositionId]!.content.tracks) for (const clip of track.clips) {
    const original = origins.get(clip.id)
    if (!original || !('placement' in clip)) continue
    const plan = await planConcatSourceEdit({ ...concatSourceEditInput(original, node.content.canvas), command: { kind: 'move', startTick: clip.placement.timelineStartTick } })
    const part = plan.parts[0]
    if (plan.parts.length !== 1 || part?.role !== 'clip' || plan.rippleBy !== 0 || !part.placement.concatRetime?.documentClock || part.placement.timelineStartTick !== clip.placement.timelineStartTick) throw new Error('Некорректный план позиции копии Concat')
    replacements.set(clip.id, applyConcatSourcePart(clip as SourceClip, part, 0))
  }
  const copied = candidate.compositions[compositionId]!
  const tracks = copied.content.tracks.map(track => ({ ...track, clips: track.clips.map(clip => replacements.get(clip.id) ?? clip) }) as WireCompositionTrack)
  return { ...candidate, compositions: { ...candidate.compositions, [compositionId]: { ...copied, content: { ...copied.content, tracks } } } }
}
