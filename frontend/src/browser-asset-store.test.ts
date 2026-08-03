import 'fake-indexeddb/auto'

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  allBrowserAssetManifests,
  auditBrowserAssets,
  BrowserAssetStorageError,
  deleteBrowserAsset,
  fingerprintBlob,
  getBrowserAsset,
  putBrowserAsset,
  relinkBrowserAsset,
} from './browser-asset-store'

function clearDatabase(): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.deleteDatabase('video-kadr-media')
    request.onsuccess = () => resolve()
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new Error('IndexedDB deletion blocked'))
  })
}

function asset(id = 'asset-1') {
  return {
    id,
    file: new Blob(['durable bytes'], { type: 'video/mp4' }),
    filename: 'clip.mp4',
    fileType: 'video/mp4',
    info: {
      id, filename: 'clip.mp4', duration: 2, width: 1920, height: 1080,
      mediaKind: 'video' as const, sizeBytes: 13,
    },
    createdAt: 10,
  }
}

function evictBytes(id: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('video-kadr-media', 1)
    request.onerror = () => reject(request.error)
    request.onsuccess = () => {
      const database = request.result
      const transaction = database.transaction('blobs', 'readwrite')
      transaction.objectStore('blobs').delete(id)
      transaction.oncomplete = () => {
        database.close()
        resolve()
      }
      transaction.onerror = () => reject(transaction.error)
    }
  })
}

describe('browser asset persistence', () => {
  const originalStorage = Object.getOwnPropertyDescriptor(navigator, 'storage')
  beforeEach(clearDatabase)
  afterEach(() => {
    if (originalStorage) Object.defineProperty(navigator, 'storage', originalStorage)
    else Reflect.deleteProperty(navigator, 'storage')
  })

  it('commits a lightweight manifest and reads bytes lazily', async () => {
    await putBrowserAsset(asset())
    const [manifest] = await allBrowserAssetManifests()
    expect(manifest).toMatchObject({
      id: 'asset-1', byteLength: 13,
    })
    expect(manifest).not.toHaveProperty('file')

    const restored = await getBrowserAsset('asset-1')
    expect(restored.info).toMatchObject({ id: 'asset-1', mediaKind: 'video' })
    expect(restored.file).toBeDefined()
    expect(restored.fingerprint).toMatch(/^[a-f0-9]{64}$/)
  })

  it('deletes manifest and bytes together', async () => {
    await putBrowserAsset(asset())
    await deleteBrowserAsset('asset-1')
    expect(await allBrowserAssetManifests()).toEqual([])
    await expect(getBrowserAsset('asset-1')).rejects.toMatchObject({ reason: 'missing' })
  })

  it('relinks only the exact fingerprint without changing asset identity', async () => {
    await putBrowserAsset(asset())
    await evictBytes('asset-1')
    expect(await auditBrowserAssets()).toEqual([
      expect.objectContaining({ id: 'asset-1', availability: 'offline' }),
    ])
    const exact = new File(['durable bytes'], 'renamed.mp4', { type: 'video/mp4' })
    const relinked = await relinkBrowserAsset('asset-1', exact)
    expect(relinked.id).toBe('asset-1')
    await expect(
      relinkBrowserAsset('asset-1', new File(['wrong'], 'clip.mp4')),
    ).rejects.toEqual(expect.objectContaining<Partial<BrowserAssetStorageError>>({
      reason: 'fingerprint',
    }))
  })

  it('uses OPFS by default without storing a fallback Blob', async () => {
    const files = new Map<string, Blob>()
    const mediaDirectory = {
      getFileHandle: async (id: string) => ({
        createWritable: async () => ({
          write: async (blob: Blob) => { files.set(id, blob) },
          close: async () => undefined,
          abort: async () => undefined,
        }),
        getFile: async () => new File([files.get(id)!], id),
      }),
      removeEntry: async (id: string) => { files.delete(id) },
    }
    const root = {
      getDirectoryHandle: async () => mediaDirectory,
    }
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => root },
    })

    const manifest = await putBrowserAsset(asset('opfs-asset'))
    expect(manifest.storage).toBe('opfs')
    expect((await getBrowserAsset('opfs-asset')).file.size).toBe(13)
    expect(await auditBrowserAssets()).toEqual([
      expect.objectContaining({ id: 'opfs-asset', availability: 'ready', storage: 'opfs' }),
    ])
  })

  it('fails closed before materializing an oversized fingerprint in memory', async () => {
    const arrayBuffer = vi.fn<() => Promise<ArrayBuffer>>()
    const oversized = { size: 128 * 1024 * 1024 + 1, arrayBuffer } as unknown as Blob
    await expect(fingerprintBlob(oversized)).rejects.toMatchObject({ reason: 'unavailable' })
    expect(arrayBuffer).not.toHaveBeenCalled()
  })
})
