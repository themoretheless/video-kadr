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

describe('browser output rate control', () => {
  it('uses the size-v1 bitrate golden without CRF and fixes audio bitrate', async () => {
    const { browserOutputSpec } = await import('./browser-media')
    const spec = browserOutputSpec({
      format: 'mp4', codec: 'h264',
      rateControl: { mode: 'target_size', targetBytes: 10_000_000, videoBitrateBps: 7_472_000, audioBitrateBps: 128_000, estimatorVersion: 'size-v1' },
    })
    expect(spec.args).toEqual(expect.arrayContaining(['-b:v', '7472000', '-maxrate', '7472000', '-bufsize', '14944000', '-b:a', '128000']))
    expect(spec.args).not.toContain('-crf')
  })

  it('rejects H.265 instead of silently encoding H.264', async () => {
    const { browserOutputSpec } = await import('./browser-media')
    expect(() => browserOutputSpec({ format: 'mp4', codec: 'h265' })).toThrow('H.265 недоступен')
  })

  it('rejects unknown formats and incompatible codec tokens', async () => {
    const { browserOutputSpec } = await import('./browser-media')
    expect(() => browserOutputSpec({ format: 'av1' })).toThrow('Неподдерживаемый формат')
    expect(() => browserOutputSpec({ format: 'mp4', codec: 'vp9' })).toThrow('Некорректный кодек MP4')
    expect(() => browserOutputSpec({ format: 'webm', codec: 'vp9' })).toThrow('Кодек можно задавать только для MP4')
  })

  it('rejects malformed tagged CRF and coexistence with legacy quality', async () => {
    const { browserOutputSpec } = await import('./browser-media')
    expect(() => browserOutputSpec({ format: 'mp4', rateControl: { mode: 'quality', crf: 52 } })).toThrow('quality rateControl')
    expect(() => browserOutputSpec({ format: 'webm', rateControl: { mode: 'quality', crf: 1.5 } })).toThrow('quality rateControl')
    expect(() => browserOutputSpec({ format: 'mp4', quality: 23, rateControl: { mode: 'quality', crf: 23 } })).toThrow('взаимоисключающие')
    expect(() => browserOutputSpec({ format: 'gif', rateControl: { mode: 'quality', crf: 23 } })).toThrow('Quality rateControl')
    expect(() => browserOutputSpec({ format: 'mp3', rateControl: { mode: 'quality', crf: 23 } })).toThrow('Quality rateControl')
  })
})
