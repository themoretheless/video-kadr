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

    for (let index = 0; index < 20; index += 1) {
      const id = `asset-${index}`
      const file = new Blob([`durable-video-${index}`], { type: 'video/mp4' })
      await putBrowserAsset({
        id,
        file,
        filename: `${id}.mp4`,
        fileType: file.type,
        info: {
          id, filename: `${id}.mp4`, duration: index + 1,
          width: 1920, height: 1080, mediaKind: 'video', sizeBytes: file.size,
        },
        createdAt: index,
      })
    }

    const document = schema.migrateProjectDocument({
      videoId: 'asset-0',
      video: { id: 'asset-0', filename: 'asset-0.mp4', duration: 1, width: 1920, height: 1080 },
      edit: {},
    })
    for (let index = 1; index < 20; index += 1) {
      document.media.push({
        id: `asset-${index}`,
        kind: 'video',
        metadata: { filename: `asset-${index}.mp4`, duration: index + 1 },
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
})
