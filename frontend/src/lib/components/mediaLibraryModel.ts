import type { MediaEntry } from '$lib/types.js'

export const MAX_LIBRARY_TITLE_CHARS = 120
export const MAX_LIBRARY_TAGS = 20
export const MAX_LIBRARY_TAG_CHARS = 32

export interface LibraryFilters {
  query: string
  favoritesOnly: boolean
}

function normalized(value: string): string {
  return value.normalize('NFKC').toLocaleLowerCase()
}

function unicodeLength(value: string): number {
  return [...value].length
}

function searchableMediaType(entry: MediaEntry): string {
  if (entry.mediaType) return entry.mediaType
  const extension = entry.filename.split('.').pop()?.toLocaleLowerCase() ?? ''
  if (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(extension)) return 'image'
  if (['aac', 'flac', 'm4a', 'mp3', 'ogg', 'opus', 'wav'].includes(extension)) return 'audio'
  if (['avi', 'm4v', 'mkv', 'mov', 'mp4', 'webm'].includes(extension)) return 'video'
  return (entry.width ?? 0) > 0 && (entry.height ?? 0) > 0 ? 'video' : ''
}

/** Match every whitespace-delimited search term against user-visible metadata. */
export function libraryEntryMatches(entry: MediaEntry, query: string): boolean {
  const terms = normalized(query).split(/\s+/u).filter(Boolean)
  if (terms.length === 0) return true
  const searchable = normalized(
    [entry.filename, entry.title ?? '', ...(entry.tags ?? []), searchableMediaType(entry), entry.kind].join(' '),
  )
  return terms.every((term) => searchable.includes(term))
}

export function filterLibraryEntries(
  entries: readonly MediaEntry[],
  filters: LibraryFilters,
): MediaEntry[] {
  return entries.filter(
    (entry) =>
      (!filters.favoritesOnly || entry.favorite === true) &&
      libraryEntryMatches(entry, filters.query),
  )
}

export function validateLibraryTitle(value: string): string | null {
  const title = value.trim()
  if (title.length === 0) return null
  if (unicodeLength(title) > MAX_LIBRARY_TITLE_CHARS || /\p{Cc}/u.test(title)) {
    throw new Error(`Название: максимум ${MAX_LIBRARY_TITLE_CHARS} символов без управляющих знаков`)
  }
  return title
}

/** Parse the comma-separated editor value into the backend's ordered tag array. */
export function parseLibraryTags(value: string): string[] {
  if (value.trim().length === 0) return []
  const tags = value.split(',').map((tag) => tag.trim())
  if (tags.some((tag) => tag.length === 0)) {
    throw new Error('Пустой тег между запятыми недопустим')
  }
  if (tags.length > MAX_LIBRARY_TAGS) {
    throw new Error(`Разрешено не больше ${MAX_LIBRARY_TAGS} тегов`)
  }
  const seen = new Set<string>()
  for (const tag of tags) {
    if (unicodeLength(tag) > MAX_LIBRARY_TAG_CHARS || /\p{Cc}/u.test(tag)) {
      throw new Error(`Тег: максимум ${MAX_LIBRARY_TAG_CHARS} символа без управляющих знаков`)
    }
    const key = normalized(tag)
    if (seen.has(key)) throw new Error(`Тег «${tag}» указан повторно`)
    seen.add(key)
  }
  return tags
}
