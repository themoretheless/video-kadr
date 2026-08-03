import type { ProjectDto } from './api'
import {
  auditBrowserAssets,
  deleteBrowserAsset,
  prepareBrowserStorage,
  putBrowserAsset,
} from './browser-asset-store'
import { readSourceForArchive } from './browser-media'
import { putProject } from './browser-project-store'
import { OPFS_INGEST_OVERHEAD_BYTES } from './browser-resource-plan'
import {
  buildPortableArchive,
  PortableArchiveError,
  readPortableArchive,
  type PortableArchiveCodecProgress,
  type PortableArchivePayloadInput,
} from './project-archive-codec'
import {
  createProjectDocumentFromLegacy,
  legacyProjectValues,
  migrateProjectDocument,
  type JsonObject,
  type ProjectDocument,
  type ProjectMedia,
} from './project-schema'
import type { EditState, VideoInfo } from './types'

const ARCHIVE_ORIGIN_FIELD = 'portableArchiveOrigin'

interface ArchiveOrigin {
  schemaVersion: 1
  revision: number
  createdAt: number
  updatedAt: number
}

export interface BrowserProjectArchiveOptions {
  includeOriginalMedia: boolean
  includeProxies: boolean
}

export interface BrowserProjectArchiveProgress {
  phase: 'preparing' | 'hashing' | 'verifying' | 'installing' | 'done'
  completedBytes: number
  totalBytes: number
  message: string
}

export interface BrowserProjectArchiveExportResult {
  blob: Blob
  filename: string
  warnings: string[]
  rootHash: string
}

export interface BrowserProjectArchiveImportResult {
  project: ProjectDto
  importedMedia: number
  reusedMedia: number
  missingMedia: string[]
  warnings: string[]
}

function archiveError(code: 'missing_media' | 'unsupported_lut' | 'invalid_project', message: string): PortableArchiveError {
  return new PortableArchiveError('invalid_manifest', message, { cause: { code } })
}

function abortIfNeeded(signal?: AbortSignal): void {
  if (signal?.aborted) throw new PortableArchiveError('cancelled', 'Операция с архивом отменена.')
}

function extension(filename: string): string {
  const value = filename.split('.').pop()?.toLowerCase() ?? ''
  return /^[a-z0-9]{1,12}$/.test(value) ? value : 'bin'
}

function mimeType(filename: string, media: ProjectMedia): string {
  const stored = typeof media.metadata.fileType === 'string' ? media.metadata.fileType : ''
  if (stored) return stored
  const ext = extension(filename)
  const known: Record<string, string> = {
    mp4: 'video/mp4', mov: 'video/quicktime', webm: 'video/webm', mkv: 'video/x-matroska',
    mp3: 'audio/mpeg', wav: 'audio/wav', m4a: 'audio/mp4', aac: 'audio/aac', flac: 'audio/flac',
    png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp',
  }
  return known[ext] ?? (media.kind === 'audio' ? 'audio/*' : media.kind === 'image' ? 'image/*' : 'video/*')
}

function mediaName(media: ProjectMedia): string {
  const value = media.metadata.filename
  return typeof value === 'string' && value.trim() ? value : `${media.id}.${media.kind === 'audio' ? 'wav' : 'mp4'}`
}

function portableDocument(project: ProjectDto): ProjectDocument {
  const document = project.document
    ? migrateProjectDocument(project.document)
    : createProjectDocumentFromLegacy(
        project.videoId,
        project.name,
        project.video as unknown as JsonObject,
        project.edit as unknown as JsonObject,
      )
  const value = structuredClone(document) as ProjectDocument & Record<string, unknown>
  value[ARCHIVE_ORIGIN_FIELD] = {
    schemaVersion: 1,
    revision: project.revision ?? 1,
    createdAt: project.createdAt,
    updatedAt: project.updatedAt,
  } satisfies ArchiveOrigin
  return value
}

function referencedLutIds(document: ProjectDocument): string[] {
  const result = new Set<string>()
  for (const effect of document.sequences.flatMap(sequence => sequence.tracks).flatMap(track => track.clips).flatMap(clip => clip.effects)) {
    const parameters = effect.parameters
    const lutId = parameters.lutId
    if (typeof lutId === 'string' && lutId.trim()) result.add(lutId)
    const lut = parameters.lut
    if (lut && typeof lut === 'object' && !Array.isArray(lut) && typeof (lut as JsonObject).id === 'string') {
      result.add((lut as JsonObject).id as string)
    }
  }
  return [...result]
}

function codecProgress(
  progress: PortableArchiveCodecProgress,
  callback?: (progress: BrowserProjectArchiveProgress) => void,
): void {
  callback?.({
    phase: progress.phase,
    completedBytes: progress.completedBytes,
    totalBytes: progress.totalBytes,
    message: progress.phase === 'hashing' ? `Проверяю ${progress.path}` : `Проверяю архив: ${progress.path}`,
  })
}

export async function exportBrowserProjectArchive(
  project: ProjectDto,
  options: BrowserProjectArchiveOptions,
  control: { signal?: AbortSignal; onProgress?: (progress: BrowserProjectArchiveProgress) => void } = {},
): Promise<BrowserProjectArchiveExportResult> {
  abortIfNeeded(control.signal)
  const document = portableDocument(project)
  const luts = referencedLutIds(document)
  if (luts.length) {
    throw archiveError(
      'unsupported_lut',
      'Проект использует LUT. Browser-хранилище LUT пока не переносится; архив не создан, чтобы не потерять цветокоррекцию.',
    )
  }
  control.onProgress?.({ phase: 'preparing', completedBytes: 0, totalBytes: 0, message: 'Собираю состав проекта…' })
  const payloads: PortableArchivePayloadInput[] = []
  const warnings: string[] = []
  const included = new Set<string>()
  if (options.includeOriginalMedia) {
    for (const media of document.media) {
      abortIfNeeded(control.signal)
      const fingerprint = media.contentFingerprint
      if (!fingerprint) throw archiveError('missing_media', `У исходника «${mediaName(media)}» нет fingerprint; полный архив создать нельзя.`)
      if (included.has(fingerprint)) continue
      const assetRef = media.assetRef ?? media.id
      let source
      try { source = await readSourceForArchive(assetRef, fingerprint) }
      catch {
        throw archiveError('missing_media', `Исходник «${mediaName(media)}» недоступен. Разрешите доступ, выполните relink или выключите «Включить оригиналы».`)
      }
      if (source.file.size <= 0) throw archiveError('missing_media', `Исходник «${mediaName(media)}» пуст.`)
      payloads.push({
        path: `media/${fingerprint}.${extension(source.filename)}`,
        kind: 'media',
        blob: source.file,
      })
      included.add(fingerprint)
    }
  } else if (document.media.length) {
    warnings.push('Оригиналы не включены: после импорта потребуется точный relink по SHA-256.')
  }
  if (options.includeProxies) {
    warnings.push('Прокси не включены: текущий browser proxy descriptor несовместим с переносимым provenance и будет пересоздан.')
  }
  const built = await buildPortableArchive(document, payloads, {
    signal: control.signal,
    onProgress: progress => codecProgress(progress, control.onProgress),
  })
  control.onProgress?.({ phase: 'done', completedBytes: built.blob.size, totalBytes: built.blob.size, message: 'Архив готов' })
  const base = project.name.trim().replace(/[^\p{L}\p{N}._-]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 80) || 'project'
  return { blob: built.blob, filename: `${base}.vkadr`, warnings, rootHash: built.manifest.rootHash }
}

function archiveOrigin(project: Record<string, unknown>): ArchiveOrigin | null {
  const value = project[ARCHIVE_ORIGIN_FIELD]
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null
  const candidate = value as Partial<ArchiveOrigin>
  if (
    candidate.schemaVersion !== 1
    || !Number.isSafeInteger(candidate.revision)
    || Number(candidate.revision) < 1
    || !Number.isSafeInteger(candidate.createdAt)
    || !Number.isSafeInteger(candidate.updatedAt)
    || Number(candidate.createdAt) < 0
    || Number(candidate.updatedAt) < 0
  ) return null
  return candidate as ArchiveOrigin
}

function number(metadata: JsonObject, key: string, fallback = 0): number {
  const value = metadata[key]
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function string(metadata: JsonObject, key: string): string | undefined {
  const value = metadata[key]
  return typeof value === 'string' && value.trim() ? value : undefined
}

function projectVideo(document: ProjectDocument, available: Set<string>): VideoInfo {
  const media = document.media.find(item => item.id === document.primaryMediaId)!
  const assetId = media.assetRef ?? media.id
  const metadata = media.metadata
  return {
    id: assetId,
    assetId,
    url: '',
    filename: mediaName(media),
    duration: number(metadata, 'duration'),
    width: number(metadata, 'width'),
    height: number(metadata, 'height'),
    title: string(metadata, 'title'),
    fps: number(metadata, 'fps') || undefined,
    vcodec: string(metadata, 'vcodec'),
    acodec: string(metadata, 'acodec'),
    mediaKind: media.kind === 'audio' ? 'audio' : 'video',
    fingerprint: media.contentFingerprint,
    sizeBytes: number(metadata, 'sizeBytes') || undefined,
    availability: available.has(assetId) ? 'ready' : 'offline',
  }
}

export async function importBrowserProjectArchive(
  archive: Blob,
  control: { signal?: AbortSignal; onProgress?: (progress: BrowserProjectArchiveProgress) => void } = {},
): Promise<BrowserProjectArchiveImportResult> {
  abortIfNeeded(control.signal)
  const decoded = await readPortableArchive(archive, {
    signal: control.signal,
    onProgress: progress => codecProgress(progress, control.onProgress),
  })
  if (decoded.manifest.entries.some(entry => entry.kind === 'lut') || referencedLutIds(decoded.project).length) {
    throw archiveError('unsupported_lut', 'Архив содержит LUT, который эта browser-версия пока не может безопасно сопоставить и сохранить.')
  }
  const rawProject = structuredClone(decoded.manifest.project)
  const origin = archiveOrigin(rawProject)
  delete rawProject[ARCHIVE_ORIGIN_FIELD]
  const document = migrateProjectDocument(rawProject)
  const existingAssets = await auditBrowserAssets()
  const readyByFingerprint = new Map(existingAssets.filter(asset => asset.availability === 'ready').map(asset => [asset.fingerprint, asset]))
  const anyByFingerprint = new Map(existingAssets.map(asset => [asset.fingerprint, asset]))
  const chosenByFingerprint = new Map<string, string>()
  const installs: Array<{ id: string; media: ProjectMedia; blob: Blob; filename: string }> = []
  const missingMedia: string[] = []
  let reusedMedia = 0

  for (const media of document.media) {
    abortIfNeeded(control.signal)
    const fingerprint = media.contentFingerprint
    if (!fingerprint) {
      media.assetRef = crypto.randomUUID()
      missingMedia.push(mediaName(media))
      continue
    }
    const alreadyChosen = chosenByFingerprint.get(fingerprint)
    if (alreadyChosen) { media.assetRef = alreadyChosen; continue }
    const payloadEntry = decoded.manifest.entries.find(entry => entry.kind === 'media' && entry.sha256.toLowerCase() === fingerprint)
    const payload = payloadEntry ? decoded.payloads.get(payloadEntry.path) : undefined
    const exact = payload ? readyByFingerprint.get(fingerprint) : anyByFingerprint.get(fingerprint)
    if (exact) {
      media.assetRef = exact.id
      chosenByFingerprint.set(fingerprint, exact.id)
      reusedMedia++
      continue
    }
    const id = crypto.randomUUID()
    media.assetRef = id
    chosenByFingerprint.set(fingerprint, id)
    if (payload) installs.push({ id, media, blob: payload, filename: mediaName(media) })
    else missingMedia.push(mediaName(media))
  }

  const installBytes = installs.reduce((sum, item) => sum + item.blob.size, 0)
  if (installBytes > 0) await prepareBrowserStorage(installBytes * 2 + OPFS_INGEST_OVERHEAD_BYTES)
  const created: string[] = []
  const available = new Set<string>([...chosenByFingerprint.values()].filter(id => existingAssets.some(asset => asset.id === id && asset.availability === 'ready')))
  let installedBytes = 0
  try {
    for (const item of installs) {
      abortIfNeeded(control.signal)
      const fingerprint = item.media.contentFingerprint!
      const file = new File([item.blob], item.filename, { type: mimeType(item.filename, item.media) })
      const metadata = item.media.metadata
      await putBrowserAsset({
        id: item.id,
        filename: item.filename,
        fileType: file.type,
        file,
        fingerprint,
        byteLength: file.size,
        createdAt: Date.now(),
        info: {
          id: item.id,
          assetId: item.id,
          filename: item.filename,
          duration: number(metadata, 'duration'),
          width: number(metadata, 'width'),
          height: number(metadata, 'height'),
          title: string(metadata, 'title'),
          fps: number(metadata, 'fps') || undefined,
          vcodec: string(metadata, 'vcodec'),
          acodec: string(metadata, 'acodec'),
          mediaKind: item.media.kind === 'audio' ? 'audio' : 'video',
          fingerprint,
          sizeBytes: file.size,
          availability: 'ready',
        },
      })
      created.push(item.id)
      available.add(item.id)
      installedBytes += item.blob.size
      control.onProgress?.({ phase: 'installing', completedBytes: installedBytes, totalBytes: installBytes, message: `Сохраняю ${item.filename}` })
    }
    abortIfNeeded(control.signal)
    // The source ID is not authority across installations. Every import is a
    // new local project while safe timestamps remain provenance metadata.
    const projectId = crypto.randomUUID()
    const now = Date.now()
    const video = projectVideo(document, available)
    const legacy = legacyProjectValues(document)
    const imported: ProjectDto = {
      id: projectId,
      name: document.name,
      videoId: video.assetId ?? video.id,
      video,
      edit: legacy.edit as Partial<EditState>,
      document,
      revision: 1,
      createdAt: origin?.createdAt ?? now,
      updatedAt: now,
    }
    await putProject(imported)
    const warnings = decoded.manifest.entries.some(entry => entry.kind === 'proxy')
      ? ['Прокси проверены, но не установлены; они будут пересозданы локально.']
      : []
    control.onProgress?.({ phase: 'done', completedBytes: archive.size, totalBytes: archive.size, message: 'Проект импортирован' })
    return { project: imported, importedMedia: installs.length, reusedMedia, missingMedia, warnings }
  } catch (error) {
    for (const id of created.reverse()) await deleteBrowserAsset(id).catch(() => undefined)
    throw error
  }
}
