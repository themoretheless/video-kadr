import { beforeEach, describe, expect, it, vi } from 'vitest'
import { sha256 } from '@noble/hashes/sha2.js'
import type { ProjectDto } from './api'
import { createProjectDocumentFromLegacy } from './project-schema'

vi.mock('./browser-asset-store', () => ({
  auditBrowserAssets: vi.fn(async () => []),
  deleteBrowserAsset: vi.fn(async () => undefined),
  prepareBrowserStorage: vi.fn(async () => ({ persisted: true, usage: 0, quota: 1_000_000 })),
  putBrowserAsset: vi.fn(async (asset: { id: string }) => ({ id: asset.id })),
}))
vi.mock('./browser-media', () => ({ readSourceForArchive: vi.fn() }))
vi.mock('./browser-project-store', () => ({
  putProject: vi.fn(async () => undefined),
}))

import { auditBrowserAssets, prepareBrowserStorage, putBrowserAsset } from './browser-asset-store'
import { readSourceForArchive } from './browser-media'
import { putProject } from './browser-project-store'
import { exportBrowserProjectArchive, importBrowserProjectArchive } from './browser-project-archive'

function digest(value: string): string {
  return Array.from(sha256(new TextEncoder().encode(value)), byte => byte.toString(16).padStart(2, '0')).join('')
}

function fixture(content = 'source'): ProjectDto {
  const fingerprint = digest(content)
  const video = {
    id: 'asset-1', assetId: 'asset-1', url: 'blob:runtime', filename: 'clip.mp4',
    duration: 1, width: 16, height: 16, fingerprint,
  }
  return {
    id: 'project-1', name: 'Portable', videoId: 'asset-1', video, edit: {}, revision: 3,
    createdAt: 10, updatedAt: 20,
    document: createProjectDocumentFromLegacy('asset-1', 'Portable', video, {}),
  }
}

describe('browser portable project archive', () => {
  beforeEach(() => vi.clearAllMocks())

  it('exports and imports project-only as an offline placeholder without touching media bytes', async () => {
    const exported = await exportBrowserProjectArchive(fixture(), {
      includeOriginalMedia: false,
      includeProxies: false,
    })
    expect(exported.warnings.join(' ')).toContain('relink')

    const imported = await importBrowserProjectArchive(exported.blob)
    expect(imported.missingMedia).toEqual(['clip.mp4'])
    expect(putBrowserAsset).not.toHaveBeenCalled()
    expect(putProject).toHaveBeenCalledOnce()
    expect(imported.project.id).not.toBe('project-1')
    expect(imported.project.document?.media[0]!.assetRef).not.toBe('asset-1')
    expect(imported.project.video.availability).toBe('offline')
  })

  it('round-trips included originals and preflights browser storage before commit', async () => {
    const project = fixture()
    const file = new File(['source'], 'clip.mp4', { type: 'video/mp4' })
    vi.mocked(readSourceForArchive).mockResolvedValue({
      file, filename: file.name, fileType: file.type, fingerprint: project.video.fingerprint!,
      info: { ...project.video, url: undefined } as never,
    })
    const exported = await exportBrowserProjectArchive(project, {
      includeOriginalMedia: true,
      includeProxies: false,
    })
    const imported = await importBrowserProjectArchive(exported.blob)
    expect(imported.importedMedia).toBe(1)
    expect(imported.missingMedia).toEqual([])
    expect(prepareBrowserStorage).toHaveBeenCalledOnce()
    expect(putBrowserAsset).toHaveBeenCalledOnce()
    expect(putProject).toHaveBeenCalledOnce()
  })

  it('reuses exact local bytes and never overwrites an asset manifest', async () => {
    const project = fixture()
    const file = new File(['source'], 'clip.mp4', { type: 'video/mp4' })
    vi.mocked(readSourceForArchive).mockResolvedValue({ file, filename: file.name, fileType: file.type, fingerprint: project.video.fingerprint!, info: project.video })
    vi.mocked(auditBrowserAssets).mockResolvedValue([{
      id: 'existing-exact', filename: 'clip.mp4', fileType: 'video/mp4', byteLength: file.size,
      fingerprint: project.video.fingerprint!, storage: 'idb', info: project.video,
      createdAt: 1, availability: 'ready',
    }])
    const exported = await exportBrowserProjectArchive(project, { includeOriginalMedia: true, includeProxies: false })
    const imported = await importBrowserProjectArchive(exported.blob)
    expect(imported.reusedMedia).toBe(1)
    expect(imported.project.document?.media[0]!.assetRef).toBe('existing-exact')
    expect(putBrowserAsset).not.toHaveBeenCalled()
  })

  it('always imports as a new project identity and blocks false LUT portability', async () => {
    const project = fixture()
    const exported = await exportBrowserProjectArchive(project, { includeOriginalMedia: false, includeProxies: false })
    const first = await importBrowserProjectArchive(exported.blob)
    const second = await importBrowserProjectArchive(exported.blob)
    expect(first.project.id).not.toBe(project.id)
    expect(second.project.id).not.toBe(project.id)
    expect(second.project.id).not.toBe(first.project.id)

    project.document!.sequences[0]!.tracks[0]!.clips[0]!.effects[0]!.parameters.lutId = '11111111-1111-4111-8111-111111111111'
    await expect(exportBrowserProjectArchive(project, { includeOriginalMedia: false, includeProxies: false }))
      .rejects.toThrow(/LUT/)
  })
})
