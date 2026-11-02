import type { Composition } from './types'

/** Content edit admission. History/project restoration deliberately bypasses this guard. */
export function assertCompositionLocksPreserved(before: Composition, after: Composition): void {
  for (const track of before.captionTracks ?? []) {
    if (!track.locked) continue
    const next = after.captionTracks?.find(candidate => candidate.id === track.id)
    if (!next || JSON.stringify({...track,locked:false,hidden:false}) !== JSON.stringify({...next,locked:false,hidden:false})) throw new Error(`Caption track ${track.id} is locked`)
  }

  for (const track of before.tracks) for (const clip of track.clips) {
    if (!track.locked && !clip.locked) continue
    const nextTrack = after.tracks.find((item) => item.clips.some((item) => item.id === clip.id))
    const next = nextTrack?.clips.find((item) => item.id === clip.id)
    const content = (value: unknown) => JSON.stringify(Object.fromEntries(Object.entries(value as Record<string, unknown>).filter(([key]) => key !== 'locked')))
    if (!next || nextTrack?.id !== track.id || content(clip) !== content(next)) {
      throw new Error(`Clip ${clip.id} is locked; unlock it before editing`)
    }
    const survivingIds = new Set(after.tracks.flatMap((track) => track.clips.map((clip) => clip.id)))
    const membership = (document: Composition) => (document.links ?? []).map((link) => ({ ...link, memberIds: link.memberIds.filter((id) => survivingIds.has(id)) })).filter((link) => link.memberIds.length >= 2 && link.memberIds.includes(clip.id))
    const transitions = (document: Composition) => document.tracks.flatMap((item) => item.kind === 'video' ? (item.transitions ?? []).filter((transition) => transition.fromClipId === clip.id || transition.toClipId === clip.id) : [])
    if (JSON.stringify(membership(before)) !== JSON.stringify(membership(after)) || JSON.stringify(transitions(before)) !== JSON.stringify(transitions(after))) {
      throw new Error(`Clip ${clip.id} is locked; its links and transitions cannot change`)
    }
    if ('sourceId' in clip && JSON.stringify(before.sources[clip.sourceId]) !== JSON.stringify(after.sources[clip.sourceId])) {
      throw new Error(`Clip ${clip.id} is locked; its source cannot change`)
    }
  }
}
