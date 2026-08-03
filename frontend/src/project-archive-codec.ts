import { sha256 } from '@noble/hashes/sha2.js'
import { migrateProjectDocument, type ProjectDocument } from './project-schema'

export const VKADR_MAGIC = new Uint8Array([0x56, 0x4b, 0x41, 0x44, 0x52, 0x76, 0x31, 0x0a]) // VKADRv1\n
export const VKADR_HEADER_BYTES = 12
export const VKADR_MAX_MANIFEST_BYTES = 16 * 1024 * 1024
export const PORTABLE_ARCHIVE_SCHEMA_VERSION = 1 as const

export type PortableArchiveEntryKind = 'project' | 'media' | 'lut' | 'proxy'

export interface PortableArchiveProxyProvenance {
  sourceSha256: string
  profileFingerprint: string
  rendererCompatibility: string
}

export interface PortableArchiveEntry {
  path: string
  kind: PortableArchiveEntryKind
  sizeBytes: number
  sha256: string
  /** Rust serializes `None` as JSON null, so this field is never omitted. */
  proxyProvenance: PortableArchiveProxyProvenance | null
}

export interface PortableArchiveManifest {
  schemaVersion: typeof PORTABLE_ARCHIVE_SCHEMA_VERSION
  project: Record<string, unknown>
  entries: PortableArchiveEntry[]
  rootHash: string
}

export interface PortableArchiveLimits {
  maxEntries: number
  maxEntryBytes: number
  maxTotalBytes: number
  maxManifestBytes: number
}

export const DEFAULT_PORTABLE_ARCHIVE_LIMITS: PortableArchiveLimits = {
  maxEntries: 16_384,
  maxEntryBytes: 256 * 1024 * 1024 * 1024,
  maxTotalBytes: 1024 * 1024 * 1024 * 1024,
  maxManifestBytes: VKADR_MAX_MANIFEST_BYTES,
}

export interface PortableArchivePayloadInput {
  path: string
  kind: Exclude<PortableArchiveEntryKind, 'project'>
  blob: Blob
  proxyProvenance?: PortableArchiveProxyProvenance | null
}

export interface PortableArchiveDecoded {
  manifest: PortableArchiveManifest
  project: ProjectDocument
  payloads: ReadonlyMap<string, Blob>
}

export interface PortableArchiveCodecProgress {
  phase: 'hashing' | 'verifying'
  completedBytes: number
  totalBytes: number
  path: string
}

export class PortableArchiveError extends Error {
  constructor(
    readonly code:
      | 'cancelled'
      | 'wrong_format'
      | 'unsupported_version'
      | 'invalid_manifest'
      | 'unsafe_path'
      | 'too_large'
      | 'truncated'
      | 'trailing_bytes'
      | 'checksum_mismatch',
    message: string,
    options?: ErrorOptions,
  ) {
    super(message, options)
    this.name = 'PortableArchiveError'
  }
}

const encoder = new TextEncoder()
const decoder = new TextDecoder('utf-8', { fatal: true })
const HEX_SHA256 = /^[a-f0-9]{64}$/i

function abortIfNeeded(signal?: AbortSignal): void {
  if (signal?.aborted) throw new PortableArchiveError('cancelled', 'Операция с архивом отменена.')
}

function hex(bytes: Uint8Array): string {
  return Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')
}

function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  return left.length === right.length && left.every((byte, index) => byte === right[index])
}

/** Cross-language root encoding used by Rust: keys follow JavaScript UTF-16
 * ordering and every finite number is represented by normalized IEEE-754 bits. */
export function rustCanonicalJson(value: unknown): string {
  const write = (item: unknown): string => {
    if (Array.isArray(item)) return `[${item.map(write).join(',')}]`
    if (item && typeof item === 'object') {
      const parts: string[] = []
      for (const key of Object.keys(item as Record<string, unknown>).sort()) {
        const nested = (item as Record<string, unknown>)[key]
        if (nested !== undefined) parts.push(`${JSON.stringify(key)}:${write(nested)}`)
      }
      return `{${parts.join(',')}}`
    }
    if (typeof item === 'number') {
      if (!Number.isFinite(item) || (Number.isInteger(item) && !Number.isSafeInteger(item))) {
        throw new PortableArchiveError('invalid_manifest', 'Архив содержит некорректное число.')
      }
      const bytes = new Uint8Array(8)
      new DataView(bytes.buffer).setFloat64(0, Object.is(item, -0) ? 0 : item, false)
      return `~${Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('')}`
    }
    const scalar = JSON.stringify(item)
    if (scalar === undefined) throw new PortableArchiveError('invalid_manifest', 'Архив содержит неподдерживаемое значение.')
    return scalar
  }
  return write(value)
}

function wireEntries(entries: readonly PortableArchiveEntry[]): Array<Record<string, unknown>> {
  return entries.map(entry => ({
    path: entry.path,
    kind: entry.kind,
    sizeBytes: entry.sizeBytes,
    sha256: entry.sha256,
    proxyProvenance: entry.proxyProvenance === null ? null : {
      sourceSha256: entry.proxyProvenance.sourceSha256,
      profileFingerprint: entry.proxyProvenance.profileFingerprint,
      rendererCompatibility: entry.proxyProvenance.rendererCompatibility,
    },
  }))
}

function projectPayloadJson(value: unknown): string {
  const canonical = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(canonical)
    if (item && typeof item === 'object') {
      const result: Record<string, unknown> = {}
      for (const key of Object.keys(item as Record<string, unknown>).sort()) {
        const nested = (item as Record<string, unknown>)[key]
        if (nested !== undefined) result[key] = canonical(nested)
      }
      return result
    }
    return item
  }
  return JSON.stringify(canonical(value))
}

function u32be(value: number): Uint8Array {
  const bytes = new Uint8Array(4)
  new DataView(bytes.buffer).setUint32(0, value, false)
  return bytes
}

function u64be(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new PortableArchiveError('too_large', 'Размер архива превышает безопасный диапазон браузера.')
  }
  const bytes = new Uint8Array(8)
  new DataView(bytes.buffer).setBigUint64(0, BigInt(value), false)
  return bytes
}

/** Byte-for-byte mirror of backend `portable_archive::root_hash`. */
export function portableArchiveRootHash(
  schemaVersion: number,
  project: Record<string, unknown>,
  entries: readonly PortableArchiveEntry[],
): string {
  const schema = u32be(schemaVersion)
  const ordered = [...entries].sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0)
  const parts = [
    encoder.encode('portable-project-v1'),
    schema,
    encoder.encode(rustCanonicalJson(project)),
    encoder.encode(rustCanonicalJson(wireEntries(ordered))),
  ]
  const hash = sha256.create()
  for (const part of parts) {
    hash.update(u64be(part.byteLength))
    hash.update(part)
  }
  return hex(hash.digest())
}

async function hashBlob(
  blob: Blob,
  signal: AbortSignal | undefined,
  onChunk: (bytes: number) => void,
): Promise<string> {
  const hash = sha256.create()
  const stream = typeof blob.stream === 'function' ? blob.stream() : null
  if (stream) {
    const reader = stream.getReader()
    try {
      while (true) {
        abortIfNeeded(signal)
        const { done, value } = await reader.read()
        if (done) break
        hash.update(value)
        onChunk(value.byteLength)
      }
    } finally {
      reader.releaseLock()
    }
  } else {
    const chunkBytes = 4 * 1024 * 1024
    for (let offset = 0; offset < blob.size; offset += chunkBytes) {
      abortIfNeeded(signal)
      const bytes = new Uint8Array(await blob.slice(offset, offset + chunkBytes).arrayBuffer())
      hash.update(bytes)
      onChunk(bytes.byteLength)
    }
  }
  abortIfNeeded(signal)
  return hex(hash.digest())
}

function isSafeToken(value: string): boolean {
  return value.length > 0 && value.length <= 128 && /^[A-Za-z0-9._:-]+$/.test(value)
}

function isSafeExtension(value: string): boolean {
  return value.length > 0 && value.length <= 12 && /^[A-Za-z0-9]+$/.test(value)
}

function validateEntryPath(entry: PortableArchiveEntry): void {
  const parts = entry.path.split('/')
  if (
    !entry.path
    || entry.path.length > 512
    || [...entry.path].some(character => character.codePointAt(0)! > 0x7f)
    || entry.path.includes('\\')
    || parts.some(part => !part || part === '.' || part === '..')
  ) throw new PortableArchiveError('unsafe_path', `Небезопасный путь в архиве: ${entry.path || 'пустой'}.`)

  let valid = false
  if (entry.kind === 'project') valid = entry.path === 'project.json'
  else if (entry.kind === 'media') {
    const file = parts.length === 2 && parts[0] === 'media' ? parts[1]! : ''
    const dot = file.lastIndexOf('.')
    valid = dot > 0 && file.slice(0, dot).toLowerCase() === entry.sha256.toLowerCase() && isSafeExtension(file.slice(dot + 1))
  } else if (entry.kind === 'lut') {
    valid = parts.length === 2 && parts[0] === 'luts' && parts[1] === `${entry.sha256}.cube`
  } else if (entry.kind === 'proxy') {
    const file = parts.length === 3 && parts[0] === 'proxies' && isSafeToken(parts[1]!) ? parts[2]! : ''
    const dot = file.lastIndexOf('.')
    valid = dot > 0 && file.slice(0, dot).toLowerCase() === entry.sha256.toLowerCase() && isSafeExtension(file.slice(dot + 1))
  }
  if (!valid) throw new PortableArchiveError('unsafe_path', `Путь ${entry.path} не соответствует типу ${entry.kind}.`)
}

function validateProvenance(entry: PortableArchiveEntry): void {
  if (entry.kind !== 'proxy') {
    if (entry.proxyProvenance !== null) throw new PortableArchiveError('invalid_manifest', 'Proxy provenance указан не для proxy.')
    return
  }
  const value = entry.proxyProvenance
  if (
    !value
    || !HEX_SHA256.test(value.sourceSha256)
    || !HEX_SHA256.test(value.profileFingerprint)
    || !isSafeToken(value.rendererCompatibility)
  ) throw new PortableArchiveError('invalid_manifest', 'В архиве повреждён provenance proxy.')
}

function exactKeys(value: Record<string, unknown>, keys: readonly string[], label: string): void {
  const actual = Object.keys(value).sort()
  const expected = [...keys].sort()
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    throw new PortableArchiveError('invalid_manifest', `${label} содержит неизвестные или пропущенные поля.`)
  }
}

function parseEntry(value: unknown): PortableArchiveEntry {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new PortableArchiveError('invalid_manifest', 'Некорректная запись manifest.')
  const object = value as Record<string, unknown>
  exactKeys(object, ['path', 'kind', 'sizeBytes', 'sha256', 'proxyProvenance'], 'Запись manifest')
  if (
    typeof object.path !== 'string'
    || !['project', 'media', 'lut', 'proxy'].includes(String(object.kind))
    || !Number.isSafeInteger(object.sizeBytes)
    || Number(object.sizeBytes) <= 0
    || typeof object.sha256 !== 'string'
    || !HEX_SHA256.test(object.sha256)
  ) throw new PortableArchiveError('invalid_manifest', 'Некорректная запись manifest.')
  let provenance: PortableArchiveProxyProvenance | null = null
  if (object.proxyProvenance !== null) {
    if (!object.proxyProvenance || typeof object.proxyProvenance !== 'object' || Array.isArray(object.proxyProvenance)) {
      throw new PortableArchiveError('invalid_manifest', 'Некорректный proxy provenance.')
    }
    const raw = object.proxyProvenance as Record<string, unknown>
    exactKeys(raw, ['sourceSha256', 'profileFingerprint', 'rendererCompatibility'], 'Proxy provenance')
    if (typeof raw.sourceSha256 !== 'string' || typeof raw.profileFingerprint !== 'string' || typeof raw.rendererCompatibility !== 'string') {
      throw new PortableArchiveError('invalid_manifest', 'Некорректный proxy provenance.')
    }
    provenance = {
      sourceSha256: raw.sourceSha256,
      profileFingerprint: raw.profileFingerprint,
      rendererCompatibility: raw.rendererCompatibility,
    }
  }
  return {
    path: object.path,
    kind: object.kind as PortableArchiveEntryKind,
    sizeBytes: object.sizeBytes as number,
    sha256: object.sha256,
    proxyProvenance: provenance,
  }
}

function validateManifest(
  value: unknown,
  limits: PortableArchiveLimits,
): { manifest: PortableArchiveManifest; project: ProjectDocument; payloadBytes: number } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new PortableArchiveError('invalid_manifest', 'Manifest архива повреждён.')
  const object = value as Record<string, unknown>
  exactKeys(object, ['schemaVersion', 'project', 'entries', 'rootHash'], 'Manifest')
  if (object.schemaVersion !== PORTABLE_ARCHIVE_SCHEMA_VERSION) {
    throw new PortableArchiveError('unsupported_version', 'Архив создан более новой или неподдерживаемой версией приложения.')
  }
  if (!object.project || typeof object.project !== 'object' || Array.isArray(object.project) || !Array.isArray(object.entries)) {
    throw new PortableArchiveError('invalid_manifest', 'Manifest архива повреждён.')
  }
  if (object.entries.length > limits.maxEntries) throw new PortableArchiveError('too_large', 'В архиве слишком много файлов.')
  const entries = object.entries.map(parseEntry)
  const paths = new Set<string>()
  const folded = new Set<string>()
  let payloadBytes = 0
  let projectEntries = 0
  for (const entry of entries) {
    if (entry.sizeBytes > limits.maxEntryBytes) throw new PortableArchiveError('too_large', `Файл ${entry.path} превышает лимит.`)
    payloadBytes += entry.sizeBytes
    if (!Number.isSafeInteger(payloadBytes) || payloadBytes > limits.maxTotalBytes) throw new PortableArchiveError('too_large', 'Архив превышает допустимый размер.')
    validateEntryPath(entry)
    validateProvenance(entry)
    const lower = entry.path.toLowerCase()
    if (paths.has(entry.path) || folded.has(lower)) throw new PortableArchiveError('invalid_manifest', 'Manifest содержит повторяющийся путь.')
    paths.add(entry.path); folded.add(lower)
    if (entry.kind === 'project') projectEntries++
  }
  if (projectEntries !== 1) throw new PortableArchiveError('invalid_manifest', 'Архив должен содержать ровно один project.json.')
  for (const path of paths) {
    const components = path.split('/')
    for (let index = 1; index < components.length; index++) {
      if (paths.has(components.slice(0, index).join('/'))) throw new PortableArchiveError('unsafe_path', 'В архиве конфликтуют файл и каталог.')
    }
  }
  if (typeof object.rootHash !== 'string' || !HEX_SHA256.test(object.rootHash)) throw new PortableArchiveError('invalid_manifest', 'Некорректный root hash архива.')
  const projectValue = object.project as Record<string, unknown>
  const rootHash = portableArchiveRootHash(PORTABLE_ARCHIVE_SCHEMA_VERSION, projectValue, entries)
  if (rootHash.toLowerCase() !== object.rootHash.toLowerCase()) throw new PortableArchiveError('checksum_mismatch', 'Manifest архива не прошёл проверку целостности.')
  let project: ProjectDocument
  try { project = migrateProjectDocument(projectValue) }
  catch (error) { throw new PortableArchiveError('invalid_manifest', 'Проект внутри архива повреждён или создан более новой версией.', { cause: error }) }
  return {
    manifest: { schemaVersion: PORTABLE_ARCHIVE_SCHEMA_VERSION, project: projectValue, entries, rootHash: object.rootHash },
    project,
    payloadBytes,
  }
}

export async function buildPortableArchive(
  projectInput: ProjectDocument,
  payloadInputs: readonly PortableArchivePayloadInput[],
  options: {
    limits?: PortableArchiveLimits
    signal?: AbortSignal
    onProgress?: (progress: PortableArchiveCodecProgress) => void
  } = {},
): Promise<{ blob: Blob; manifest: PortableArchiveManifest }> {
  const limits = options.limits ?? DEFAULT_PORTABLE_ARCHIVE_LIMITS
  const project = migrateProjectDocument(projectInput) as Record<string, unknown>
  const projectBytes = encoder.encode(projectPayloadJson(project))
  const projectBlob = new Blob([projectBytes], { type: 'application/json' })
  const inputs = [
    { path: 'project.json', kind: 'project' as const, blob: projectBlob, proxyProvenance: null },
    ...payloadInputs.map(input => ({ ...input, proxyProvenance: input.proxyProvenance ?? null })),
  ].sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0)
  if (inputs.length > limits.maxEntries) throw new PortableArchiveError('too_large', 'В архиве слишком много файлов.')
  const totalBytes = inputs.reduce((sum, input) => sum + input.blob.size, 0)
  if (!Number.isSafeInteger(totalBytes) || totalBytes > limits.maxTotalBytes) throw new PortableArchiveError('too_large', 'Архив превышает допустимый размер.')
  let completedBytes = 0
  const entries: PortableArchiveEntry[] = []
  for (const input of inputs) {
    abortIfNeeded(options.signal)
    if (input.blob.size <= 0 || input.blob.size > limits.maxEntryBytes) throw new PortableArchiveError('too_large', `Файл ${input.path} имеет недопустимый размер.`)
    const digest = await hashBlob(input.blob, options.signal, bytes => {
      completedBytes += bytes
      options.onProgress?.({ phase: 'hashing', completedBytes, totalBytes, path: input.path })
    })
    entries.push({
      path: input.path,
      kind: input.kind,
      sizeBytes: input.blob.size,
      sha256: digest,
      proxyProvenance: input.proxyProvenance,
    })
  }
  const rootHash = portableArchiveRootHash(PORTABLE_ARCHIVE_SCHEMA_VERSION, project, entries)
  const manifest: PortableArchiveManifest = { schemaVersion: PORTABLE_ARCHIVE_SCHEMA_VERSION, project, entries, rootHash }
  validateManifest(manifest, limits)
  const manifestBytes = encoder.encode(JSON.stringify(manifest))
  if (manifestBytes.byteLength <= 0 || manifestBytes.byteLength > limits.maxManifestBytes || manifestBytes.byteLength > 0xffff_ffff) {
    throw new PortableArchiveError('too_large', 'Manifest архива превышает допустимый размер.')
  }
  const header = new Uint8Array(VKADR_HEADER_BYTES)
  header.set(VKADR_MAGIC, 0)
  new DataView(header.buffer).setUint32(8, manifestBytes.byteLength, false)
  const blobs = new Map(inputs.map(input => [input.path, input.blob]))
  const parts: BlobPart[] = [header, manifestBytes]
  for (const entry of entries) parts.push(blobs.get(entry.path)!)
  return { blob: new Blob(parts, { type: 'application/vnd.video-kadr.project' }), manifest }
}

export async function readPortableArchive(
  archive: Blob,
  options: {
    limits?: PortableArchiveLimits
    signal?: AbortSignal
    onProgress?: (progress: PortableArchiveCodecProgress) => void
  } = {},
): Promise<PortableArchiveDecoded> {
  const limits = options.limits ?? DEFAULT_PORTABLE_ARCHIVE_LIMITS
  abortIfNeeded(options.signal)
  if (archive.size < VKADR_HEADER_BYTES) throw new PortableArchiveError('wrong_format', 'Это не архив Video Kadr (.vkadr).')
  const header = new Uint8Array(await archive.slice(0, VKADR_HEADER_BYTES).arrayBuffer())
  if (!bytesEqual(header.slice(0, 8), VKADR_MAGIC)) throw new PortableArchiveError('wrong_format', 'Это не архив Video Kadr (.vkadr).')
  const manifestLength = new DataView(header.buffer, header.byteOffset, header.byteLength).getUint32(8, false)
  if (manifestLength <= 0 || manifestLength > limits.maxManifestBytes) throw new PortableArchiveError('too_large', 'Некорректный размер manifest архива.')
  const payloadStart = VKADR_HEADER_BYTES + manifestLength
  if (payloadStart > archive.size) throw new PortableArchiveError('truncated', 'Архив обрезан до окончания manifest.')
  let rawManifest: unknown
  try {
    const bytes = new Uint8Array(await archive.slice(VKADR_HEADER_BYTES, payloadStart).arrayBuffer())
    rawManifest = JSON.parse(decoder.decode(bytes))
  } catch (error) {
    throw new PortableArchiveError('invalid_manifest', 'Manifest архива повреждён.', { cause: error })
  }
  const { manifest, project, payloadBytes } = validateManifest(rawManifest, limits)
  const expectedSize = payloadStart + payloadBytes
  if (!Number.isSafeInteger(expectedSize)) throw new PortableArchiveError('too_large', 'Архив превышает безопасный размер браузера.')
  if (archive.size < expectedSize) throw new PortableArchiveError('truncated', 'Архив обрезан: не хватает заявленных файлов.')
  if (archive.size > expectedSize) throw new PortableArchiveError('trailing_bytes', 'После заявленных файлов в архиве есть лишние данные.')

  const payloads = new Map<string, Blob>()
  let offset = payloadStart
  let completedBytes = 0
  for (const entry of manifest.entries) {
    abortIfNeeded(options.signal)
    const payload = archive.slice(offset, offset + entry.sizeBytes)
    const digest = await hashBlob(payload, options.signal, bytes => {
      completedBytes += bytes
      options.onProgress?.({ phase: 'verifying', completedBytes, totalBytes: payloadBytes, path: entry.path })
    })
    if (digest.toLowerCase() !== entry.sha256.toLowerCase()) throw new PortableArchiveError('checksum_mismatch', `Файл ${entry.path} повреждён.`)
    payloads.set(entry.path, payload)
    offset += entry.sizeBytes
  }
  const projectEntry = manifest.entries.find(entry => entry.kind === 'project')!
  try {
    const projectPayload = JSON.parse(decoder.decode(new Uint8Array(await payloads.get(projectEntry.path)!.arrayBuffer())))
    if (projectPayloadJson(projectPayload) !== projectPayloadJson(manifest.project)) {
      throw new PortableArchiveError('checksum_mismatch', 'project.json не совпадает с manifest.')
    }
  } catch (error) {
    if (error instanceof PortableArchiveError) throw error
    throw new PortableArchiveError('invalid_manifest', 'project.json повреждён.', { cause: error })
  }
  return { manifest, project, payloads }
}
