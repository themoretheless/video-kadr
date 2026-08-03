import { parseCubeLut, sha256Hex, type ParsedCubeLut } from './domain/cube-lut'

export const BROWSER_LUT_LIBRARY_CONTRACT = 'browser-lut-library-v1' as const
const DATABASE = 'video-kadr-luts'
const VERSION = 1
const ASSETS = 'assets'
const BLOBS = 'blobs'
const PREFERENCES = 'preferences'
const MAX_NAME_CHARS = 256
const MAX_QUERY_CHARS = 256
const MAX_LIST_LIMIT = 100

export interface BrowserLutAsset {
  schemaVersion: 1
  id: string
  sha256: string
  name: string
  cubeSize: number
  sizeBytes: number
  source: 'upload' | 'baked'
  createdAt: number
}

interface BrowserLutPreference {
  sha256: string
  favorite: boolean
  updatedAt: number
}

export interface BrowserLutListOptions {
  query?: string
  favorite?: boolean
  limit?: number
}

export interface BrowserLutListItem extends BrowserLutAsset {
  favorite: boolean
}

export class BrowserLutStorageError extends Error {
  constructor(public readonly reason: 'unsupported' | 'missing' | 'integrity' | 'invalid') {
    super(`Browser LUT storage error: ${reason}`)
    this.name = 'BrowserLutStorageError'
  }
}

let opening: Promise<IDBDatabase> | null = null

function requestResult<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error)
  })
}

function transactionDone(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onabort = () => reject(transaction.error ?? new Error('LUT transaction aborted'))
    transaction.onerror = () => reject(transaction.error)
  })
}

function openDatabase(): Promise<IDBDatabase> {
  if (typeof indexedDB === 'undefined') return Promise.reject(new BrowserLutStorageError('unsupported'))
  opening ??= new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = () => {
      const database = request.result
      const assets = database.createObjectStore(ASSETS, { keyPath: 'sha256' })
      assets.createIndex('id', 'id', { unique: true })
      assets.createIndex('createdAt', 'createdAt')
      database.createObjectStore(BLOBS)
      database.createObjectStore(PREFERENCES, { keyPath: 'sha256' })
    }
    request.onsuccess = () => {
      request.result.onversionchange = () => request.result.close()
      resolve(request.result)
    }
    request.onerror = () => reject(request.error)
    request.onblocked = () => reject(new BrowserLutStorageError('unsupported'))
  })
  return opening
}

/** Close the cached connection; primarily useful for logout/tests/schema upgrades. */
export async function closeBrowserLutLibrary(): Promise<void> {
  const pending = opening
  opening = null
  if (pending) (await pending).close()
}

function contentId(sha: string): string {
  // Stable UUID-shaped local handle; SHA remains the authoritative identity.
  return `${sha.slice(0, 8)}-${sha.slice(8, 12)}-5${sha.slice(13, 16)}-${((Number.parseInt(sha[16]!, 16) & 3) | 8).toString(16)}${sha.slice(17, 20)}-${sha.slice(20, 32)}`
}

function displayName(name: string): string {
  const normalized = name.normalize('NFC').trim().replace(/[\p{Cc}]/gu, '')
  if (!normalized) return 'Untitled LUT'
  return [...normalized].slice(0, MAX_NAME_CHARS).join('')
}

function searchKey(value: string): string {
  return value.normalize('NFKC').toLocaleLowerCase().trim()
}

export async function putBrowserLut(
  input: Blob | Uint8Array | ArrayBuffer | string,
  name: string,
  source: BrowserLutAsset['source'] = 'upload',
  createdAt = Date.now(),
): Promise<BrowserLutAsset> {
  let bytes: Uint8Array
  if (input instanceof Blob) bytes = new Uint8Array(await input.arrayBuffer())
  else if (typeof input === 'string') bytes = new TextEncoder().encode(input)
  else bytes = input instanceof Uint8Array ? input : new Uint8Array(input)
  let parsed: ParsedCubeLut
  try {
    parsed = parseCubeLut(bytes)
  } catch (error) {
    throw error instanceof Error ? error : new BrowserLutStorageError('invalid')
  }
  const database = await openDatabase()
  const transaction = database.transaction([ASSETS, BLOBS], 'readwrite')
  const assets = transaction.objectStore(ASSETS)
  const existing = await requestResult(assets.get(parsed.sha256)) as BrowserLutAsset | undefined
  if (existing) {
    return existing
  }
  const asset: BrowserLutAsset = {
    schemaVersion: 1,
    id: contentId(parsed.sha256),
    sha256: parsed.sha256,
    name: displayName(name),
    cubeSize: parsed.cubeSize,
    sizeBytes: parsed.canonicalBytes.byteLength,
    source,
    createdAt: Number.isSafeInteger(createdAt) && createdAt >= 0 ? createdAt : Date.now(),
  }
  assets.add(asset)
  transaction.objectStore(BLOBS).add(parsed.canonicalBytes.slice().buffer, parsed.sha256)
  await transactionDone(transaction)
  return asset
}

async function resolveAsset(database: IDBDatabase, idOrSha: string): Promise<BrowserLutAsset | undefined> {
  const transaction = database.transaction(ASSETS, 'readonly')
  const store = transaction.objectStore(ASSETS)
  return idOrSha.length === 64
    ? await requestResult(store.get(idOrSha)) as BrowserLutAsset | undefined
    : await requestResult(store.index('id').get(idOrSha)) as BrowserLutAsset | undefined
}

/** Read and re-hash immutable bytes; corruption never reaches the renderer. */
export async function getBrowserLut(idOrSha: string): Promise<{ asset: BrowserLutAsset; blob: Blob }> {
  const database = await openDatabase()
  const asset = await resolveAsset(database, idOrSha)
  if (!asset) throw new BrowserLutStorageError('missing')
  const stored = await requestResult(database.transaction(BLOBS, 'readonly').objectStore(BLOBS).get(asset.sha256)) as ArrayBuffer | Uint8Array | Blob | undefined
  const bytes = stored instanceof ArrayBuffer
    ? new Uint8Array(stored)
    : stored instanceof Uint8Array
      ? stored
      : stored instanceof Blob
        ? new Uint8Array(await stored.arrayBuffer())
        : null
  if (!bytes || bytes.byteLength !== asset.sizeBytes) throw new BrowserLutStorageError('integrity')
  if (sha256Hex(bytes) !== asset.sha256) throw new BrowserLutStorageError('integrity')
  return { asset, blob: new Blob([bytes.slice().buffer], { type: 'application/x-cube' }) }
}

export async function setBrowserLutFavorite(idOrSha: string, favorite: boolean): Promise<void> {
  const database = await openDatabase()
  const asset = await resolveAsset(database, idOrSha)
  if (!asset) throw new BrowserLutStorageError('missing')
  const transaction = database.transaction(PREFERENCES, 'readwrite')
  if (favorite) transaction.objectStore(PREFERENCES).put({ sha256: asset.sha256, favorite: true, updatedAt: Date.now() } satisfies BrowserLutPreference)
  else transaction.objectStore(PREFERENCES).delete(asset.sha256)
  await transactionDone(transaction)
}

export async function listBrowserLuts(options: BrowserLutListOptions = {}): Promise<BrowserLutListItem[]> {
  const database = await openDatabase()
  const transaction = database.transaction([ASSETS, PREFERENCES], 'readonly')
  const [assets, preferences] = await Promise.all([
    requestResult(transaction.objectStore(ASSETS).getAll()) as Promise<BrowserLutAsset[]>,
    requestResult(transaction.objectStore(PREFERENCES).getAll()) as Promise<BrowserLutPreference[]>,
  ])
  const favoriteShas = new Set(preferences.filter(item => item.favorite).map(item => item.sha256))
  const query = [...searchKey(options.query ?? '')].slice(0, MAX_QUERY_CHARS).join('')
  const requestedLimit = options.limit ?? MAX_LIST_LIMIT
  const limit = Number.isFinite(requestedLimit)
    ? Math.max(1, Math.min(MAX_LIST_LIMIT, Math.floor(requestedLimit)))
    : MAX_LIST_LIMIT
  return assets
    .map(asset => ({ ...asset, favorite: favoriteShas.has(asset.sha256) }))
    .filter(asset => (!query || searchKey(asset.name).includes(query)) && (options.favorite === undefined || asset.favorite === options.favorite))
    .sort((left, right) => Number(right.favorite) - Number(left.favorite) || right.createdAt - left.createdAt || left.sha256.localeCompare(right.sha256))
    .slice(0, limit)
}
