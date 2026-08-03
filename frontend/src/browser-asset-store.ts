import type { VideoInfo } from './types'

const DATABASE = 'video-kadr-media'
const VERSION = 1
const MANIFESTS = 'manifests'
const BLOBS = 'blobs'
const OPFS_DIRECTORY = 'video-kadr-media'
const IDB_BLOB_LIMIT = 128 * 1024 * 1024
// WebCrypto has no streaming digest API. Keep the temporary implementation
// bounded until the worker-side incremental hasher lands.
const IN_MEMORY_HASH_LIMIT = 128 * 1024 * 1024

export interface StoredBrowserAssetManifest {
  id: string
  filename: string
  fileType: string
  byteLength: number
  fingerprint: string
  storage: 'opfs' | 'idb'
  info: Omit<VideoInfo, 'url'>
  createdAt: number
}

export interface AuditedBrowserAsset extends StoredBrowserAssetManifest {
  availability: 'ready' | 'offline'
}

export interface StoredBrowserAsset extends StoredBrowserAssetManifest {
  file: Blob
}

interface StoredBlob {
  id: string
  file: Blob
}

export class BrowserAssetStorageError extends Error {
  constructor(
    message: string,
    readonly reason: 'quota' | 'unavailable' | 'missing' | 'fingerprint',
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

export async function prepareBrowserStorage(requiredBytes: number): Promise<BrowserStorageEstimate> {
  const storage = navigator.storage
  if (!storage) return { persisted: false, usage: null, quota: null }
  const persisted = storage.persist ? await storage.persist().catch(() => false) : false
  const estimate: StorageEstimate = storage.estimate
    ? await storage.estimate().catch(() => ({} as StorageEstimate))
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

async function writeOpfs(id: string, blob: Blob): Promise<boolean> {
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
    request.onupgradeneeded = () => {
      const database = request.result
      if (!database.objectStoreNames.contains(MANIFESTS)) {
        database.createObjectStore(MANIFESTS, { keyPath: 'id' })
      }
      if (!database.objectStoreNames.contains(BLOBS)) {
        database.createObjectStore(BLOBS, { keyPath: 'id' })
      }
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(storageError(request.error))
    request.onblocked = () => reject(new BrowserAssetStorageError(
      'Хранилище браузера заблокировано другой вкладкой.', 'unavailable',
    ))
  })
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
  if (!crypto.subtle) {
    throw new BrowserAssetStorageError('Проверка целостности файлов недоступна.', 'unavailable')
  }
  if (blob.size > IN_MEMORY_HASH_LIMIT) {
    throw new BrowserAssetStorageError(
      'Файл больше 128 МБ: для этой версии нужен потоковый модуль проверки целостности.',
      'unavailable',
    )
  }
  const digest = await crypto.subtle.digest('SHA-256', await blob.arrayBuffer())
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, '0')).join('')
}

/** Manifest and bytes commit in the same transaction: ready never means partial. */
export async function putBrowserAsset(
  asset: Omit<StoredBrowserAsset, 'fingerprint' | 'byteLength' | 'storage'> & {
    fingerprint?: string
    byteLength?: number
  },
): Promise<StoredBrowserAssetManifest> {
  const fingerprint = asset.fingerprint ?? await fingerprintBlob(asset.file)
  const storedInOpfs = await writeOpfs(asset.id, asset.file).catch((error: unknown) => {
    throw storageError(error)
  })
  if (!storedInOpfs && asset.file.size > IDB_BLOB_LIMIT) {
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
    const transaction = database.transaction([MANIFESTS, BLOBS], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).put(manifest)
    if (manifest.storage === 'idb') {
      transaction.objectStore(BLOBS).put({ id: asset.id, file: asset.file } satisfies StoredBlob)
    } else {
      transaction.objectStore(BLOBS).delete(asset.id)
    }
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
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS], 'readonly')
    const [manifests, blobIds] = await Promise.all([
      requestResult<StoredBrowserAssetManifest[]>(transaction.objectStore(MANIFESTS).getAll()),
      requestResult<IDBValidKey[]>(transaction.objectStore(BLOBS).getAllKeys()),
    ])
    const available = new Set(blobIds.map(String))
    const audited = await Promise.all(manifests.map(async (manifest) => {
      const opfsFile = manifest.storage === 'opfs' ? await readOpfs(manifest.id) : null
      const present = manifest.storage === 'opfs'
        ? opfsFile?.size === manifest.byteLength
        : available.has(manifest.id)
      return { ...manifest, availability: present ? 'ready' as const : 'offline' as const }
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
    const file = manifest?.storage === 'opfs' ? await readOpfs(id) : storedBlob?.file
    if (!manifest || !file || (typeof file.size === 'number' && file.size !== manifest.byteLength)) {
      throw new BrowserAssetStorageError('Медиафайл отсутствует в хранилище браузера.', 'missing')
    }
    return { ...manifest, file }
  } finally {
    database.close()
  }
}

export async function relinkBrowserAsset(id: string, file: File): Promise<StoredBrowserAssetManifest> {
  const current = await getBrowserAssetManifest(id)
  if (!current) throw new BrowserAssetStorageError('Manifest для relink не найден.', 'missing')
  const fingerprint = await fingerprintBlob(file)
  if (fingerprint !== current.fingerprint || file.size !== current.byteLength) {
    throw new BrowserAssetStorageError('Выбран другой файл: fingerprint не совпадает.', 'fingerprint')
  }
  return putBrowserAsset({
    ...current,
    file,
    filename: current.filename,
    fileType: file.type || current.fileType,
    fingerprint,
  })
}

export async function deleteBrowserAsset(id: string): Promise<void> {
  const manifest = await getBrowserAssetManifest(id)
  // For OPFS, delete bytes first. If the following IDB transaction fails, the
  // retained manifest is auditable/offline and can be relinked or deleted again.
  // The inverse order could leave untracked bytes that the UI can never clean up.
  if (manifest?.storage === 'opfs') await deleteOpfs(id)
  const database = await openDatabase()
  try {
    const transaction = database.transaction([MANIFESTS, BLOBS], 'readwrite')
    const committed = transactionDone(transaction)
    transaction.objectStore(MANIFESTS).delete(id)
    transaction.objectStore(BLOBS).delete(id)
    await committed
  } finally {
    database.close()
  }
}
