import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it, vi } from 'vitest'

function clearDatabase(name: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase(name)
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error(`IndexedDB deletion blocked: ${name}`))
  })
}

describe('browser persistence lifecycle', () => {
  beforeEach(async () => {
    await clearDatabase('video-kadr-media')
    await clearDatabase('video-kadr')
    vi.resetModules()
    Object.defineProperty(navigator, 'storage', { configurable: true, value: undefined })
    vi.spyOn(URL, 'createObjectURL').mockImplementation((value) =>
      `blob:test-${value instanceof Blob ? value.size : 'media-source'}`,
    )
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)
  })

  it('restores 20 persisted uploads and lazily materializes bytes after a module reload', async () => {
    const { putBrowserAsset } = await import('./browser-asset-store')
    const schema = await import('./project-schema')
    const media = await import('./browser-media')
    const manifests = new Map<string, Awaited<ReturnType<typeof putBrowserAsset>>>()

    for (let index = 0; index < 20; index += 1) {
      const id = `asset-${index}`
      const file = new Blob([`durable-video-${index}`], { type: 'video/mp4' })
      manifests.set(id, await putBrowserAsset({
        id,
        file,
        filename: `${id}.mp4`,
        fileType: file.type,
        info: {
          id, filename: `${id}.mp4`, duration: index + 1,
          width: 1920, height: 1080, mediaKind: 'video', sizeBytes: file.size,
        },
        createdAt: index,
      }))
    }

    const document = schema.migrateProjectDocument({
      videoId: 'asset-0',
      video: {
        id: 'asset-0', assetId: 'asset-0', fingerprint: manifests.get('asset-0')!.fingerprint,
        filename: 'asset-0.mp4', duration: 1, width: 1920, height: 1080,
        sizeBytes: manifests.get('asset-0')!.byteLength,
      },
      edit: {},
    })
    for (let index = 1; index < 20; index += 1) {
      document.media.push({
        id: `asset-${index}`,
        kind: 'video',
        assetRef: `asset-${index}`,
        contentFingerprint: manifests.get(`asset-${index}`)!.fingerprint,
        metadata: {
          filename: `asset-${index}.mp4`, duration: index + 1,
          sizeBytes: manifests.get(`asset-${index}`)!.byteLength,
        },
      })
    }
    await media.saveProject({
      videoId: 'asset-0', name: 'Twenty assets',
      video: { id: 'asset-0', filename: 'asset-0.mp4', duration: 1 },
      edit: {}, document,
    })

    expect(await media.getLibrary()).toHaveLength(20)
    expect(URL.createObjectURL).not.toHaveBeenCalled()

    vi.resetModules()
    const reloaded = await import('./browser-media')
    const restored = await reloaded.getLibrary()
    expect(restored).toHaveLength(20)
    expect(restored.every((entry) => entry.availability === 'ready' && entry.url === '')).toBe(true)
    expect(URL.createObjectURL).not.toHaveBeenCalled()

    const reopened = await reloaded.resolveSource('asset-19')
    expect(reopened.id).toBe('asset-19')
    expect(reopened.url).toMatch(/^blob:test-\d+$/)
    expect(URL.createObjectURL).toHaveBeenCalledTimes(1)
  })

  it('roots project media by assetRef when clip id differs', async () => {
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: (_name: string, _options: unknown, operation: () => Promise<unknown>) => operation() },
    })
    const assetStore = await import('./browser-asset-store')
    const media = await import('./browser-media')
    const file = new Blob(['aliased bytes'], { type: 'video/mp4' })
    const manifest = await assetStore.putBrowserAsset({
      id: 'asset-1', file, filename: 'alias.mp4', fileType: file.type,
      info: { id: 'asset-1', filename: 'alias.mp4', duration: 1, width: 10, height: 10, mediaKind: 'video' },
      createdAt: 1,
    })
    const { migrateProjectDocument } = await import('./project-schema')
    const document = migrateProjectDocument({
      videoId: 'asset-1', name: 'Alias',
      video: { id: 'asset-1', assetId: 'asset-1', fingerprint: manifest.fingerprint, filename: 'alias.mp4' },
      edit: {},
    })
    document.media[0]!.id = 'clip-source'
    document.primaryMediaId = 'clip-source'
    for (const sequence of document.sequences) for (const track of sequence.tracks) {
      for (const clip of track.clips) clip.mediaId = 'clip-source'
    }
    await expect(media.saveProject({
      videoId: 'asset-1', name: 'Alias', video: { id: 'asset-1' }, edit: {}, document,
    })).resolves.toMatchObject({ document: { media: [expect.objectContaining({ id: 'clip-source', assetRef: 'asset-1' })] } })
    await expect(media.deleteLibraryItem('asset-1')).rejects.toThrow('используется в проекте')
    await expect(assetStore.getBrowserAsset('asset-1')).resolves.toMatchObject({ id: 'asset-1' })
  })

  it('recreates an offline anchor from a project and exact-relinks after manifest eviction', async () => {
    const assetStore = await import('./browser-asset-store')
    const schema = await import('./project-schema')
    const media = await import('./browser-media')
    const bytes = 'recoverable-source'
    const file = new Blob([bytes], { type: 'video/mp4' })
    const manifest = await assetStore.putBrowserAsset({
      id: 'evicted-asset', file, filename: 'recover.mp4', fileType: file.type,
      info: {
        id: 'evicted-asset', filename: 'recover.mp4', duration: 4,
        width: 1920, height: 1080, mediaKind: 'video', sizeBytes: file.size,
      },
      createdAt: 1,
    })
    const document = schema.migrateProjectDocument({
      videoId: 'evicted-asset',
      video: {
        id: 'evicted-asset', assetId: 'evicted-asset', fingerprint: manifest.fingerprint,
        filename: 'recover.mp4', duration: 4, width: 1920, height: 1080,
        sizeBytes: file.size,
      },
      edit: {},
    })
    await media.saveProject({
      videoId: 'evicted-asset', name: 'Recovery',
      video: { id: 'evicted-asset', filename: 'recover.mp4', duration: 4 },
      edit: {}, document,
    })

    await clearDatabase('video-kadr-media')
    vi.resetModules()
    const reloaded = await import('./browser-media')
    expect(await reloaded.getLibrary()).toEqual([
      expect.objectContaining({
        id: 'evicted-asset', fingerprint: manifest.fingerprint, availability: 'offline',
      }),
    ])
    await expect(reloaded.relinkSource(
      'evicted-asset', new File(['wrong'], 'recover.mp4', { type: 'video/mp4' }),
    )).rejects.toMatchObject({ reason: 'fingerprint' })
    const restored = await reloaded.relinkSource(
      'evicted-asset', new File([bytes], 'recover.mp4', { type: 'video/mp4' }),
    )
    expect(restored).toMatchObject({ id: 'evicted-asset', fingerprint: manifest.fingerprint })
    expect((await reloaded.getLibrary())[0]).toMatchObject({ availability: 'ready' })
  })
})
