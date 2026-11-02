import { planConcatSourceEdit, type NativeNestedProject } from '$lib/api'
import type { ConcatSourceDocumentClock, WireCompositionTrack } from './types'
import { applyConcatSourcePart, concatSourceEditInput, type SourceClip } from './concatSourceSplit'
import { expandNativeClipLinks } from './nativeClipLinks'
import { nativeClipEnd } from './nativeClipInsert'
export type SourceSpeedPoints = ConcatSourceDocumentClock['speedCurve']
export function validateSourceSpeedPoints(points: SourceSpeedPoints): void {
  if (points != null && (!Array.isArray(points) || points.length > 4096 || points.some(point => !point || typeof point !== 'object' || Object.keys(point).some(key => key !== 'at' && key !== 'speed') || !Number.isFinite(point.at) || !Number.isFinite(point.speed)))) throw new Error('Некорректные точки скорости Concat')
}
export function sourceCurveParticipants(project: NativeNestedProject, compositionId: string, ids: readonly string[]): readonly SourceClip[] {
  const selected = new Set(expandNativeClipLinks(project, compositionId, ids)), node = project.compositions[compositionId]!
  if (!selected.size) throw new Error('Выберите клипы для изменения скорости')
  const clips: SourceClip[] = []
  for (const track of node.content.tracks) for (const clip of track.clips) {
    if (!selected.has(clip.id)) continue
    if (track.locked) throw new Error('Дорожка связанного клипа заблокирована')
    if (!('placement' in clip) || !clip.placement.concatRetime || !('sourceId' in clip)) throw new Error('Выберите клипы с исходными часами Concat')
    const source = clip as SourceClip
    concatSourceEditInput(source, node.content.canvas)
    clips.push(source)
  }
  return clips
}
/** Set-curve stretches normalized source keys; it does not rewindow or ripple. */
export async function setNativeSourceSpeedCurve(project: NativeNestedProject, compositionId: string, ids: readonly string[], points: SourceSpeedPoints): Promise<NativeNestedProject> {
  validateSourceSpeedPoints(points)
  // Snapshot mutable caller input before the first asynchronous request.
  const captured = points?.map(point => ({ at: point.at, speed: point.speed })) ?? null
  const clips = sourceCurveParticipants(project, compositionId, ids), node = project.compositions[compositionId]!, replacements = new Map<string, SourceClip>()
  for (const clip of clips) {
    const input = concatSourceEditInput(clip, node.content.canvas)
    const plan = await planConcatSourceEdit({ ...input, command: { kind: 'setCurve', points: captured } })
    if (!plan.changed) continue
    const part = plan.parts[0]
    if (plan.parts.length !== 1 || part?.role !== 'clip' || plan.rippleBy !== 0 || !part.placement.concatRetime?.documentClock) throw new Error('Некорректный план скорости Concat')
    replacements.set(clip.id, applyConcatSourcePart(clip, part, 0))
  }
  if (!replacements.size) return project
  const tracks = node.content.tracks.map(track => ({ ...track, clips: track.clips.map(clip => replacements.get(clip.id) ?? clip) }) as WireCompositionTrack)
  const changed = { ...node, content: { ...node.content, tracks } }
  if (node.durationUs !== undefined) changed.durationUs = Math.max(node.durationUs, ...tracks.flatMap(track => track.clips.map(nativeClipEnd)))
  return { ...project, compositions: { ...project.compositions, [compositionId]: changed } }
}
