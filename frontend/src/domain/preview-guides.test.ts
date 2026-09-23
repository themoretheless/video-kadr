import { describe, expect, it } from 'vitest'

import {
  PREVIEW_GUIDES,
  isPreviewGuide,
  loadEnabledPreviewGuides,
  saveEnabledPreviewGuides,
  type PreviewGuide,
} from './preview-guides'

function memoryStorage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial))
  return {
    getItem: (key: string) => data.get(key) ?? null,
    setItem: (key: string, value: string) => { data.set(key, value) },
    removeItem: (key: string) => { data.delete(key) },
    data,
  }
}

describe('preview guides persistence', () => {
  it('exposes the four guide toggles in display order', () => {
    expect(PREVIEW_GUIDES.map((guide) => guide.key)).toEqual(['thirds', 'center', 'action', 'title'])
  })

  it('loads only known guides and tolerates broken storage payloads', () => {
    expect(loadEnabledPreviewGuides(memoryStorage({ ve_guides: JSON.stringify(['thirds', 'title', 'zoom']) })))
      .toEqual(['thirds', 'title'])
    expect(loadEnabledPreviewGuides(memoryStorage({ ve_guides: 'not json' }))).toEqual([])
    expect(loadEnabledPreviewGuides(memoryStorage())).toEqual([])
    expect(loadEnabledPreviewGuides(memoryStorage({ ve_guides: '{"a":1}' }))).toEqual([])
  })

  it('round-trips the active set and clears the key when nothing is active', () => {
    const storage = memoryStorage()
    saveEnabledPreviewGuides(storage, ['action', 'center'])
    expect(storage.data.get('ve_guides')).toBe(JSON.stringify(['action', 'center']))
    expect(loadEnabledPreviewGuides(storage)).toEqual(['action', 'center'])
    saveEnabledPreviewGuides(storage, [])
    expect(storage.data.has('ve_guides')).toBe(false)
  })

  it('narrows unknown values to guide keys', () => {
    const values: unknown[] = ['thirds', 'center', 'action', 'title', 'safe', null, 7]
    expect(values.filter(isPreviewGuide)).toEqual(['thirds', 'center', 'action', 'title'])
    const guide: PreviewGuide = 'title'
    expect(guide).toBe('title')
  })
})
