import type { VideoInfo } from './types'
import { sha256 } from '@noble/hashes/sha2.js'

export type ProxyPolicy = 'auto' | 'original' | 'proxy'
export interface BrowserProxyDescriptor {
  schemaVersion: 1
  key: string
  sourceFingerprint: string
  profileFingerprint: string
  mimeType: string
  width: number
  height: number
  durationTicks: number
  sourceDurationTicks: number
  startTicks: 0
  /** Canonical project-clock ticks used for source/proxy duration mapping;
   * this is not the MediaRecorder container's native WebM timecode scale. */
  mappingTimeBase: 1_000_000
  nominalFps: number
  hasAudio: boolean
  sizeBytes: number
  artifactFingerprint: string
  createdAt: number
}
export interface BrowserProxyArtifact { descriptor: BrowserProxyDescriptor; blob: Blob }
interface StoredBrowserProxyArtifact { descriptor: BrowserProxyDescriptor; storage: 'idb' | 'opfs'; data?: ArrayBuffer; objectKey?: string }
export interface BrowserProxyProbe { duration: number; width: number; height: number }
export interface ProxyPreviewSource { url: string; usingProxy: boolean; status: 'original' | 'ready' | 'missing' | 'stale' | 'unsupported'; artifactKey?: string; mappingIdentity?: string; hasAudio?: boolean; revoke?: () => void }

const DB = 'video-kadr-proxy-artifacts', STORE = 'artifacts', MAX_BYTES = 64 * 1024 * 1024
const MAX_TOTAL_BYTES = 256 * 1024 * 1024, IDB_SAFE_BYTES = 16 * 1024 * 1024, OPFS_DIR = 'video-kadr-proxies'
let database: Promise<IDBDatabase> | undefined
let localMutationTail: Promise<void> = Promise.resolve()
function open(): Promise<IDBDatabase> { return database ??= new Promise((resolve, reject) => { const r = indexedDB.open(DB, 1); r.onupgradeneeded = () => r.result.createObjectStore(STORE, { keyPath: 'descriptor.key' }); r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error) }) }
function req<T>(r: IDBRequest<T>): Promise<T> { return new Promise((resolve, reject) => { r.onsuccess = () => resolve(r.result); r.onerror = () => reject(r.error) }) }
function done(tx: IDBTransaction): Promise<void> { return new Promise((resolve, reject) => { tx.oncomplete = () => resolve(); tx.onabort = tx.onerror = () => reject(tx.error) }) }

async function withProxyMutationLock<T>(work: () => Promise<T>): Promise<T> {
  if (navigator.locks?.request) return navigator.locks.request('video-kadr-proxy-artifacts', { mode: 'exclusive' }, work)
  const previous = localMutationTail
  let release!: () => void
  localMutationTail = new Promise<void>(resolve => { release = resolve })
  await previous
  try { return await work() } finally { release() }
}

async function proxyDirectory(create: boolean): Promise<FileSystemDirectoryHandle | null> {
  // OPFS reference counting spans tabs, so physical files are used only when
  // the origin-wide Web Locks primitive can serialize publication and GC.
  if (!navigator.storage?.getDirectory || !navigator.locks?.request) return null
  const root = await navigator.storage.getDirectory()
  try { return await root.getDirectoryHandle(OPFS_DIR, { create }) }
  catch (error) { if (!create && error instanceof DOMException && error.name === 'NotFoundError') return null; throw error }
}

async function writeProxyOpfs(objectKey: string, blob: Blob): Promise<'created' | 'reused' | false> {
  const directory = await proxyDirectory(true).catch(() => null); if (!directory) return false
  try {
    const existing = await (await directory.getFileHandle(objectKey)).getFile()
    if (existing.size === blob.size && await fingerprintBrowserProxy(existing) === objectKey.replace(/\.webm$/, '')) return 'reused'
    // Never truncate a content-addressed object that may still be referenced.
    return false
  } catch (error) {
    if (!(error instanceof DOMException && error.name === 'NotFoundError')) return false
  }
  const handle = await directory.getFileHandle(objectKey, { create: true }); const writable = await handle.createWritable()
  try { await writable.write(blob); await writable.close() }
  catch (error) { await writable.abort().catch(() => undefined); await directory.removeEntry(objectKey).catch(() => undefined); throw error }
  let storedSize: number
  try { storedSize = (await handle.getFile()).size }
  catch (error) { await directory.removeEntry(objectKey).catch(() => undefined); throw error }
  if (storedSize !== blob.size) {
    await directory.removeEntry(objectKey).catch(() => undefined)
    return false
  }
  return 'created'
}

async function readProxyOpfs(objectKey: string): Promise<File | null> {
  const directory = await proxyDirectory(false).catch(() => null); if (!directory) return null
  try { return await (await directory.getFileHandle(objectKey)).getFile() }
  catch (error) { if (error instanceof DOMException && error.name === 'NotFoundError') return null; throw error }
}

async function deleteProxyOpfs(objectKey?: string): Promise<void> {
  if (!objectKey) return
  const directory = await proxyDirectory(false).catch(() => null); if (!directory) return
  await directory.removeEntry(objectKey).catch(() => undefined)
}

async function deleteProxyOpfsIfUnreferenced(db: IDBDatabase, objectKey?: string): Promise<void> {
  if (!objectKey) return
  const records = await req(db.transaction(STORE).objectStore(STORE).getAll()) as StoredBrowserProxyArtifact[]
  if (!records.some(item => item.storage === 'opfs' && item.objectKey === objectKey)) await deleteProxyOpfs(objectKey)
}

async function removeStoredSnapshotIfCurrent(db: IDBDatabase, snapshot: StoredBrowserProxyArtifact): Promise<void> {
  const tx = db.transaction(STORE, 'readwrite'), store = tx.objectStore(STORE)
  const current = await req(store.get(snapshot.descriptor.key)) as StoredBrowserProxyArtifact | undefined
  if (!current || current.descriptor.createdAt !== snapshot.descriptor.createdAt || current.descriptor.artifactFingerprint !== snapshot.descriptor.artifactFingerprint || current.storage !== snapshot.storage || current.objectKey !== snapshot.objectKey) { await done(tx); return }
  store.delete(snapshot.descriptor.key); await done(tx)
  await deleteProxyOpfsIfUnreferenced(db, snapshot.objectKey)
}

export function browserProxyProfileFingerprint(): string { return 'vp8-640-15fps-muted-v1' }
export function browserProxyKey(sourceFingerprint: string, profile = browserProxyProfileFingerprint()): string { return `proxy:v1:${sourceFingerprint}:${profile}` }

export function browserProxyCapability(): { supported: boolean; reason: string | null } {
  if (typeof MediaRecorder === 'undefined') return { supported: false, reason: 'MediaRecorder недоступен' }
  if (typeof HTMLCanvasElement === 'undefined' || typeof HTMLCanvasElement.prototype.captureStream !== 'function') {
    return { supported: false, reason: 'Canvas captureStream недоступен' }
  }
  if (!MediaRecorder.isTypeSupported('video/webm;codecs=vp8') && !MediaRecorder.isTypeSupported('video/webm')) {
    return { supported: false, reason: 'WebM recorder недоступен' }
  }
  if (typeof indexedDB === 'undefined') return { supported: false, reason: 'IndexedDB недоступен' }
  return { supported: true, reason: null }
}

function validate(artifact: BrowserProxyArtifact): void {
  const d = artifact.descriptor
  const clonedSize = typeof artifact.blob?.size === 'number' ? artifact.blob.size : d.sizeBytes
  // fake-indexeddb does not retain native Blob accessors; real browsers do.
  if (d.schemaVersion !== 1 || !/^[a-f0-9]{64}$/.test(d.sourceFingerprint) || !/^[a-f0-9]{64}$/.test(d.artifactFingerprint) || d.mappingTimeBase !== 1_000_000 || d.startTicks !== 0 || !Number.isSafeInteger(d.durationTicks) || d.durationTicks <= 0 || !Number.isSafeInteger(d.sourceDurationTicks) || d.sourceDurationTicks <= 0 || d.sizeBytes !== clonedSize || d.sizeBytes <= 0 || d.sizeBytes > MAX_BYTES) throw new Error('invalid browser proxy artifact')
  if (d.key !== browserProxyKey(d.sourceFingerprint, d.profileFingerprint) || d.width <= 0 || d.height <= 0 || !Number.isFinite(d.nominalFps) || d.nominalFps <= 0) throw new Error('invalid browser proxy provenance')
}

export async function fingerprintBrowserProxy(blob: Blob): Promise<string> {
  const digest = sha256(new Uint8Array(await blob.arrayBuffer()))
  return Array.from(digest, byte => byte.toString(16).padStart(2, '0')).join('')
}

export function validateBrowserProxyProbe(
  probe: BrowserProxyProbe,
  expected: { duration: number; width: number; height: number },
): void {
  if (!Number.isFinite(probe.duration) || probe.duration <= 0 || probe.width !== expected.width || probe.height !== expected.height) {
    throw new Error('browser proxy media validation failed')
  }
  if (Math.abs(probe.duration - expected.duration) > Math.max(1 / 15, 0.05)) {
    throw new Error('browser proxy duration diverges from source')
  }
}

export async function probeBrowserProxyBlob(blob: Blob): Promise<BrowserProxyProbe> {
  const url = URL.createObjectURL(blob)
  const video = document.createElement('video')
  video.preload = 'metadata'; video.muted = true; video.src = url
  try {
    await new Promise<void>((resolve, reject) => {
      const timeout = window.setTimeout(() => reject(new Error('browser proxy metadata timeout')), 5_000)
      video.onloadedmetadata = () => { window.clearTimeout(timeout); resolve() }
      video.onerror = () => { window.clearTimeout(timeout); reject(new Error('browser proxy is not decodable')) }
      video.load()
    })
    if (!Number.isFinite(video.duration)) {
      await new Promise<void>((resolve) => {
        const finish = () => resolve()
        video.ondurationchange = finish; video.ontimeupdate = finish
        video.currentTime = Number.MAX_SAFE_INTEGER
        window.setTimeout(finish, 1_000)
      })
    }
    return { duration: video.duration, width: video.videoWidth, height: video.videoHeight }
  } finally {
    video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url)
  }
}

async function putBrowserProxyArtifactUnlocked(artifact: BrowserProxyArtifact): Promise<void> {
  validate(artifact)
  if (await fingerprintBrowserProxy(artifact.blob) !== artifact.descriptor.artifactFingerprint) throw new Error('browser proxy checksum mismatch')
  const estimate: StorageEstimate = await navigator.storage?.estimate?.().catch(() => ({} as StorageEstimate)) ?? {}
  if (typeof estimate?.quota === 'number' && typeof estimate.usage === 'number' && artifact.blob.size > estimate.quota - estimate.usage) throw new DOMException('Недостаточно места для proxy', 'QuotaExceededError')
  const objectKey = `${artifact.descriptor.artifactFingerprint}.webm`
  const opfsResult = await writeProxyOpfs(objectKey, artifact.blob).catch(() => false)
  const useOpfs = Boolean(opfsResult)
  if (!useOpfs && artifact.blob.size > IDB_SAFE_BYTES) throw new DOMException('Proxy слишком велик для IndexedDB; OPFS недоступен', 'QuotaExceededError')
  const data = useOpfs ? undefined : await artifact.blob.arrayBuffer()
  let db: IDBDatabase
  let existing: StoredBrowserProxyArtifact[]
  let tx: IDBTransaction
  let store: IDBObjectStore
  try {
    db = await open()
    tx = db.transaction(STORE, 'readwrite'); store = tx.objectStore(STORE)
    existing = await req(store.getAll()) as StoredBrowserProxyArtifact[]
  } catch (error) {
    if (opfsResult === 'created') await deleteProxyOpfs(objectKey)
    throw error
  }
  let total = existing.filter(item => item.descriptor.key !== artifact.descriptor.key).reduce((sum, item) => sum + item.descriptor.sizeBytes, 0)
  const evicted: StoredBrowserProxyArtifact[] = []
  for (const item of existing.filter(item => item.descriptor.key !== artifact.descriptor.key).sort((a, b) => a.descriptor.createdAt - b.descriptor.createdAt)) {
    if (total + artifact.blob.size <= MAX_TOTAL_BYTES) break
    total -= item.descriptor.sizeBytes; evicted.push(item)
  }
  for (const item of evicted) store.delete(item.descriptor.key)
  const stored: StoredBrowserProxyArtifact = { descriptor: artifact.descriptor, storage: useOpfs ? 'opfs' : 'idb', ...(data ? { data } : {}), ...(useOpfs ? { objectKey } : {}) }
  try { store.put(stored); await done(tx) }
  catch (error) { if (useOpfs) await deleteProxyOpfsIfUnreferenced(db, objectKey); throw error }
  await Promise.all(evicted.map(item => deleteProxyOpfsIfUnreferenced(db, item.objectKey)))
  const replaced = existing.find(item => item.descriptor.key === artifact.descriptor.key)
  if (replaced?.objectKey && replaced.objectKey !== stored.objectKey) await deleteProxyOpfsIfUnreferenced(db, replaced.objectKey)
}

export async function putBrowserProxyArtifact(artifact: BrowserProxyArtifact): Promise<void> {
  return withProxyMutationLock(() => putBrowserProxyArtifactUnlocked(artifact))
}

export async function getBrowserProxyArtifact(sourceFingerprint: string): Promise<BrowserProxyArtifact | null> {
  const db = await open(), stored = await req(db.transaction(STORE).objectStore(STORE).get(browserProxyKey(sourceFingerprint))) as StoredBrowserProxyArtifact | undefined
  const storage = stored?.storage ?? 'idb'
  const blob = storage === 'opfs' && stored?.objectKey
    ? await readProxyOpfs(stored.objectKey)
    : stored?.data ? new Blob([stored.data], { type: stored.descriptor.mimeType }) : null
  const value = stored && blob ? { descriptor: stored.descriptor, blob } : undefined
  if (!value) {
    if (stored) await withProxyMutationLock(() => removeStoredSnapshotIfCurrent(db, stored))
    return null
  }
  try {
    validate(value)
    if (await fingerprintBrowserProxy(value.blob) !== value.descriptor.artifactFingerprint) throw new Error('browser proxy checksum mismatch')
    return value
  } catch { if (stored) await withProxyMutationLock(() => removeStoredSnapshotIfCurrent(db, stored)); return null }
}

export async function deleteBrowserProxyArtifact(sourceFingerprint: string): Promise<void> {
  await withProxyMutationLock(async () => {
    const db = await open(), current = await req(db.transaction(STORE).objectStore(STORE).get(browserProxyKey(sourceFingerprint))) as StoredBrowserProxyArtifact | undefined
    const tx = db.transaction(STORE, 'readwrite')
    tx.objectStore(STORE).delete(browserProxyKey(sourceFingerprint)); await done(tx)
    await deleteProxyOpfsIfUnreferenced(db, current?.objectKey)
  })
}

export async function resolveBrowserPreviewSource(video: VideoInfo, policy: ProxyPolicy, capable: boolean): Promise<ProxyPreviewSource> {
  if (policy === 'original') return { url: video.url, usingProxy: false, status: 'original' }
  if (!capable) return { url: video.url, usingProxy: false, status: 'unsupported' }
  if (!video.fingerprint) return { url: video.url, usingProxy: false, status: 'stale' }
  const artifact = await getBrowserProxyArtifact(video.fingerprint)
  if (!artifact) return { url: video.url, usingProxy: false, status: 'missing' }
  // `undefined` means the browser probe could not establish audio provenance;
  // only explicit `null` proves that selecting a muted proxy is lossless.
  if (policy === 'auto' && video.acodec !== null && !artifact.descriptor.hasAudio) {
    return { url: video.url, usingProxy: false, status: 'unsupported' }
  }
  const url = URL.createObjectURL(artifact.blob)
  return { url, usingProxy: true, status: 'ready', artifactKey: artifact.descriptor.key, mappingIdentity: `${artifact.descriptor.profileFingerprint}:${artifact.descriptor.artifactFingerprint}:${artifact.descriptor.mappingTimeBase}:${url}`, hasAudio: artifact.descriptor.hasAudio, revoke: () => URL.revokeObjectURL(url) }
}
