import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createProjectDocumentFromLegacy } from './project-schema'

/* eslint-disable @typescript-eslint/no-explicit-any -- isolated facade mocks intentionally accept generated domain values */

const mocks = vi.hoisted(() => ({
  templates: [] as Array<{ id: string; revision: number; createdAt: number; updatedAt: number; value: any }>,
  kits: [] as Array<{ id: string; revision: number; createdAt: number; updatedAt: number; value: any }>,
  saveTemplate: vi.fn(), saveKit: vi.fn(), putAsset: vi.fn(), open: vi.fn(),
  deleteAsset: vi.fn(), getAsset: vi.fn(),
  timeline: { document: null as any },
}))

vi.mock('./browser-design-store', () => ({
  listProjectTemplates: vi.fn(async () => mocks.templates), listBrandKits: vi.fn(async () => mocks.kits),
  getProjectTemplate: vi.fn(async (id: string) => mocks.templates.find(row => row.id === id) ?? null),
  getBrandKit: vi.fn(async (id: string) => mocks.kits.find(row => row.id === id) ?? null),
  saveProjectTemplate: mocks.saveTemplate, saveBrandKit: mocks.saveKit, putDesignAsset: mocks.putAsset,
  getDesignAsset: mocks.getAsset,
  deleteDesignAsset: mocks.deleteAsset,
  deleteProjectTemplate: vi.fn(async () => true), deleteBrandKit: vi.fn(async () => true),
}))
vi.mock('./store', () => ({ timelineState: mocks.timeline, openInstantiatedProject: mocks.open }))

describe('design store facade', () => {
  beforeEach(() => {
    vi.resetModules(); vi.clearAllMocks(); mocks.templates = []; mocks.kits = []
    mocks.getAsset.mockResolvedValue(null)
    mocks.timeline.document = createProjectDocumentFromLegacy('source', 'Source', {
      id: 'source', filename: 'source.mp4', duration: 5, width: 320, height: 180,
      assetId: 'asset-source', fingerprint: 'a'.repeat(64), mediaKind: 'video', sizeBytes: 100,
    }, {})
    mocks.saveTemplate.mockImplementation(async (value: any) => {
      const row = { id: value.id, revision: value.revision, createdAt: 1, updatedAt: 1, value }
      mocks.templates.push(row); return row
    })
    mocks.saveKit.mockImplementation(async (value: any) => {
      const row = { id: value.id, revision: value.revision, createdAt: 1, updatedAt: 1, value }
      mocks.kits.push(row); return row
    })
  })

  it('saves an immutable current-project template and instantiates disjoint IDs atomically', async () => {
    const design = await import('./design-store')
    await design.saveCurrentProjectTemplate('Launch', [{ label: 'Hero', kind: 'video', required: true }])
    expect(design.designState.templates).toHaveLength(1)
    const template = design.designState.templates[0]!
    expect(template.placeholders[0]).toMatchObject({ name: 'Hero', kind: 'media', required: true })
    await design.instantiateTemplate(template.id)
    expect(mocks.open).toHaveBeenCalledOnce()
    const opened = mocks.open.mock.calls[0]![0]
    expect(opened.primaryMediaId).not.toBe(template.document.primaryMediaId)
    expect(opened.media[0]).toMatchObject({ assetRef: 'asset-source', contentFingerprint: 'a'.repeat(64) })
  })

  it('normalizes canonical sRGB colors into a revisioned brand kit', async () => {
    const design = await import('./design-store')
    await design.createBrandKit({ name: 'Studio', colors: [{ name: 'Accent', value: '#336699' }], fontFiles: [], logoFiles: [] })
    expect(design.designState.kits[0]).toMatchObject({ name: 'Studio', revision: 1 })
    expect(design.designState.kits[0]!.colors[0]!.value).toEqual({ space: 'srgb', rgba: [0.2, 0.4, 0.6, 1] })
  })

  it('rolls back uploaded assets when saving a brand kit fails', async () => {
    mocks.putAsset.mockResolvedValue({ id: 'stored', kind: 'font', name: 'Brand.woff2', mimeType: 'font/woff2', byteLength: 4, fingerprint: 'b'.repeat(64), createdAt: 1 })
    mocks.saveKit.mockRejectedValueOnce(new Error('quota'))
    const design = await import('./design-store')
    await design.createBrandKit({ name: 'Studio', colors: [], fontFiles: [new File(['wOF2'], 'Brand.woff2')], logoFiles: [] })
    expect(mocks.deleteAsset).toHaveBeenCalledOnce()
    expect(design.designState.error).toBe('quota')
  })

  it('passes only strict BrandAssetMetadata to the domain persistence boundary', async () => {
    mocks.putAsset.mockResolvedValue({ id: 'stored', kind: 'font', name: 'Brand.woff2', mimeType: 'font/woff2', byteLength: 4, fingerprint: 'b'.repeat(64), createdAt: 1 })
    const design = await import('./design-store')
    await design.createBrandKit({ name: 'Studio', colors: [], fontFiles: [new File(['wOF2'], 'Brand.woff2')], logoFiles: [] })
    const saved = mocks.saveKit.mock.calls[0]![0]
    expect(saved.fonts[0]!.asset).toEqual({ assetRef: 'stored', fingerprint: 'b'.repeat(64), mimeType: 'font/woff2', byteLength: 4 })
  })

  it('blocks instantiation before open when a required design asset is unavailable', async () => {
    const design = await import('./design-store')
    await design.saveCurrentProjectTemplate('Launch', [{ label: 'Hero', kind: 'video', required: true }])
    design.designState.templates[0]!.requiredAssets.push({ kind: 'logo', assetRef: 'logo-missing', fingerprint: 'f'.repeat(64), mimeType: 'image/png', byteLength: 10 })
    await design.instantiateTemplate(design.designState.templates[0]!.id)
    expect(mocks.open).not.toHaveBeenCalled()
    expect(design.designState.error).toContain('logo-missing')
  })
})
