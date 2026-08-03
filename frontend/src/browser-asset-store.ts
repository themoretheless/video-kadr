import type { VideoInfo } from './types'
import { sha256 } from '@noble/hashes/sha2.js'

const DATABASE = 'video-kadr-media'
const VERSION = 4
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
  /** Immutable content-addressed byte locator. Older manifests fall back to id. */
  objectKey?: string
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

export interface BrowserAssetRecoveryReference {
  id: string
  filename: string
  fileType?: string
  fingerprint: string
  byteLength?: number
  info: Omit<VideoInfo, 'url'>
  createdAt?: number
}

interface StoredBlob {
  id: string
  file: Blob
}

interface IngestJournalEntry {
  id: string
  assetId?: string
  stagingKey?: string
  startedAt: number
  updatedAt?: number
  phase?: 'writing' | 'publishing'
  objectKey?: string
}

const storageLockName = 'video-kadr-media-maintenance'

function supportsWebLocks(): boolean {
  return typeof (navigator as unknown as { locks?: { request?: unknown } }).locks?.request === 'function'
}

async function withStorageLock<T>(callback: () => Promise<T>): Promise<T> {
  const locks = navigator.locks
  if (!locks?.request) return callback()
  return locks.request(storageLockName, { mode: 'exclusive' }, callback)
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

async function writeOpfs(id: string, blob: Blob, preserveExisting = false): Promise<boolean> {
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
    if (!preserveExisting) await directory.removeEntry(id).catch(() => undefined)
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

async function putIngest(entry: IngestJournalEntry): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(INGESTS, 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(INGESTS).put(entry)
    await committed
  } finally {
    database.close()
  }
}

async function beginIngest(assetId: string, objectKey: string): Promise<IngestJournalEntry> {
  const now = Date.now()
  const owner = crypto.randomUUID()
  const entry: IngestJournalEntry = {
    id: owner,
    assetId,
    stagingKey: `.staging-${objectKey}-${owner}`,
    objectKey,
    startedAt: now,
    updatedAt: now,
    phase: 'writing',
  }
  await putIngest(entry)
  return entry
}

async function clearIngest(journalId: string): Promise<void> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(INGESTS, 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(INGESTS).delete(journalId)
    await committed
  } finally {
    database.close()
  }
}

/** Remove bytes left by a tab/process crash before its manifest commit. */
export async function reconcileBrowserAssetIngests(): Promise<void> {
  // Destructive cleanup must never race another tab. Browsers without Web
  // Locks keep orphan bytes (safe leak) instead of risking live-media loss.
  if (!supportsWebLocks()) return
  return withStorageLock(reconcileBrowserAssetIngestsUnlocked)
}

async function reconcileBrowserAssetIngestsUnlocked(): Promise<void> {
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
    if (Date.now() - (ingest.updatedAt ?? ingest.startedAt) < ABANDONED_INGEST_MS) continue
    if (ingest.stagingKey) await deleteOpfs(ingest.stagingKey)
    else if (!ingest.assetId && !await getBrowserAssetManifest(ingest.id)) await deleteOpfs(ingest.id)
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
  return withStorageLock(async () => putBrowserAssetUnlocked(asset, fingerprint))
}

async function putBrowserAssetUnlocked(
  asset: Omit<StoredBrowserAsset, 'fingerprint' | 'byteLength' | 'storage'> & {
    fingerprint?: string
    byteLength?: number
  },
  fingerprint: string,
): Promise<StoredBrowserAssetManifest> {
  const objectKey = `sha256-${fingerprint}`
  const previousManifest = await getBrowserAssetManifest(asset.id)
  const ingest = await beginIngest(asset.id, objectKey)
  let storedInOpfs = false
  try {
    storedInOpfs = await writeOpfs(ingest.stagingKey!, asset.file).catch((error: unknown) => {
      throw storageError(error)
    })
    if (storedInOpfs) {
      ingest.phase = 'publishing'
      ingest.updatedAt = Date.now()
      await putIngest(ingest)
      const staged = await readOpfs(ingest.stagingKey!)
      const existingCandidate = await readOpfs(objectKey)
      const existing = existingCandidate?.size === 0 && staged && staged.size > 0 ? null : existingCandidate
      if (!staged || staged.size !== asset.file.size || await fingerprintBlob(staged) !== fingerprint) {
        throw new BrowserAssetStorageError('Не удалось опубликовать staged media object.', 'unavailable')
      }
      if (existing && (existing.size !== staged.size || await fingerprintBlob(existing) !== fingerprint)) {
        throw new BrowserAssetStorageError('Конфликт content-addressed media object.', 'integrity')
      }
      if (!existing) {
        if (!await writeOpfs(objectKey, staged)) throw new BrowserAssetStorageError('Не удалось опубликовать staged media object.', 'unavailable')
        const published = await readOpfs(objectKey)
        if (!published || published.size !== staged.size || await fingerprintBlob(published) !== fingerprint) {
          throw new BrowserAssetStorageError('Проверка опубликованного media object не пройдена.', 'integrity')
        }
      }
      await deleteOpfs(ingest.stagingKey!)
    }
  } catch (error) {
    await deleteOpfs(ingest.stagingKey!).catch(() => undefined)
    await clearIngest(ingest.id).catch(() => undefined)
    throw error
  }
  if (!storedInOpfs && asset.file.size > IDB_BLOB_LIMIT) {
    await clearIngest(ingest.id)
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
    objectKey,
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
      transaction.objectStore(BLOBS).put({ id: objectKey, file: asset.file } satisfies StoredBlob)
    } else {
      transaction.objectStore(BLOBS).delete(asset.id)
    }
    transaction.objectStore(INGESTS).delete(ingest.id)
    await committed
    const previousKey = previousManifest?.objectKey ?? previousManifest?.id
    if (supportsWebLocks() && previousKey && previousKey !== objectKey) {
      const referenced = (await allBrowserAssetManifests())
        .some((candidate) => (candidate.objectKey ?? candidate.id) === previousKey)
      if (!referenced) {
        if (previousManifest?.storage === 'opfs') await deleteOpfs(previousKey)
        const cleanup = await openDatabase()
        try {
          const cleanupTransaction = cleanup.transaction(BLOBS, 'readwrite')
          const cleanupCommitted = transactionDone(cleanupTransaction)
          cleanupTransaction.objectStore(BLOBS).delete(previousKey)
          await cleanupCommitted
        } finally {
          cleanup.close()
        }
      }
    }
    return manifest
  } catch (error) {
    if (manifest.storage === 'opfs') await deleteOpfs(ingest.stagingKey!).catch(() => undefined)
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
      const objectKey = manifest.objectKey ?? manifest.id
      const opfsFile = manifest.storage === 'opfs' ? await readOpfs(objectKey) : null
      const durablePresent = manifest.storage === 'opfs'
        ? opfsFile?.size === manifest.byteLength
        : manifest.storage === 'idb' && (available.has(objectKey) || available.has(manifest.id))
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
    const manifest = await requestResult<StoredBrowserAssetManifest | undefined>(
      database.transaction(MANIFESTS, 'readonly').objectStore(MANIFESTS).get(id),
    )
    const objectKey = manifest?.objectKey ?? id
    const contentBlob = await requestResult<StoredBlob | undefined>(
      database.transaction(BLOBS, 'readonly').objectStore(BLOBS).get(objectKey),
    )
    let file = manifest?.storage === 'opfs'
      ? await readOpfs(objectKey)
      : manifest?.storage === 'idb' ? contentBlob?.file : undefined
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
  recovery?: BrowserAssetRecoveryReference,
): Promise<StoredBrowserAssetManifest> {
  const current = await getBrowserAssetManifest(id)
  const fingerprint = await fingerprintBlob(file)
  const expectedFingerprint = recovery?.fingerprint ?? current?.fingerprint
  const expectedByteLength = recovery?.byteLength ?? current?.byteLength
  if (
    !expectedFingerprint
    || fingerprint !== expectedFingerprint
    || (expectedByteLength !== undefined && file.size !== expectedByteLength)
  ) {
    throw new BrowserAssetStorageError('Выбран другой файл: fingerprint не совпадает.', 'fingerprint')
  }
  if (!current && !recovery) throw new BrowserAssetStorageError('Manifest для relink не найден.', 'missing')
  const currentMatchesRecovery = !recovery || current?.fingerprint === recovery.fingerprint
  const base = current && currentMatchesRecovery ? current : {
    id: recovery!.id,
    filename: recovery!.filename,
    fileType: recovery!.fileType || file.type,
    byteLength: recovery!.byteLength ?? file.size,
    fingerprint: recovery!.fingerprint,
    storage: 'idb' as const,
    info: recovery!.info,
    createdAt: recovery!.createdAt ?? Date.now(),
  }
  try {
    const manifest = await putBrowserAsset({
      ...base,
      file,
      filename: base.filename,
      fileType: file.type || base.fileType,
      fingerprint,
    })
    if (handle) await storeExternalHandle(id, handle)
    return manifest
  } catch (error) {
    if (handle && error instanceof BrowserAssetStorageError && error.reason !== 'integrity') {
      return storeFsaLocator(base, handle)
    }
    throw error
  }
}

export async function deleteBrowserAsset(id: string): Promise<void> {
  return withStorageLock(() => deleteBrowserAssetUnlocked(id))
}

async function deleteBrowserAssetUnlocked(id: string): Promise<void> {
  const manifest = await getBrowserAssetManifest(id)
  const objectKey = manifest?.objectKey ?? id
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS, HANDLES], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).delete(id)
    transaction.objectStore(HANDLES).delete(id)
    await committed
  } finally {
    database.close()
  }
  const stillReferenced = (await allBrowserAssetManifests())
    .some((candidate) => (candidate.objectKey ?? candidate.id) === objectKey)
  if (supportsWebLocks() && !stillReferenced) {
    if (manifest?.storage === 'opfs') await deleteOpfs(objectKey)
    const cleanup = await openDatabase()
    try {
      const transaction = cleanup.transaction(BLOBS, 'readwrite')
      const committed = transactionDone(transaction)
      transaction.objectStore(BLOBS).delete(objectKey)
      transaction.objectStore(BLOBS).delete(id)
      await committed
    } finally {
      cleanup.close()
    }
  }
}
