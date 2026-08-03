import 'fake-indexeddb/auto'

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import {
  allBrowserAssetManifests,
  auditBrowserAssets,
  BrowserAssetStorageError,
  deleteBrowserAsset,
  fingerprintBlob,
  getBrowserAsset,
  prepareBrowserStorage,
  putBrowserAsset,
  reconcileBrowserAssetIngests,
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
    const request = indexedDB.open('video-kadr-media', 3)
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
    vi.unstubAllGlobals()
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
    expect(restored.fingerprint).toBe('849e9d3592edcb72635d1e74af2b7ded2c07f6b79f4b27de7e4bc2e507169213')
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

  it('marks a truncated OPFS asset offline and refuses to materialize it', async () => {
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
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => mediaDirectory }) },
    })
    await putBrowserAsset(asset('corrupt-opfs'))
    files.set('corrupt-opfs', new Blob(['cut']))

    expect(await auditBrowserAssets()).toEqual([
      expect.objectContaining({ id: 'corrupt-opfs', availability: 'offline' }),
    ])
    await expect(getBrowserAsset('corrupt-opfs')).rejects.toMatchObject({ reason: 'missing' })
  })

  it('routes large media to a worker without materializing the whole Blob', async () => {
    const arrayBuffer = vi.fn()
    const large = { size: 129 * 1024 * 1024, arrayBuffer } as unknown as Blob
    class FakeWorker {
      onmessage: ((event: MessageEvent) => void) | null = null
      onerror: ((event: ErrorEvent) => void) | null = null
      constructor() {
        queueMicrotask(() => this.onmessage?.({ data: { ready: true } } as MessageEvent))
      }
      postMessage(value: unknown) {
        expect(value).toBe(large)
        queueMicrotask(() => this.onmessage?.({ data: { digest: 'a'.repeat(64) } } as MessageEvent))
      }
      terminate() {}
    }
    vi.stubGlobal('Worker', FakeWorker)
    await expect(fingerprintBlob(large)).resolves.toBe('a'.repeat(64))
    expect(arrayBuffer).not.toHaveBeenCalled()
  })

  it('reconciles a crash journal by deleting uncommitted OPFS bytes', async () => {
    const removed: string[] = []
    const root = { getDirectoryHandle: async () => ({
      removeEntry: async (id: string) => { removed.push(id) },
    }) }
    Object.defineProperty(navigator, 'storage', {
      configurable: true, value: { getDirectory: async () => root },
    })
    await allBrowserAssetManifests()
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-media', 3)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('ingests', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('ingests').put({ id: 'orphan', startedAt: 1 })
    await committed
    database.close()

    await reconcileBrowserAssetIngests()
    expect(removed).toEqual(['orphan'])
  })

  it('reports quota pressure before attempting a write', async () => {
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: {
        persist: async () => false,
        estimate: async () => ({ usage: 900, quota: 1_000 }),
      },
    })
    await expect(prepareBrowserStorage(101)).rejects.toMatchObject({ reason: 'quota' })
    await expect(prepareBrowserStorage(100)).resolves.toEqual({
      persisted: false, usage: 900, quota: 1_000,
    })
  })

  it('falls back to IndexedDB when OPFS is unavailable as in private browsing', async () => {
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => { throw new DOMException('blocked', 'SecurityError') } },
    })
    const manifest = await putBrowserAsset(asset('private-fallback'))
    expect(manifest.storage).toBe('idb')
    await expect(getBrowserAsset('private-fallback')).resolves.toMatchObject({ id: 'private-fallback' })
  })
})
