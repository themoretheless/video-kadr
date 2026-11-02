import { expandNativeClipLinks } from './nativeClipLinks'
import type { WireCompositionTrack } from './types'
import type { NativeNestedProject } from '$lib/api'
export function removeNativeClips(project: NativeNestedProject, compositionId: string, ids: readonly string[]): NativeNestedProject {
  const node = project.compositions[compositionId]
  if (!Object.hasOwn(project.compositions, compositionId) || !node) throw new Error('Таймлайн не найден')
  if (!ids.length) return project
  if (new Set(ids).size !== ids.length || ids.length > 512) throw new Error('Выберите от 1 до 512 разных клипов')
  const captionIds=new Set((node.content.captionTracks ?? []).flatMap(track=>track.clips.map(cue=>cue.id)))
  const selected = new Set([...expandNativeClipLinks(project, compositionId, ids.filter(id=>!captionIds.has(id))),...ids.filter(id=>captionIds.has(id))])
  if (selected.size > 512) throw new Error('Выберите от 1 до 512 разных клипов')
  const found = new Set<string>()
  for (const track of node.content.tracks) for (const clip of track.clips) if (selected.has(clip.id)) {
    if (track.locked) throw new Error('Дорожка выбранного клипа заблокирована')
    found.add(clip.id)
  }
  for (const track of node.content.captionTracks ?? []) for (const cue of track.clips) if (selected.has(cue.id)) {
    if (track.locked) throw new Error('Дорожка субтитров заблокирована')
    found.add(cue.id)
  }
  if (found.size !== selected.size) throw new Error('Выбранный клип не найден')
  const tracks = node.content.tracks.map(track => {
    if (!track.clips.some(clip => selected.has(clip.id))) return track
    if (track.kind === 'video') return { ...track, clips: track.clips.filter(clip => !selected.has(clip.id)), transitions: track.transitions.filter(transition => !(transition.fromClipId && selected.has(transition.fromClipId)) && !(transition.toClipId && selected.has(transition.toClipId))) }
    if (track.kind === 'audio') {
      const ordered = [...track.clips].sort((a, b) => a.placement.timelineStartTick - b.placement.timelineStartTick || (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
      const previous = new Map(ordered.map((clip, index) => [clip.id, ordered[index - 1]?.id]))
      return { ...track, clips: track.clips.filter(clip => !selected.has(clip.id)).map(clip => clip.crossfadeInTicks > 0 && selected.has(previous.get(clip.id) ?? '') ? { ...clip, crossfadeInTicks: 0 } : clip) }
    }
    return { ...track, clips: track.clips.filter(clip => !selected.has(clip.id)) } as WireCompositionTrack
  })
  const metadata = node.content.metadata
  return { ...project, compositions: { ...project.compositions, [compositionId]: { ...node, content: { ...node.content, tracks, ...(node.content.captionTracks ? {captionTracks:node.content.captionTracks.map(track=>({...track,clips:track.clips.filter(cue=>!selected.has(cue.id))}))} : {}), ...(metadata ? { metadata: { ...metadata, ...(metadata.clipLinks ? { clipLinks: Object.fromEntries(Object.entries(metadata.clipLinks).filter(([, members]) => !members.some(id => selected.has(id)))) } : {}), clipNames: Object.fromEntries(Object.entries(metadata.clipNames ?? {}).filter(([id]) => !selected.has(id))), mediaVersions: Object.fromEntries(Object.entries(metadata.mediaVersions ?? {}).filter(([id]) => !selected.has(id))) } } : {}) } } } }
}
