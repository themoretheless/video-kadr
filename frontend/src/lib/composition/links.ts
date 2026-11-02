import { assertCompositionLocksPreserved } from './locks.js'
import type { Composition } from './types.js'
import { assertValidComposition, isStableId } from './validation.js'

export function linkedClipIds(document: Composition, clipId: string): readonly string[] {
  return document.links?.find((link) => link.memberIds.includes(clipId))?.memberIds ?? [clipId]
}

export function compositionLinkAccent(id: string, disabled = false): string {
  let hash = 2166136261
  for (let index = 0; index < id.length; index++) hash = Math.imul(hash ^ id.charCodeAt(index), 16777619)
  return `hsl(${(hash >>> 0) % 360} 65% 55% / ${disabled ? 0.4 : 1})`
}

export function compositionLinkToggleState(document: Composition, selectedIds: readonly string[]): { mode: 'create' | 'remove' | 'unavailable'; linkId?: string; reason?: string } {
  if (!selectedIds.length) return { mode: 'unavailable', reason: 'Сначала выберите клипы' }
  const links = document.links ?? []
  const common = links.find((link) => selectedIds.every((id) => link.memberIds.includes(id)))
  const subjects = common?.memberIds ?? selectedIds
  for (const id of subjects) {
    const track = document.tracks.find((track) => track.clips.some((clip) => clip.id === id))
    if (!track) return { mode: 'unavailable', reason: 'Выбранный клип отсутствует' }
    if (track.clips.find((clip) => clip.id === id)?.locked) return { mode: 'unavailable', reason: 'Клип участника заблокирован' }
    if (track.locked) return { mode: 'unavailable', reason: 'Дорожка участника заблокирована' }
  }
  if (common) return { mode: 'remove', linkId: common.id }
  if (selectedIds.length < 2) return { mode: 'unavailable', reason: 'Для связи нужны два клипа' }
  if (links.some((link) => link.memberIds.some((id) => selectedIds.includes(id)))) return { mode: 'unavailable', reason: 'Выбраны разные связи или связанные и отдельные клипы; используйте переназначение' }
  return { mode: 'create' }
}

/** Remove vanished members and dissolve undersized links in the same mutation. */
export function pruneCompositionLinks(document: Composition): Composition {
  if (!document.links) return document
  const ids = new Set(document.tracks.flatMap((track) => track.clips.map((clip) => clip.id)))
  return { ...document, links: document.links.map((link) => ({ ...link, memberIds: link.memberIds.filter((id) => ids.has(id)) })).filter((link) => link.memberIds.length >= 2) }
}

export function createCompositionLink(document: Composition, id: string, members: readonly string[], reassign = false): Composition {
  assertValidComposition(document)
  if (!Array.isArray(members) || typeof reassign !== 'boolean') throw new Error('Invalid link members or reassign flag')
  if (!isStableId(id) || document.links?.some((link) => link.id === id)) throw new Error('Link id must be unique and stable')
  if (members.length < 2 || new Set(members).size !== members.length) throw new Error('Link requires at least two distinct clips')
  for (const member of members) {
    const track = document.tracks.find((track) => track.clips.some((clip) => clip.id === member))
    if (!track) throw new Error(`Missing linked clip ${member}`)
    if (track.locked || track.clips.find((clip) => clip.id === member)?.locked) throw new Error('Linked member is locked')
    if (!reassign && (document.links ?? []).some((link) => link.memberIds.includes(member))) throw new Error('Clip is already linked; reassign is required')
  }
  const links = (document.links ?? []).map((link) => ({ ...link, memberIds: link.memberIds.filter((member) => !members.includes(member)) })).filter((link) => link.memberIds.length >= 2)
  const result = { ...document, links: [...links, { id, memberIds: [...members] }] }
  assertValidComposition(result)
  assertCompositionLocksPreserved(document, result)
  return result
}

export function removeCompositionLink(document: Composition, id: string): Composition {
  assertValidComposition(document)
  const link = document.links?.find((link) => link.id === id)
  if (!link) throw new Error('Missing link')
  if (document.tracks.some((track) => track.clips.some((clip) => (track.locked || clip.locked) && link.memberIds.includes(clip.id)))) throw new Error('Linked member track is locked')
  return { ...document, links: document.links!.filter((link) => link.id !== id) }
}
