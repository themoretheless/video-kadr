import { describe, expect, it } from 'vitest'
import { migrateProjectDocument } from '../project-schema'
import { instantiateProjectTemplate, migrateProjectTemplate, nextProjectTemplateRevision, type ProjectTemplate } from './project-template'

const template = (): ProjectTemplate => {
  const document = migrateProjectDocument({ videoId: 'media-old', name: 'Template', video: { id: 'media-old', duration: 2 }, edit: {} })
  document.media[0]!.contentFingerprint = 'a'.repeat(64); document.media[0]!.metadata.sizeBytes = 100
  const effect = document.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!
  effect.parameters.text = 'old'; effect.parameters.assetRef = 'logo-old'
  return { schemaVersion: 1, id: 'template-1', revision: 1, name: 'Social', sourceProjectSchemaVersion: 4, document,
    placeholders: [
      { id: 'source', name: 'Source', kind: 'media', target: { mediaId: 'media-old' }, required: true, constraints: { mediaKind: 'video' } },
      { id: 'title', name: 'Title', kind: 'text', target: { effectId: effect.id, parameter: 'text' }, required: true, constraints: { maxLength: 20 } },
      { id: 'logo', name: 'Logo', kind: 'logo', target: { effectId: effect.id, parameter: 'assetRef' }, required: false },
    ], requiredAssets: [{ kind: 'media', assetRef: 'media-old', fingerprint: 'a'.repeat(64), mimeType: 'application/octet-stream', byteLength: 100 }] }
}

describe('project template domain', () => {
  it('instantiates atomically, resolves typed bindings and rekeys every entity', () => {
    const source = template(); let n = 0
    const output = instantiateProjectTemplate(source, {
      source: { kind: 'media', media: { assetRef: 'asset-new', contentFingerprint: 'c'.repeat(64), kind: 'video', metadata: { duration: 2 } } },
      title: { kind: 'text', text: 'Hello' },
      logo: { kind: 'logo', assetRef: 'logo-new', fingerprint: 'd'.repeat(64), mimeType: 'image/png', byteLength: 10 },
    }, old => `new-${++n}-${old}`)
    expect(output.primaryMediaId).not.toBe(source.document.primaryMediaId)
    expect(output.media[0]).toMatchObject({ assetRef: 'asset-new', contentFingerprint: 'c'.repeat(64) })
    const effect = output.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!
    expect(effect.parameters).toMatchObject({ text: 'Hello', assetRef: 'logo-new', contentFingerprint: 'd'.repeat(64) })
    expect(source.document.media[0]!.assetRef).not.toBe('asset-new')
  })
  it('leaves source untouched on missing binding or duplicate generated id', () => {
    const source = template(); const before = structuredClone(source)
    expect(() => instantiateProjectTemplate(source, {}, old => `new-${old}`)).toThrow('missing placeholder binding source')
    expect(() => instantiateProjectTemplate(source, { source: { kind: 'media', media: { assetRef: 'a', contentFingerprint: 'a'.repeat(64), kind: 'video', metadata: { duration: 2 } } }, title: { kind: 'text', text: 'ok' } }, () => 'same')).toThrow('duplicate generated id same')
    expect(source).toEqual(before)
  })
  it('strictly validates allowlisted targets and future versions', () => {
    expect(() => migrateProjectTemplate({ ...template(), surprise: true })).toThrow('unexpected surprise')
    const unsafe = template() as unknown as { placeholders: Array<Record<string, unknown>> }; unsafe.placeholders[0] = { ...unsafe.placeholders[0], target: { mediaId: 'media-old', path: '__proto__' } }
    expect(() => migrateProjectTemplate(unsafe)).toThrow('unexpected path')
    expect(() => migrateProjectTemplate({ schemaVersion: 2 })).toThrow('unsupported project template schemaVersion 2')
  })
  it('creates immutable revisions without changing identity', () => {
    const source = template(); const next = nextProjectTemplateRevision(source, { name: 'Next', sourceProjectSchemaVersion: 4, document: source.document, placeholders: source.placeholders, requiredAssets: source.requiredAssets })
    expect(next).toMatchObject({ id: source.id, revision: 2, name: 'Next' }); expect(source.revision).toBe(1)
  })
  it('pins and durably applies a matching brand snapshot', () => {
    const source = template()
    const effect = source.document.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!
    effect.parameters.brandColorId = 'accent'; effect.parameters.brandFontId = 'heading'; effect.parameters.brandLogoId = 'primary-logo'
    source.brandKitPin = { id: 'brand-1', revision: 3 }
    source.brandKitSnapshot = {
      schemaVersion: 1, id: 'brand-1', revision: 3, name: 'Brand',
      colors: [{ id: 'accent', name: 'Accent', value: { space: 'srgb', rgba: [0.1, 0.2, 0.3, 1] } }],
      fonts: [{ id: 'heading', name: 'Heading', family: 'Studio', weight: 700, style: 'normal', asset: { assetRef: 'font-file', fingerprint: 'e'.repeat(64), mimeType: 'font/woff2', byteLength: 20 } }],
      logos: [{ id: 'primary-logo', name: 'Logo', variant: 'primary', asset: { assetRef: 'logo-file', fingerprint: 'f'.repeat(64), mimeType: 'image/png', byteLength: 10 } }],
      defaults: { primaryColorId: 'accent', headingFontId: 'heading', primaryLogoId: 'primary-logo' },
    }
    source.requiredAssets.push(
      { kind: 'font', assetRef: 'font-file', fingerprint: 'e'.repeat(64), mimeType: 'font/woff2', byteLength: 20 },
      { kind: 'logo', assetRef: 'logo-file', fingerprint: 'f'.repeat(64), mimeType: 'image/png', byteLength: 10 },
    )
    const output = instantiateProjectTemplate(source, {
      source: { kind: 'media', media: { assetRef: 'asset-new', contentFingerprint: 'c'.repeat(64), kind: 'video', metadata: { duration: 2 } } }, title: { kind: 'text', text: 'Hello' },
    }, old => `copy-${old}`)
    const resolved = output.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!.parameters
    expect(resolved.color).toEqual({ space: 'srgb', rgba: [0.1, 0.2, 0.3, 1] })
    expect(resolved.font).toMatchObject({ assetRef: 'font-file', family: 'Studio' })
    expect(resolved).toMatchObject({ assetRef: 'logo-file', contentFingerprint: 'f'.repeat(64) })
    expect(output.brandKitSnapshot).toEqual(source.brandKitSnapshot)
  })
})
