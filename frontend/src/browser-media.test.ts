import 'fake-indexeddb/auto'

import { beforeEach, describe, expect, it, vi } from 'vitest'

import { putBrowserAsset } from './browser-asset-store'

function clearDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr-media')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

describe('browser media reload hydration', () => {
  beforeEach(async () => {
    await clearDatabase()
    vi.resetModules()
  })

  it('loads manifests without blobs and materializes a fresh URL only on demand', async () => {
    await putBrowserAsset({
      id: 'durable-source',
      file: new Blob(['video bytes'], { type: 'video/mp4' }),
      filename: 'durable.mp4',
      fileType: 'video/mp4',
      info: {
        id: 'durable-source', filename: 'durable.mp4', duration: 2,
        width: 1920, height: 1080, mediaKind: 'video', sizeBytes: 11,
      },
      createdAt: 1,
    })
    let serial = 0
    const createUrl = vi.spyOn(URL, 'createObjectURL')
      .mockImplementation(() => `blob:runtime-${++serial}`)
    vi.spyOn(URL, 'revokeObjectURL').mockImplementation(() => undefined)

    const firstModule = await import('./browser-media')
    const firstLibrary = await firstModule.getLibrary()
    expect(firstLibrary[0]).toMatchObject({
      id: 'durable-source', url: '', availability: 'ready',
    })
    expect(createUrl).not.toHaveBeenCalled()
    expect((await firstModule.resolveSource('durable-source')).url).toBe('blob:runtime-1')

    vi.resetModules()
    const reloadedModule = await import('./browser-media')
    expect((await reloadedModule.getLibrary())[0]?.url).toBe('')
    expect((await reloadedModule.resolveSource('durable-source')).url).toBe('blob:runtime-2')
  })

  it('fails closed on physical deletion when cross-tab Web Locks are unavailable', async () => {
    await putBrowserAsset({
      id: 'protected-source', file: new Blob(['protected']), filename: 'protected.mp4',
      fileType: 'video/mp4',
      info: {
        id: 'protected-source', filename: 'protected.mp4', duration: 1,
        width: 1920, height: 1080, mediaKind: 'video',
      },
      createdAt: 1,
    })
    Object.defineProperty(navigator, 'locks', { configurable: true, value: undefined })
    const media = await import('./browser-media')
    await expect(media.deleteLibraryItem('protected-source')).rejects.toThrow('Web Locks API')
    expect(await media.getLibrary()).toEqual([
      expect.objectContaining({ id: 'protected-source', availability: 'ready' }),
    ])
  })
})
