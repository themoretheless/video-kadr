import type { WireComposition } from './types'
import { isStableId } from './validation'

/** Explicit source-lane membership. Track names/ID spelling never imply a group. */
export function nativeLaneMembers(content: Pick<WireComposition, 'tracks' | 'metadata'>, id: string): readonly string[] {
  const used = new Set<string>()
  let selected: readonly string[] = [id]
  for (const [owner, members] of Object.entries(content.metadata?.concatLaneGroups ?? {})) {
    if (!isStableId(owner) || !Array.isArray(members) || members.length !== 2 || members[0] !== owner || new Set(members).size !== 2) throw new Error('Некорректная связь дорожек Concat')
    const video = content.tracks.findIndex(track => track.id === owner)
    const image = content.tracks.findIndex(track => track.id === members[1])
    if (video < 0 || image + 1 !== video || content.tracks[video]?.kind !== 'video' || content.tracks[image]?.kind !== 'image') throw new Error('Связанные дорожки Concat отсутствуют или изменили порядок')
    for (const member of members) {
      if (!isStableId(member) || used.has(member)) throw new Error('Дорожка Concat принадлежит нескольким группам')
      used.add(member)
    }
    if (members.includes(id)) selected = members
  }
  return selected
}
