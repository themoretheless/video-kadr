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
    const request = indexedDB.open('video-kadr-media', 4)
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

function blobKeys(): Promise<string[]> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open('video-kadr-media', 4)
    request.onerror = () => reject(request.error)
    request.onsuccess = () => {
      const database = request.result
      const keys = database.transaction('blobs', 'readonly').objectStore('blobs').getAllKeys()
      keys.onsuccess = () => { database.close(); resolve(keys.result.map(String)) }
      keys.onerror = () => reject(keys.error)
    }
  })
}

describe('browser asset persistence', () => {
  const originalStorage = Object.getOwnPropertyDescriptor(navigator, 'storage')
  const originalLocks = Object.getOwnPropertyDescriptor(navigator, 'locks')
  beforeEach(async () => {
    await clearDatabase()
    Object.defineProperty(navigator, 'locks', {
      configurable: true,
      value: { request: (_name: string, _options: unknown, operation: () => Promise<unknown>) => operation() },
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
    if (originalStorage) Object.defineProperty(navigator, 'storage', originalStorage)
    else Reflect.deleteProperty(navigator, 'storage')
    if (originalLocks) Object.defineProperty(navigator, 'locks', originalLocks)
    else Reflect.deleteProperty(navigator, 'locks')
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

  it('deduplicates aliases by digest and retains bytes until the last manifest is deleted', async () => {
    const first = await putBrowserAsset(asset('alias-a'))
    const second = await putBrowserAsset(asset('alias-b'))
    expect(first.objectKey).toBe(second.objectKey)
    await deleteBrowserAsset('alias-a')
    await expect(getBrowserAsset('alias-b')).resolves.toMatchObject({ id: 'alias-b' })
    await deleteBrowserAsset('alias-b')
    await expect(getBrowserAsset('alias-b')).rejects.toMatchObject({ reason: 'missing' })
  })

  it('reclaims the previous immutable object when an asset changes content', async () => {
    const first = await putBrowserAsset(asset('replace-me'))
    const replacementFile = new Blob(['replacement bytes'], { type: 'video/mp4' })
    const second = await putBrowserAsset({
      ...asset('replace-me'), file: replacementFile,
      info: { ...asset('replace-me').info, sizeBytes: replacementFile.size },
    })
    expect(second.objectKey).not.toBe(first.objectKey)
    expect(await blobKeys()).toEqual([second.objectKey])
    await expect(getBrowserAsset('replace-me')).resolves.toMatchObject({ fingerprint: second.fingerprint })
  })

  it('relinks only the exact fingerprint without changing asset identity', async () => {
    const original = await putBrowserAsset(asset())
    await evictBytes(original.objectKey!)
    expect(await auditBrowserAssets()).toEqual([
      expect.objectContaining({ id: 'asset-1', availability: 'offline' }),
    ])
    const exact = new File(['durable bytes'], 'renamed.mp4', { type: 'video/mp4' })
    const relinked = await relinkBrowserAsset('asset-1', exact)
    expect(relinked.id).toBe('asset-1')
    await expect(
      relinkBrowserAsset('asset-1', new File(['wrong content'], 'clip.mp4')),
    ).rejects.toEqual(expect.objectContaining<Partial<BrowserAssetStorageError>>({
      reason: 'fingerprint',
    }))
  })

  it('uses OPFS by default without storing a fallback Blob', async () => {
    const files = new Map<string, Blob>()
    const mediaDirectory = {
      getFileHandle: async (id: string, options?: { create?: boolean }) => {
        if (!files.has(id) && !options?.create) throw new DOMException('missing', 'NotFoundError')
        return ({
        createWritable: async () => ({
          write: async (blob: Blob) => { files.set(id, blob) },
          close: async () => undefined,
          abort: async () => undefined,
        }),
        getFile: async () => new File([files.get(id)!], id),
        })
      },
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
      getFileHandle: async (id: string, options?: { create?: boolean }) => {
        if (!files.has(id) && !options?.create) throw new DOMException('missing', 'NotFoundError')
        return ({
        createWritable: async () => ({
          write: async (blob: Blob) => { files.set(id, blob) },
          close: async () => undefined,
          abort: async () => undefined,
        }),
        getFile: async () => new File([files.get(id)!], id),
        })
      },
      removeEntry: async (id: string) => { files.delete(id) },
    }
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => mediaDirectory }) },
    })
    const manifest = await putBrowserAsset(asset('corrupt-opfs'))
    files.set(manifest.objectKey!, new Blob(['cut']))

    expect(await auditBrowserAssets()).toEqual([
      expect.objectContaining({ id: 'corrupt-opfs', availability: 'offline' }),
    ])
    await expect(getBrowserAsset('corrupt-opfs')).rejects.toMatchObject({ reason: 'missing' })
  })

  it('rejects a same-size corrupted content-addressed OPFS object before dedup commit', async () => {
    const files = new Map<string, Blob>()
    const mediaDirectory = {
      getFileHandle: async (id: string, options?: { create?: boolean }) => {
        if (!files.has(id) && !options?.create) throw new DOMException('missing', 'NotFoundError')
        return {
          createWritable: async () => ({
            write: async (blob: Blob) => { files.set(id, blob) },
            close: async () => undefined,
            abort: async () => undefined,
          }),
          getFile: async () => new File([files.get(id)!], id),
        }
      },
      removeEntry: async (id: string) => { files.delete(id) },
    }
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => mediaDirectory }) },
    })
    const first = await putBrowserAsset(asset('cas-a'))
    files.set(first.objectKey!, new Blob(['xxxxxxxxxxxxx']))
    await expect(putBrowserAsset(asset('cas-b'))).rejects.toMatchObject({ reason: 'integrity' })
    expect(await allBrowserAssetManifests()).toEqual([
      expect.objectContaining({ id: 'cas-a' }),
    ])
  })

  it('mark-sweeps only unrooted immutable OPFS objects after reconciliation', async () => {
    const files = new Map<string, Blob>()
    const mediaDirectory = {
      getFileHandle: async (id: string, options?: { create?: boolean }) => {
        if (!files.has(id) && !options?.create) throw new DOMException('missing', 'NotFoundError')
        return {
          createWritable: async () => ({
            write: async (blob: Blob) => { files.set(id, blob) }, close: async () => undefined, abort: async () => undefined,
          }),
          getFile: async () => new File([files.get(id)!], id),
        }
      },
      removeEntry: async (id: string) => { files.delete(id) },
      async *entries() { for (const id of files.keys()) yield [id, {}] as [string, FileSystemHandle] },
    }
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => mediaDirectory }) },
    })
    const rooted = await putBrowserAsset(asset('rooted-opfs'))
    const orphan = `sha256-${'b'.repeat(64)}`
    files.set(orphan, new Blob(['orphan']))
    await reconcileBrowserAssetIngests()
    expect(files.has(rooted.objectKey!)).toBe(true)
    expect(files.has(orphan)).toBe(false)
  })

  it('keeps old committed bytes after publish-to-IDB abort and later sweeps the orphan', async () => {
    const files = new Map<string, Blob>()
    const mediaDirectory = {
      getFileHandle: async (id: string, options?: { create?: boolean }) => {
        if (!files.has(id) && !options?.create) throw new DOMException('missing', 'NotFoundError')
        return {
          createWritable: async () => ({
            write: async (blob: Blob) => { files.set(id, blob) }, close: async () => undefined, abort: async () => undefined,
          }),
          getFile: async () => new File([files.get(id)!], id),
        }
      },
      removeEntry: async (id: string) => { files.delete(id) },
      async *entries() { for (const id of files.keys()) yield [id, {}] as [string, FileSystemHandle] },
    }
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => mediaDirectory }) },
    })
    const original = await putBrowserAsset(asset('crash-replace'))
    const nativePut = IDBObjectStore.prototype.put
    const putSpy = vi.spyOn(IDBObjectStore.prototype, 'put').mockImplementation(function (
      this: IDBObjectStore, value: unknown, key?: IDBValidKey,
    ) {
      if (this.name === 'manifests' && (value as { id?: string }).id === 'crash-replace') {
        throw new DOMException('fault after object publish', 'QuotaExceededError')
      }
      return key === undefined ? nativePut.call(this, value) : nativePut.call(this, value, key)
    })
    const replacement = new Blob(['replacement bytes'], { type: 'video/mp4' })
    await expect(putBrowserAsset({ ...asset('crash-replace'), file: replacement })).rejects.toBeDefined()
    putSpy.mockRestore()
    await expect(getBrowserAsset('crash-replace')).resolves.toMatchObject({ objectKey: original.objectKey })

    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-media', 4)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const read = database.transaction('ingests').objectStore('ingests').getAll()
    const journals = await new Promise<Array<Record<string, unknown>>>((resolve, reject) => {
      read.onsuccess = () => resolve(read.result)
      read.onerror = () => reject(read.error)
    })
    const transaction = database.transaction('ingests', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    for (const journal of journals) transaction.objectStore('ingests').put({ ...journal, updatedAt: 1 })
    await committed
    database.close()

    await reconcileBrowserAssetIngests()
    expect([...files.keys()].filter((key) => key.startsWith('sha256-'))).toEqual([original.objectKey])
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
      const request = indexedDB.open('video-kadr-media', 4)
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

  it('does not garbage-collect a fresh ingest that may belong to another tab', async () => {
    const removed: string[] = []
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => ({
        removeEntry: async (id: string) => { removed.push(id) },
      }) }) },
    })
    await allBrowserAssetManifests()
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-media', 4)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('ingests', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('ingests').put({ id: 'active-other-tab', startedAt: Date.now() })
    await committed
    database.close()

    await reconcileBrowserAssetIngests()
    expect(removed).toEqual([])
  })

  it('does no physical reconciliation when Web Locks are unavailable', async () => {
    const removed: string[] = []
    Reflect.deleteProperty(navigator, 'locks')
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => ({
        removeEntry: async (id: string) => { removed.push(id) },
      }) }) },
    })
    await allBrowserAssetManifests()
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-media', 4)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('ingests', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('ingests').put({ id: 'old-orphan', startedAt: 1 })
    await committed
    database.close()
    await reconcileBrowserAssetIngests()
    expect(removed).toEqual([])
  })

  it('clears an abandoned journal without deleting bytes after manifest commit', async () => {
    const removed: string[] = []
    Object.defineProperty(navigator, 'storage', {
      configurable: true,
      value: { getDirectory: async () => ({ getDirectoryHandle: async () => ({
        removeEntry: async (id: string) => { removed.push(id) },
      }) }) },
    })
    await putBrowserAsset(asset('committed-before-crash'))
    removed.length = 0
    const database = await new Promise<IDBDatabase>((resolve, reject) => {
      const request = indexedDB.open('video-kadr-media', 4)
      request.onsuccess = () => resolve(request.result)
      request.onerror = () => reject(request.error)
    })
    const transaction = database.transaction('ingests', 'readwrite')
    const committed = new Promise<void>((resolve) => { transaction.oncomplete = () => resolve() })
    transaction.objectStore('ingests').put({ id: 'committed-before-crash', startedAt: 1 })
    await committed
    database.close()

    await reconcileBrowserAssetIngests()
    expect(removed).toEqual([])
    expect(await allBrowserAssetManifests()).toEqual([
      expect.objectContaining({ id: 'committed-before-crash' }),
    ])
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
