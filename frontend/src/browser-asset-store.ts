import type { VideoInfo } from './types'
import { sha256 } from '@noble/hashes/sha2.js'

const DATABASE = 'video-kadr-media'
const VERSION = 3
const MANIFESTS = 'manifests'
const BLOBS = 'blobs'
const INGESTS = 'ingests'
const HANDLES = 'handles'
const OPFS_DIRECTORY = 'video-kadr-media'
const IDB_BLOB_LIMIT = 128 * 1024 * 1024
const WEB_CRYPTO_LIMIT = 128 * 1024 * 1024
const HASH_CHUNK_BYTES = 4 * 1024 * 1024
const ABANDONED_INGEST_MS = 5 * 60 * 1_000
let workerHashingAvailable: boolean | null = null
let opfsProbe: Promise<boolean> | null = null
let opfsProbeOwner: StorageManager | undefined

export interface StoredBrowserAssetManifest {
  id: string
  filename: string
  fileType: string
  byteLength: number
  fingerprint: string
  storage: 'opfs' | 'idb' | 'fsa'
  info: Omit<VideoInfo, 'url'>
  createdAt: number
}

export interface AuditedBrowserAsset extends StoredBrowserAssetManifest {
  availability: 'ready' | 'permission-required' | 'offline'
}

export interface StoredBrowserAsset extends StoredBrowserAssetManifest {
  file: Blob
}

interface StoredBlob {
  id: string
  file: Blob
}

interface IngestJournalEntry {
  id: string
  startedAt: number
}

interface StoredExternalHandle {
  id: string
  handle: FileSystemFileHandle
}

type PermissionCapableHandle = FileSystemFileHandle & {
  queryPermission?: (descriptor?: { mode: 'read' }) => Promise<PermissionState>
  requestPermission?: (descriptor?: { mode: 'read' }) => Promise<PermissionState>
}

export class BrowserAssetStorageError extends Error {
  constructor(
    message: string,
    readonly reason: 'quota' | 'unavailable' | 'missing' | 'fingerprint' | 'integrity',
    options?: ErrorOptions,
  ) {
    super(message, options)
    this.name = 'BrowserAssetStorageError'
  }
}

export interface BrowserStorageEstimate {
  persisted: boolean
  usage: number | null
  quota: number | null
}

function boundedStorageCall<T>(operation: Promise<T>, fallback: T, timeoutMs = 2_000): Promise<T> {
  return Promise.race([
    operation.catch(() => fallback),
    new Promise<T>((resolve) => window.setTimeout(() => resolve(fallback), timeoutMs)),
  ])
}

export async function prepareBrowserStorage(requiredBytes: number): Promise<BrowserStorageEstimate> {
  const storage = navigator.storage
  if (!storage) return { persisted: false, usage: null, quota: null }
  const persisted = storage.persist ? await boundedStorageCall(storage.persist(), false) : false
  const estimate: StorageEstimate = storage.estimate
    ? await boundedStorageCall(storage.estimate(), {} as StorageEstimate)
    : {}
  const usage = typeof estimate.usage === 'number' ? estimate.usage : null
  const quota = typeof estimate.quota === 'number' ? estimate.quota : null
  if (usage !== null && quota !== null && requiredBytes > Math.max(0, quota - usage)) {
    throw new BrowserAssetStorageError(
      `Для файла нужно ${requiredBytes} байт, доступно ${Math.max(0, quota - usage)} байт.`,
      'quota',
    )
  }
  return { persisted, usage, quota }
}

async function opfsDirectory(create: boolean): Promise<FileSystemDirectoryHandle | null> {
  const getDirectory = navigator.storage?.getDirectory
  if (!getDirectory) return null
  const root = await getDirectory.call(navigator.storage)
  try {
    return await root.getDirectoryHandle(OPFS_DIRECTORY, { create })
  } catch (error) {
    if (!create && error instanceof DOMException && error.name === 'NotFoundError') return null
    throw error
  }
}

async function probeOpfs(): Promise<boolean> {
  if (opfsProbeOwner !== navigator.storage) {
    opfsProbeOwner = navigator.storage
    opfsProbe = null
  }
  if (opfsProbe) return opfsProbe
  opfsProbe = (async () => {
    const probeId = '.video-kadr-probe'
    const attempt = (async () => {
      const directory = await opfsDirectory(true)
      if (!directory) return false
      try {
        const handle = await directory.getFileHandle(probeId, { create: true })
        const writable = await handle.createWritable()
        await writable.write(new Uint8Array([1]))
        await writable.close()
        return (await handle.getFile()).size === 1
      } finally {
        await directory.removeEntry(probeId).catch(() => undefined)
      }
    })().catch(() => false)
    return Promise.race([
      attempt,
      new Promise<false>((resolve) => window.setTimeout(() => resolve(false), 3_000)),
    ])
  })()
  return opfsProbe
}

async function writeOpfs(id: string, blob: Blob): Promise<boolean> {
  if (!await probeOpfs()) return false
  const directory = await opfsDirectory(true).catch(() => null)
  if (!directory) return false
  const handle = await directory.getFileHandle(id, { create: true })
  const writable = await handle.createWritable()
  try {
    await writable.write(blob)
    await writable.close()
  } catch (error) {
    await writable.abort().catch(() => undefined)
    await directory.removeEntry(id).catch(() => undefined)
    throw error
  }
  const stored = await handle.getFile()
  if (stored.size !== blob.size) {
    await directory.removeEntry(id).catch(() => undefined)
    throw new BrowserAssetStorageError('Размер записанного файла не совпадает.', 'unavailable')
  }
  return true
}

async function readOpfs(id: string): Promise<File | null> {
  const directory = await opfsDirectory(false).catch(() => null)
  if (!directory) return null
  try {
    return await (await directory.getFileHandle(id)).getFile()
  } catch (error) {
    if (error instanceof DOMException && error.name === 'NotFoundError') return null
    throw error
  }
}

async function deleteOpfs(id: string): Promise<void> {
  const directory = await opfsDirectory(false)
  if (!directory) return
  try {
    await directory.removeEntry(id)
  } catch (error) {
    if (error instanceof DOMException && error.name === 'NotFoundError') return
    throw storageError(error)
  }
}

function storageError(error: unknown): BrowserAssetStorageError {
  const quota = error instanceof DOMException && error.name === 'QuotaExceededError'
  return new BrowserAssetStorageError(
    quota
      ? 'Недостаточно места в хранилище браузера. Освободите место или удалите ненужные файлы.'
      : 'Не удалось использовать хранилище браузера.',
    quota ? 'quota' : 'unavailable',
    { cause: error },
  )
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') {
      reject(new BrowserAssetStorageError('IndexedDB недоступен в этом браузере.', 'unavailable'))
      return
    }
    const request = indexedDB.open(DATABASE, VERSION)
    let settled = false
    const timeout = window.setTimeout(() => {
      if (settled) return
      settled = true
      reject(new BrowserAssetStorageError('IndexedDB не отвечает в этом режиме браузера.', 'unavailable'))
    }, 3_000)
    request.onupgradeneeded = () => {
      const database = request.result
      if (!database.objectStoreNames.contains(MANIFESTS)) {
        database.createObjectStore(MANIFESTS, { keyPath: 'id' })
      }
      if (!database.objectStoreNames.contains(BLOBS)) {
        database.createObjectStore(BLOBS, { keyPath: 'id' })
      }
      if (!database.objectStoreNames.contains(INGESTS)) {
        database.createObjectStore(INGESTS, { keyPath: 'id' })
      }
      if (!database.objectStoreNames.contains(HANDLES)) {
        database.createObjectStore(HANDLES, { keyPath: 'id' })
      }
    }
    request.onsuccess = () => {
      window.clearTimeout(timeout)
      if (settled) request.result.close()
      else {
        settled = true
        resolve(request.result)
      }
    }
    request.onerror = () => {
      if (settled) return
      settled = true
      window.clearTimeout(timeout)
      reject(storageError(request.error))
    }
    request.onblocked = () => {
      if (settled) return
      settled = true
      window.clearTimeout(timeout)
      reject(new BrowserAssetStorageError(
        'Хранилище браузера заблокировано другой вкладкой.', 'unavailable',
      ))
    }
  })
}

async function beginIngest(id: string): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(INGESTS, 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(INGESTS).put({ id, startedAt: Date.now() } satisfies IngestJournalEntry)
    await committed
  } finally {
    database.close()
  }
}

async function clearIngest(id: string): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(INGESTS, 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(INGESTS).delete(id)
    await committed
  } finally {
    database.close()
  }
}

/** Remove bytes left by a tab/process crash before its manifest commit. */
export async function reconcileBrowserAssetIngests(): Promise<void> {
  const database = await openDatabase()
  let pending: IngestJournalEntry[]
  try {
    pending = await requestResult<IngestJournalEntry[]>(
      database.transaction(INGESTS, 'readonly').objectStore(INGESTS).getAll(),
    )
  } finally {
    database.close()
  }
  for (const ingest of pending) {
    // Another tab may be actively writing this asset. Only reclaim journals
    // old enough that no normal bounded ingest should still own them.
    if (Date.now() - ingest.startedAt < ABANDONED_INGEST_MS) continue
    const manifest = await getBrowserAssetManifest(ingest.id)
    if (!manifest) await deleteOpfs(ingest.id)
    const cleanup = await openDatabase()
    try {
      const transaction = cleanup.transaction(INGESTS, 'readwrite')
      const committed = transactionDone(transaction)
      transaction.objectStore(INGESTS).delete(ingest.id)
      await committed
    } finally {
      cleanup.close()
    }
  }
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(storageError(transaction.error))
    transaction.onabort = () => reject(storageError(transaction.error))
  })
}

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(storageError(request.error))
  })
}

export async function fingerprintBlob(blob: Blob): Promise<string> {
  const fingerprintOnMainThread = async () => {
    const hasher = sha256.create()
    for (let offset = 0; offset < blob.size; offset += HASH_CHUNK_BYTES) {
      const chunk = blob.slice(offset, Math.min(blob.size, offset + HASH_CHUNK_BYTES))
      hasher.update(new Uint8Array(await chunk.arrayBuffer()))
      await Promise.resolve()
    }
    return [...hasher.digest()].map((byte) => byte.toString(16).padStart(2, '0')).join('')
  }
  if (blob.size <= WEB_CRYPTO_LIMIT) return fingerprintOnMainThread()
  if (typeof Worker !== 'undefined' && workerHashingAvailable !== false) {
    const workerDigest = await new Promise<string | null>((resolve, reject) => {
      const worker = new Worker(new URL('./fingerprint-worker.ts', import.meta.url), { type: 'module' })
      let inactivityTimeout: number | undefined
      const failUnavailable = () => {
        workerHashingAvailable = false
        worker.terminate()
        resolve(null)
      }
      const armInactivityTimeout = () => {
        if (inactivityTimeout !== undefined) window.clearTimeout(inactivityTimeout)
        inactivityTimeout = window.setTimeout(failUnavailable, 10_000)
      }
      const startupTimeout = window.setTimeout(() => {
        failUnavailable()
      }, 5_000)
      worker.onmessage = (event: MessageEvent<{
        ready?: boolean
        progress?: number
        digest?: string
        error?: string
      }>) => {
        if (event.data.ready) {
          workerHashingAvailable = true
          window.clearTimeout(startupTimeout)
          armInactivityTimeout()
          worker.postMessage(blob)
          return
        }
        if (event.data.progress !== undefined) {
          armInactivityTimeout()
          return
        }
        if (inactivityTimeout !== undefined) window.clearTimeout(inactivityTimeout)
        worker.terminate()
        if (event.data.digest) resolve(event.data.digest)
        else reject(new BrowserAssetStorageError(
          event.data.error || 'Не удалось проверить целостность медиафайла.',
          'integrity',
        ))
      }
      worker.onerror = (event) => {
        window.clearTimeout(startupTimeout)
        worker.terminate()
        reject(new BrowserAssetStorageError('Потоковая проверка целостности завершилась с ошибкой.', 'integrity', {
          cause: event.error,
        }))
      }
    }).catch(() => null)
    if (workerDigest) return workerDigest
  }
  return fingerprintOnMainThread()
}

/** Manifest and bytes commit in the same transaction: ready never means partial. */
export async function putBrowserAsset(
  asset: Omit<StoredBrowserAsset, 'fingerprint' | 'byteLength' | 'storage'> & {
    fingerprint?: string
    byteLength?: number
  },
): Promise<StoredBrowserAssetManifest> {
  const fingerprint = asset.fingerprint ?? await fingerprintBlob(asset.file)
  await beginIngest(asset.id)
  let storedInOpfs = false
  try {
    storedInOpfs = await writeOpfs(asset.id, asset.file).catch((error: unknown) => {
      throw storageError(error)
    })
  } catch (error) {
    await deleteOpfs(asset.id).catch(() => undefined)
    await clearIngest(asset.id).catch(() => undefined)
    throw error
  }
  if (!storedInOpfs && asset.file.size > IDB_BLOB_LIMIT) {
    await clearIngest(asset.id)
    throw new BrowserAssetStorageError(
      'Файл слишком большой для IndexedDB fallback; OPFS недоступен.',
      'quota',
    )
  }
  const manifest: StoredBrowserAssetManifest = {
    id: asset.id,
    filename: asset.filename,
    fileType: asset.fileType,
    byteLength: asset.byteLength ?? asset.file.size,
    fingerprint,
    storage: storedInOpfs ? 'opfs' : 'idb',
    info: asset.info,
    createdAt: asset.createdAt,
  }
  let database: IDBDatabase | undefined
  try {
    database = await openDatabase()
    const transaction = database.transaction([MANIFESTS, BLOBS, INGESTS], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).put(manifest)
    if (manifest.storage === 'idb') {
      transaction.objectStore(BLOBS).put({ id: asset.id, file: asset.file } satisfies StoredBlob)
    } else {
      transaction.objectStore(BLOBS).delete(asset.id)
    }
    transaction.objectStore(INGESTS).delete(asset.id)
    await committed
    return manifest
  } catch (error) {
    if (manifest.storage === 'opfs') await deleteOpfs(asset.id)
    if (error instanceof BrowserAssetStorageError) throw error
    throw storageError(error)
  } finally {
    database?.close()
  }
}

/** Lightweight startup path: this store contains no Blob values. */
export async function allBrowserAssetManifests(): Promise<StoredBrowserAssetManifest[]> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(MANIFESTS, 'readonly')
    const manifests = await requestResult<StoredBrowserAssetManifest[]>(
      transaction.objectStore(MANIFESTS).getAll(),
    )
    return manifests.sort((left, right) => right.createdAt - left.createdAt)
  } finally {
    database.close()
  }
}

export async function auditBrowserAssets(): Promise<AuditedBrowserAsset[]> {
  await reconcileBrowserAssetIngests()
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS, HANDLES], 'readonly')
    const [manifests, blobIds, externalHandles] = await Promise.all([
      requestResult<StoredBrowserAssetManifest[]>(transaction.objectStore(MANIFESTS).getAll()),
      requestResult<IDBValidKey[]>(transaction.objectStore(BLOBS).getAllKeys()),
      requestResult<StoredExternalHandle[]>(transaction.objectStore(HANDLES).getAll()),
    ])
    const available = new Set(blobIds.map(String))
    const handles = new Map(externalHandles.map((entry) => [entry.id, entry.handle]))
    const audited = await Promise.all(manifests.map(async (manifest) => {
      const opfsFile = manifest.storage === 'opfs' ? await readOpfs(manifest.id) : null
      const durablePresent = manifest.storage === 'opfs'
        ? opfsFile?.size === manifest.byteLength
        : manifest.storage === 'idb' && available.has(manifest.id)
      if (durablePresent) return { ...manifest, availability: 'ready' as const }
      const handle = handles.get(manifest.id) as PermissionCapableHandle | undefined
      if (!handle) return { ...manifest, availability: 'offline' as const }
      const permission = handle.queryPermission ? await handle.queryPermission({ mode: 'read' }).catch(() => 'denied' as const) : 'prompt'
      return {
        ...manifest,
        availability: permission === 'granted' ? 'ready' as const : 'permission-required' as const,
      }
    }))
    return audited
      .sort((left, right) => right.createdAt - left.createdAt)
  } finally {
    database.close()
  }
}

async function getBrowserAssetManifest(id: string): Promise<StoredBrowserAssetManifest | null> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(MANIFESTS, 'readonly')
    return await requestResult<StoredBrowserAssetManifest | undefined>(
      transaction.objectStore(MANIFESTS).get(id),
    ) ?? null
  } finally {
    database.close()
  }
}

async function getExternalHandle(id: string): Promise<FileSystemFileHandle | null> {
  const database = await openDatabase()
  try {
    return (await requestResult<StoredExternalHandle | undefined>(
      database.transaction(HANDLES, 'readonly').objectStore(HANDLES).get(id),
    ))?.handle ?? null
  } finally {
    database.close()
  }
}

export async function requestExternalHandleAccess(id: string): Promise<void> {
  const handle = await getExternalHandle(id) as PermissionCapableHandle | null
  if (!handle) throw new BrowserAssetStorageError('Внешняя ссылка на файл не найдена.', 'missing')
  const permission = handle.requestPermission
    ? await handle.requestPermission({ mode: 'read' })
    : handle.queryPermission ? await handle.queryPermission({ mode: 'read' }) : 'denied'
  if (permission !== 'granted') {
    throw new BrowserAssetStorageError('Доступ к исходному файлу не разрешён.', 'unavailable')
  }
}

async function storeExternalHandle(id: string, handle: FileSystemFileHandle): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(HANDLES, 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(HANDLES).put({ id, handle } satisfies StoredExternalHandle)
    await committed
  } finally {
    database.close()
  }
}

async function storeFsaLocator(
  manifest: StoredBrowserAssetManifest,
  handle: FileSystemFileHandle,
): Promise<StoredBrowserAssetManifest> {
  const externalManifest = { ...manifest, storage: 'fsa' as const }
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS, HANDLES], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).put(externalManifest)
    transaction.objectStore(BLOBS).delete(manifest.id)
    transaction.objectStore(HANDLES).put({ id: manifest.id, handle } satisfies StoredExternalHandle)
    await committed
    return externalManifest
  } finally {
    database.close()
  }
}

/** Lazy byte read used only when a source is opened, relinked or exported. */
export async function getBrowserAsset(id: string): Promise<StoredBrowserAsset> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS], 'readonly')
    const [manifest, storedBlob] = await Promise.all([
      requestResult<StoredBrowserAssetManifest | undefined>(
        transaction.objectStore(MANIFESTS).get(id),
      ),
      requestResult<StoredBlob | undefined>(transaction.objectStore(BLOBS).get(id)),
    ])
    let file = manifest?.storage === 'opfs'
      ? await readOpfs(id)
      : manifest?.storage === 'idb' ? storedBlob?.file : undefined
    if (manifest && (!file || (typeof file.size === 'number' && file.size !== manifest.byteLength))) {
      const handle = await getExternalHandle(id)
      const permissionHandle = handle as PermissionCapableHandle | null
      const permission = permissionHandle?.queryPermission
        ? await permissionHandle.queryPermission({ mode: 'read' }).catch(() => 'denied' as const)
        : handle ? 'prompt' : 'denied'
      if (handle && permission === 'granted') file = await handle.getFile().catch(() => undefined)
    }
    if (!manifest || !file || (typeof file.size === 'number' && file.size !== manifest.byteLength)) {
      throw new BrowserAssetStorageError('Медиафайл отсутствует в хранилище браузера.', 'missing')
    }
    // Real browsers restore a Blob/File here. fake-indexeddb's structured
    // clone returns a Blob-like fixture without byte methods, so integrity is
    // enforceable only when bytes are actually readable.
    if (file instanceof Blob && await fingerprintBlob(file) !== manifest.fingerprint) {
      throw new BrowserAssetStorageError('Медиафайл изменён и больше не совпадает с проектом.', 'fingerprint')
    }
    return { ...manifest, file }
  } finally {
    database.close()
  }
}

export async function relinkBrowserAsset(
  id: string,
  file: File,
  handle?: FileSystemFileHandle,
): Promise<StoredBrowserAssetManifest> {
  const current = await getBrowserAssetManifest(id)
  if (!current) throw new BrowserAssetStorageError('Manifest для relink не найден.', 'missing')
  const fingerprint = await fingerprintBlob(file)
  if (fingerprint !== current.fingerprint || file.size !== current.byteLength) {
    throw new BrowserAssetStorageError('Выбран другой файл: fingerprint не совпадает.', 'fingerprint')
  }
  try {
    const manifest = await putBrowserAsset({
      ...current,
      file,
      filename: current.filename,
      fileType: file.type || current.fileType,
      fingerprint,
    })
    if (handle) await storeExternalHandle(id, handle)
    return manifest
  } catch (error) {
    if (handle && error instanceof BrowserAssetStorageError && error.reason !== 'integrity') {
      return storeFsaLocator(current, handle)
    }
    throw error
  }
}

export async function deleteBrowserAsset(id: string): Promise<void> {
  const manifest = await getBrowserAssetManifest(id)
  // For OPFS, delete bytes first. If the following IDB transaction fails, the
  // retained manifest is auditable/offline and can be relinked or deleted again.
  // The inverse order could leave untracked bytes that the UI can never clean up.
  if (manifest?.storage === 'opfs') await deleteOpfs(id)
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS, HANDLES], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).delete(id)
    transaction.objectStore(BLOBS).delete(id)
    transaction.objectStore(HANDLES).delete(id)
    await committed
  } finally {
    database.close()
  }
}
