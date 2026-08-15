import { beforeEach, describe, expect, it } from 'vitest'
import { MAX_COMPOSITION_TEMPLATE_BYTES } from '../composition/templates'
import type { CompositionClip } from '../composition/types'
import {
  addMediaInfoToComposition,
  addTextToComposition,
  compositionState,
  resetCompositionForTests,
} from './composition.svelte.js'
import {
  compositionTemplateState,
  createTemplateFromCurrentComposition,
  exportTemplateFromCatalog,
  importTemplateToCatalog,
  instantiateCatalogTemplate,
  reloadCompositionTemplateCatalog,
  resetCompositionTemplateCatalogForTests,
  setCompositionTemplateStorageForTests,
  sourceCanReplaceTemplateSlot,
} from './compositionTemplates.svelte.js'

beforeEach(() => {
  const values = new Map<string, string>()
  setCompositionTemplateStorageForTests({
    get length() { return values.size },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => { values.delete(key) },
    setItem: (key, value) => { values.set(key, value) },
  })
  resetCompositionForTests()
  resetCompositionTemplateCatalogForTests()
  addMediaInfoToComposition({
    id: 'template-video',
    url: '/files/sources/template.mp4',
    filename: 'template.mp4',
    mediaType: 'video',
    duration: 6,
    width: 1280,
    height: 720,
    acodec: 'aac',
  })
  addTextToComposition('Placeholder')
})

describe('local composition template catalog', () => {
  it('creates deterministic media/text slots and instantiates every required replacement', () => {
    const template = createTemplateFromCurrentComposition('Promo')
    expect(template.slots.map((slot) => [slot.id, slot.kind])).toEqual([
      ['slot-001', 'media'],
      ['slot-002', 'text'],
    ])

    const mediaSlot = template.slots.find((slot) => slot.kind === 'media')!
    const textSlot = template.slots.find((slot) => slot.kind === 'text')!
    const replacement = {
      id: 'replacement-video',
      kind: 'video' as const,
      durationTicks: 8_000_000,
      width: 1920,
      height: 1080,
      hasAudio: true,
    }
    expect(sourceCanReplaceTemplateSlot(template, mediaSlot, replacement)).toBe(true)

    // Applying a template is destructive to the active new draft; exercise it
    // from a clean slot while the dirty-draft guard is covered by state/UI tests.
    resetCompositionForTests()
    instantiateCatalogTemplate(template.id, {
      [mediaSlot.id]: { source: replacement },
      [textSlot.id]: { text: 'Launch day' },
    })

    expect(compositionState.projectId).toBeNull()
    expect(Object.keys(compositionState.document.sources)).toEqual(['replacement-video'])
    expect(compositionState.document.tracks.flatMap((track) => [...track.clips] as CompositionClip[])).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ sourceId: 'replacement-video' }),
        expect.objectContaining({ text: 'Launch day' }),
      ]),
    )
  })

  it('round-trips catalog JSON and rejects malformed or oversized imports before storage', () => {
    const template = createTemplateFromCurrentComposition('Reusable')
    const exported = exportTemplateFromCatalog(template.id)
    compositionTemplateState.templates = []
    reloadCompositionTemplateCatalog()
    expect(compositionTemplateState.templates).toEqual([template])
    resetCompositionTemplateCatalogForTests()

    expect(importTemplateToCatalog(exported.text)).toEqual(template)
    expect(compositionTemplateState.templates).toHaveLength(1)
    expect(exported.text).not.toContain('/files/sources/')
    expect(() => importTemplateToCatalog('{bad json')).toThrow('valid JSON')
    expect(() => importTemplateToCatalog('x'.repeat(MAX_COMPOSITION_TEMPLATE_BYTES + 1))).toThrow('limit')
  })
})
