import { describe, expect, it } from 'vitest'

import type { MediaEntry } from '$lib/types.js'
import {
  filterLibraryEntries,
  libraryEntryMatches,
  parseLibraryTags,
  validateLibraryTitle,
} from './mediaLibraryModel.js'

function entry(overrides: Partial<MediaEntry> = {}): MediaEntry {
  return {
    id: 'voice-1',
    kind: 'source',
    filename: 'Interview.WEBM',
    url: '/files/sources/Interview.WEBM',
    mediaType: 'audio',
    title: 'Morning Voiceover',
    favorite: true,
    tags: ['Client A', 'Draft'],
    createdAt: 1,
    ...overrides,
  }
}

describe('media library filtering', () => {
  it('searches filename, title, tags and media type case-insensitively', () => {
    const item = entry()
    expect(libraryEntryMatches(item, 'interview')).toBe(true)
    expect(libraryEntryMatches(item, 'MORNING')).toBe(true)
    expect(libraryEntryMatches(item, 'client draft')).toBe(true)
    expect(libraryEntryMatches(item, 'AUDIO')).toBe(true)
    expect(libraryEntryMatches(item, 'video')).toBe(false)
    expect(libraryEntryMatches(entry({ mediaType: null, filename: 'legacy.wav' }), 'audio')).toBe(true)
  })

  it('combines favorite and text filters without changing input order', () => {
    const items = [
      entry({ id: 'one' }),
      entry({ id: 'two', favorite: false, title: 'Other' }),
      entry({ id: 'three', title: 'Interview select' }),
    ]
    expect(filterLibraryEntries(items, { query: 'interview', favoritesOnly: true }).map(({ id }) => id))
      .toEqual(['one', 'three'])
  })
})

describe('media library metadata input', () => {
  it('trims a title and treats a blank title as clearing the override', () => {
    expect(validateLibraryTitle('  Select  ')).toBe('Select')
    expect(validateLibraryTitle('   ')).toBeNull()
    expect(validateLibraryTitle('🎬'.repeat(120))).toBe('🎬'.repeat(120))
    expect(() => validateLibraryTitle('x'.repeat(121))).toThrow('максимум 120')
  })

  it('parses ordered tags and rejects empty, duplicate and oversized input', () => {
    expect(parseLibraryTags(' client, final ')).toEqual(['client', 'final'])
    expect(parseLibraryTags('')).toEqual([])
    expect(() => parseLibraryTags('client,,final')).toThrow('Пустой тег')
    expect(() => parseLibraryTags('Client,client')).toThrow('повторно')
    expect(() => parseLibraryTags('x'.repeat(33))).toThrow('максимум 32')
  })
})
