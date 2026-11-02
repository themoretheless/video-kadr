import { findClipLocation, splitClip } from './commands.js'
import { linkedClipIds } from './links.js'
import { clipEndTicks, type Composition } from './types.js'
import { isSafeTick } from './validation.js'

export function splitAllSubjects(document: Composition, at: number): string[] {
  if (!isSafeTick(at)) throw new Error('Некорректная позиция разреза')
  return document.tracks.flatMap(track => track.clips
    .filter(clip => clip.timelineStartTicks < at && at < clipEndTicks(clip))
    .map(clip => clip.id))
}

export function splitAllReason(document: Composition, at: number): string | null {
  const subjects = splitAllSubjects(document, at)
  if (!subjects.length) return 'Под плейхедом нет клипов для разреза'
  const members = new Set(subjects.flatMap(id => linkedClipIds(document, id)))
  for (const id of members) {
    const { track, clip } = findClipLocation(document, id)
    if (track.locked || clip.locked) return 'Клип или связанная дорожка заблокированы'
  }
  return null
}

/** Stage every canonical split before committing once; linked partners are cut only once. */
export function splitAllAtTick(document: Composition, at: number, mint: () => string): Composition {
  const subjects = splitAllSubjects(document, at)
  if (!subjects.length) return document
  const reason = splitAllReason(document, at)
  if (reason) throw new Error(reason)
  let staged = document
  for (const id of subjects) {
    const { clip } = findClipLocation(staged, id)
    // A preceding canonical split may already have cut this linked member.
    if (clip.timelineStartTicks < at && at < clipEndTicks(clip)) staged = splitClip(staged, id, at, mint())
  }
  return staged
}
