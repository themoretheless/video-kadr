import { sha256 } from '@noble/hashes/sha2.js'

import { migrateBrandKit, type BrandKit } from './domain/brand-kit'
import { migrateProjectTemplate, type ProjectTemplate } from './domain/project-template'

const DATABASE = 'video-kadr-design'
const VERSION = 2
const TEMPLATES = 'templates'
const BRAND_KITS = 'brand-kits'
const TEMPLATE_REVISIONS = 'template-revisions'
const BRAND_KIT_REVISIONS = 'brand-kit-revisions'
const ASSETS = 'design-assets'
const BLOBS = 'design-blobs'
const QUARANTINE = 'quarantine'
const MAX_FONT_BYTES = 20 * 1024 * 1024
const MAX_LOGO_BYTES = 25 * 1024 * 1024
const MAX_TOTAL_BYTES = 256 * 1024 * 1024

export type DesignAssetKind = 'font' | 'logo'

export interface DesignRecord<T> {
  id: string
  revision: number
  createdAt: number
  updatedAt: number
  value: T
}

export interface DesignAssetManifest {
  id: string
  kind: DesignAssetKind
  name: string
  mimeType: string
  byteLength: number
  fingerprint: string
  createdAt: number
}

interface StoredBlob { fingerprint: string; byteLength: number; bytes: ArrayBuffer }
interface QuarantineRow { id: string; store: string; reason: string; value: unknown; quarantinedAt: number }
interface RevisionRecord<T> extends DesignRecord<T> { key: string }

export class DesignConflictError extends Error {
  constructor() { super('Design record revision conflict'); this.name = 'DesignConflictError' }
}

export class DesignDependencyError extends Error {
  constructor(readonly dependencies: string[]) {
    super(`Design item is still referenced by: ${dependencies.join(', ')}`)
    this.name = 'DesignDependencyError'
  }
}

export class DesignAssetError extends Error {
  constructor(message: string, readonly reason: 'type' | 'size' | 'quota' | 'missing' | 'integrity') {
    super(message); this.name = 'DesignAssetError'
  }
}

function openDatabase(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, VERSION)
    request.onupgradeneeded = () => {
      const database = request.result
      for (const name of [TEMPLATES, BRAND_KITS]) {
        if (database.objectStoreNames.contains(name)) continue
        const store = database.createObjectStore(name, { keyPath: 'id' })
        store.createIndex('updatedAt', 'updatedAt')
      }
      for (const name of [TEMPLATE_REVISIONS, BRAND_KIT_REVISIONS]) {
        if (database.objectStoreNames.contains(name)) continue
        const store = database.createObjectStore(name, { keyPath: 'key' })
        store.createIndex('id', 'id')
      }
      if (!database.objectStoreNames.contains(ASSETS)) {
        const store = database.createObjectStore(ASSETS, { keyPath: 'id' })
        store.createIndex('fingerprint', 'fingerprint')
      }
      if (!database.objectStoreNames.contains(BLOBS)) database.createObjectStore(BLOBS, { keyPath: 'fingerprint' })
      if (!database.objectStoreNames.contains(QUARANTINE)) database.createObjectStore(QUARANTINE, { keyPath: 'id' })
    }
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Unable to open design storage'))
  })
}

function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    request.onsuccess = () => resolve(request.result)
    request.onerror = () => reject(request.error ?? new Error('Design storage request failed'))
  })
}

function done(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve()
    transaction.onerror = () => reject(transaction.error ?? new Error('Design storage transaction failed'))
    transaction.onabort = () => reject(transaction.error ?? new Error('Design storage transaction aborted'))
  })
}

function validEnvelope<T>(input: unknown, parse: (value: unknown) => T): DesignRecord<T> {
  if (!input || typeof input !== 'object' || Array.isArray(input)) throw new Error('invalid design record')
  const row = input as Partial<DesignRecord<unknown>>
  if (typeof row.id !== 'string' || !row.id || !Number.isSafeInteger(row.revision) || Number(row.revision) < 1
    || !Number.isSafeInteger(row.createdAt) || Number(row.createdAt) < 0
    || !Number.isSafeInteger(row.updatedAt) || Number(row.updatedAt) < Number(row.createdAt)) {
    throw new Error('invalid design record envelope')
  }
  const value = parse(row.value)
  if ((value as { id?: unknown }).id !== row.id) throw new Error('design record identity mismatch')
  if ((value as { revision?: unknown }).revision !== row.revision) throw new Error('design record revision mismatch')
  return { id: row.id, revision: Number(row.revision), createdAt: Number(row.createdAt), updatedAt: Number(row.updatedAt), value }
}

async function quarantine(database: IDBDatabase, storeName: string, row: unknown, reason: string): Promise<void> {
  const key = row && typeof row === 'object' && typeof (row as { id?: unknown }).id === 'string'
    ? (row as { id: string }).id : crypto.randomUUID()
  const transaction = database.transaction([storeName, QUARANTINE], 'readwrite')
  const committed = done(transaction)
  transaction.objectStore(QUARANTINE).put({ id: `${storeName}:${key}`, store: storeName, reason, value: row, quarantinedAt: Date.now() } satisfies QuarantineRow)
  transaction.objectStore(storeName).delete(key)
  await committed
}

async function listValidated<T>(storeName: string, parse: (value: unknown) => T): Promise<DesignRecord<T>[]> {
  const database = await openDatabase()
  try {
    const rows = await result<unknown[]>(database.transaction(storeName).objectStore(storeName).getAll())
    const valid: DesignRecord<T>[] = []
    for (const row of rows) {
      try { valid.push(validEnvelope(row, parse)) }
      catch (error) { await quarantine(database, storeName, row, error instanceof Error ? error.message : String(error)) }
    }
    return valid.sort((left, right) => right.updatedAt - left.updatedAt || left.id.localeCompare(right.id))
  } finally { database.close() }
}

async function getValidated<T>(storeName: string, id: string, parse: (value: unknown) => T): Promise<DesignRecord<T> | null> {
  const database = await openDatabase()
  try {
    const row = await result<unknown>(database.transaction(storeName).objectStore(storeName).get(id))
    if (row === undefined) return null
    try { return validEnvelope(row, parse) }
    catch (error) {
      await quarantine(database, storeName, row, error instanceof Error ? error.message : String(error))
      return null
    }
  } finally { database.close() }
}

async function getValidatedRevision<T>(headStore: string, historyStore: string, id: string, revision: number | undefined, parse: (value: unknown) => T): Promise<DesignRecord<T> | null> {
  if (revision === undefined) return getValidated(headStore, id, parse)
  if (!Number.isSafeInteger(revision) || revision < 1) throw new Error('invalid design revision')
  const database = await openDatabase()
  try {
    const transaction = database.transaction([headStore, historyStore])
    const [historical, head] = await Promise.all([
      result<RevisionRecord<unknown> | undefined>(transaction.objectStore(historyStore).get(`${id}@${revision}`)),
      result<unknown>(transaction.objectStore(headStore).get(id)),
    ])
    const candidate = historical ?? head
    if (candidate === undefined) return null
    const decoded = validEnvelope(candidate, parse)
    return decoded.revision === revision ? decoded : null
  } finally { database.close() }
}

async function compareAndSwap<T>(
  storeName: string, historyStoreName: string, value: T, expectedRevision: number, parse: (input: unknown) => T,
): Promise<DesignRecord<T>> {
  const validated = parse(value)
  const id = (validated as { id?: unknown }).id
  if (typeof id !== 'string' || !id) throw new Error('design value has no id')
  if (!Number.isSafeInteger(expectedRevision) || expectedRevision < 0) throw new Error('invalid expected revision')
  if ((validated as { revision?: unknown }).revision !== expectedRevision + 1) throw new Error('design value revision must follow expected revision')
  const database = await openDatabase()
  try {
    return await new Promise<DesignRecord<T>>((resolve, reject) => {
      const transaction = database.transaction([storeName, historyStoreName], 'readwrite')
      const store = transaction.objectStore(storeName)
      const history = transaction.objectStore(historyStoreName)
      const read = store.get(id)
      let next: DesignRecord<T> | undefined
      read.onerror = () => reject(read.error ?? new Error('Design storage read failed'))
      read.onsuccess = () => {
        const previous = read.result as DesignRecord<unknown> | undefined
        if ((previous?.revision ?? 0) !== expectedRevision) { transaction.abort(); reject(new DesignConflictError()); return }
        const now = Date.now()
        next = { id, revision: expectedRevision + 1, createdAt: previous?.createdAt ?? now, updatedAt: now, value: validated }
        if (previous) history.put({ ...previous, key: `${id}@${previous.revision}` } satisfies RevisionRecord<unknown>)
        history.add({ ...next, key: `${id}@${next.revision}` } satisfies RevisionRecord<T>)
        store.put(next)
      }
      transaction.oncomplete = () => resolve(next!)
      transaction.onerror = () => reject(transaction.error ?? new Error('Design storage write failed'))
      transaction.onabort = () => { /* conflict rejects at the read boundary */ }
    })
  } finally { database.close() }
}

export const listProjectTemplates = (): Promise<DesignRecord<ProjectTemplate>[]> => listValidated(TEMPLATES, migrateProjectTemplate)
export const getProjectTemplate = (id: string, revision?: number): Promise<DesignRecord<ProjectTemplate> | null> => getValidatedRevision(TEMPLATES, TEMPLATE_REVISIONS, id, revision, migrateProjectTemplate)
export const saveProjectTemplate = (value: ProjectTemplate, expectedRevision: number): Promise<DesignRecord<ProjectTemplate>> => compareAndSwap(TEMPLATES, TEMPLATE_REVISIONS, value, expectedRevision, migrateProjectTemplate)
export const listBrandKits = (): Promise<DesignRecord<BrandKit>[]> => listValidated(BRAND_KITS, migrateBrandKit)
export const getBrandKit = (id: string, revision?: number): Promise<DesignRecord<BrandKit> | null> => getValidatedRevision(BRAND_KITS, BRAND_KIT_REVISIONS, id, revision, migrateBrandKit)
export const saveBrandKit = (value: BrandKit, expectedRevision: number): Promise<DesignRecord<BrandKit>> => compareAndSwap(BRAND_KITS, BRAND_KIT_REVISIONS, value, expectedRevision, migrateBrandKit)

function references(value: unknown, key: string, id: string): boolean {
  if (Array.isArray(value)) return value.some(item => references(item, key, id))
  if (!value || typeof value !== 'object') return false
  return Object.entries(value as Record<string, unknown>).some(([nestedKey, nested]) => nestedKey === key && nested === id || references(nested, key, id))
}

async function safeDelete(storeName: string, id: string): Promise<boolean> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction(storeName, 'readwrite')
    const existing = await result<unknown>(transaction.objectStore(storeName).get(id))
    if (existing === undefined) return false
    transaction.objectStore(storeName).delete(id)
    await done(transaction)
    return true
  } finally { database.close() }
}

export const deleteProjectTemplate = (id: string): Promise<boolean> => safeDelete(TEMPLATES, id)
export async function deleteBrandKit(id: string): Promise<boolean> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction([TEMPLATES, BRAND_KITS], 'readwrite')
    const [templates, existing] = await Promise.all([
      result<DesignRecord<unknown>[]>(transaction.objectStore(TEMPLATES).getAll()),
      result<unknown>(transaction.objectStore(BRAND_KITS).get(id)),
    ])
    const refs = templates.filter(row => {
      const pin = row.value && typeof row.value === 'object' ? (row.value as { brandKitPin?: { id?: unknown } }).brandKitPin : undefined
      return pin?.id === id
    }).map(row => `template:${row.id}`)
    if (refs.length) { transaction.abort(); throw new DesignDependencyError(refs) }
    if (existing === undefined) { transaction.abort(); return false }
    transaction.objectStore(BRAND_KITS).delete(id)
    await done(transaction)
    return true
  } finally { database.close() }
}

function hex(bytes: Uint8Array): string { return [...bytes].map(byte => byte.toString(16).padStart(2, '0')).join('') }

async function inspectAsset(file: Blob, kind: DesignAssetKind): Promise<{ fingerprint: string; bytes: ArrayBuffer }> {
  const maximum = kind === 'font' ? MAX_FONT_BYTES : MAX_LOGO_BYTES
  if (file.size < 1 || file.size > maximum) throw new DesignAssetError('Design asset size is invalid', 'size')
  const buffer = await file.arrayBuffer()
  const bytes = new Uint8Array(buffer)
  const ascii = String.fromCharCode(...bytes.slice(0, 16))
  if (kind === 'font') {
    if (file.type !== 'font/woff' && file.type !== 'font/woff2') throw new DesignAssetError('Unsupported font MIME type', 'type')
    if (!(file.type === 'font/woff' && ascii.startsWith('wOFF')) && !(file.type === 'font/woff2' && ascii.startsWith('wOF2'))) throw new DesignAssetError('Font signature does not match MIME type', 'type')
  } else {
    const png = bytes.length >= 8 && [137, 80, 78, 71, 13, 10, 26, 10].every((value, index) => bytes[index] === value)
    const webp = ascii.startsWith('RIFF') && ascii.slice(8, 12) === 'WEBP'
    if (!(file.type === 'image/png' && png) && !(file.type === 'image/webp' && webp)) throw new DesignAssetError('Unsupported logo MIME or signature', 'type')
  }
  return { fingerprint: hex(sha256(bytes)), bytes: buffer }
}

export async function putDesignAsset(id: string, name: string, kind: DesignAssetKind, file: Blob): Promise<DesignAssetManifest> {
  if (!id.trim() || !name.trim()) throw new DesignAssetError('Design asset identity is invalid', 'type')
  const { fingerprint, bytes } = await inspectAsset(file, kind)
  const manifest: DesignAssetManifest = { id, kind, name: name.trim(), mimeType: file.type, byteLength: file.size, fingerprint, createdAt: Date.now() }
  const database = await openDatabase()
  try {
    const transaction = database.transaction([ASSETS, BLOBS], 'readwrite')
    const assets = transaction.objectStore(ASSETS)
    const blobs = transaction.objectStore(BLOBS)
    const [existing, storedBlobs] = await Promise.all([
      result<DesignAssetManifest | undefined>(assets.get(id)), result<StoredBlob[]>(blobs.getAll()),
    ])
    if (existing && existing.fingerprint !== fingerprint) { transaction.abort(); throw new DesignConflictError() }
    const alreadyStored = storedBlobs.some(row => row.fingerprint === fingerprint)
    const used = storedBlobs.reduce((total, row) => total + row.byteLength, 0)
    if (!alreadyStored && used + file.size > MAX_TOTAL_BYTES) { transaction.abort(); throw new DesignAssetError('Design asset quota exceeded', 'quota') }
    if (!alreadyStored) blobs.add({ fingerprint, byteLength: file.size, bytes } satisfies StoredBlob)
    assets.put(existing ?? manifest)
    await done(transaction)
    return existing ?? manifest
  } finally { database.close() }
}

export async function listDesignAssets(): Promise<DesignAssetManifest[]> {
  const database = await openDatabase()
  try { return await result(database.transaction(ASSETS).objectStore(ASSETS).getAll()) }
  finally { database.close() }
}

export async function getDesignAsset(id: string): Promise<{ manifest: DesignAssetManifest; blob: Blob } | null> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction([ASSETS, BLOBS])
    const manifest = await result<DesignAssetManifest | undefined>(transaction.objectStore(ASSETS).get(id))
    if (!manifest) return null
    const stored = await result<StoredBlob | undefined>(transaction.objectStore(BLOBS).get(manifest.fingerprint))
    if (!stored || stored.byteLength !== manifest.byteLength || stored.bytes.byteLength !== manifest.byteLength) throw new DesignAssetError('Design asset bytes are missing or corrupt', 'integrity')
    return { manifest, blob: new Blob([stored.bytes], { type: manifest.mimeType }) }
  } finally { database.close() }
}

export async function deleteDesignAsset(id: string): Promise<boolean> {
  const database = await openDatabase()
  try {
    const transaction = database.transaction([TEMPLATES, BRAND_KITS, ASSETS, BLOBS], 'readwrite')
    const [manifest, manifests, templates, kits] = await Promise.all([
      result<DesignAssetManifest | undefined>(transaction.objectStore(ASSETS).get(id)),
      result<DesignAssetManifest[]>(transaction.objectStore(ASSETS).getAll()),
      result<DesignRecord<unknown>[]>(transaction.objectStore(TEMPLATES).getAll()),
      result<DesignRecord<unknown>[]>(transaction.objectStore(BRAND_KITS).getAll()),
    ])
    if (!manifest) { transaction.abort(); return false }
    const candidates = [...templates.map(row => ({ prefix: 'template', row })), ...kits.map(row => ({ prefix: 'brand-kit', row }))]
    const refs = candidates.filter(({ row }) => references(row.value, 'assetRef', id) || references(row.value, 'fingerprint', manifest.fingerprint))
      .map(({ prefix, row }) => `${prefix}:${row.id}`)
    if (refs.length) { transaction.abort(); throw new DesignDependencyError([...new Set(refs)]) }
    transaction.objectStore(ASSETS).delete(id)
    if (!manifests.some(row => row.id !== id && row.fingerprint === manifest.fingerprint)) transaction.objectStore(BLOBS).delete(manifest.fingerprint)
    await done(transaction)
    return true
  } finally { database.close() }
}
