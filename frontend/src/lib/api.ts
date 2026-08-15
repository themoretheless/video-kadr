import type { Composition, CompositionRenderRequest } from './composition/types'
import {
  parseProxyCreateResult,
  parseProxyList,
  parseProxyProfile,
  requireProxyKey,
  type ProxyCreateResult,
  type ProxyList,
  type ProxyProfile,
} from './proxy/types'
import type { Capabilities, EditState, Job, LutAsset, MediaEntry, MediaInfo, VideoInfo } from './types'

export type {
  ProxyArtifact,
  ProxyCodec,
  ProxyCreateResult,
  ProxyJobSummary,
  ProxyList,
  ProxyProfile,
} from './proxy/types'

const BACKEND_DOWN = 'Сервер недоступен. Запущен ли бэкенд? (cargo run на :8080)'

interface ApiErrorBody {
  error?: unknown
  code?: unknown
}

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message)
    this.name = 'ApiError'
  }
}

export class BackendUnavailableError extends Error {
  constructor() {
    super(BACKEND_DOWN)
    this.name = 'BackendUnavailableError'
  }
}

/** Fetch that distinguishes a network failure from a real HTTP error response. */
async function safeFetch(path: string, init?: RequestInit): Promise<Response> {
  try {
    return await fetch(path, init)
  } catch {
    throw new BackendUnavailableError()
  }
}

async function responseError(response: Response, fallback: string): Promise<ApiError> {
  const text = await response.text().catch(() => '')
  const trimmed = text.trim()
  let message = fallback
  let code: string | undefined

  if (trimmed) {
    try {
      const body = JSON.parse(trimmed) as ApiErrorBody
      if (typeof body.error === 'string' && body.error.trim()) message = body.error.trim()
      if (typeof body.code === 'string' && body.code.trim()) code = body.code.trim()
    } catch {
      message = trimmed
    }
  }

  return new ApiError(message, response.status, code)
}

async function requireOk(response: Response, fallback: string): Promise<void> {
  if (!response.ok) throw await responseError(response, fallback)
}

async function requestJson<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await safeFetch(path, init)
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

async function postJson(path: string, body: unknown): Promise<{ jobId: string }> {
  return requestJson(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
}

export function importUrl(body: Record<string, unknown>): Promise<{ jobId: string }> {
  return postJson('/api/import', body)
}

export function edit(payload: unknown): Promise<{ jobId: string }> {
  return postJson('/api/edit', payload)
}

/** Upload local video/audio/image media; the backend probes it before storage. */
export async function uploadFile(file: File): Promise<MediaInfo> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/upload', { method: 'POST', body: fd })
  await requireOk(res, `upload -> HTTP ${res.status}`)
  return res.json()
}

/** Queue a schema-v1 multi-source composition render. */
export function renderComposition(request: CompositionRenderRequest): Promise<{ jobId: string }> {
  return postJson('/api/compositions/render', request)
}

/** Upload and validate a 3D `.cube` LUT. */
export async function uploadLut(file: File, signal?: AbortSignal): Promise<LutAsset> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/luts', { method: 'POST', body: fd, signal })
  await requireOk(res, `LUT upload -> HTTP ${res.status}`)
  return res.json()
}

/** Resolve metadata for a previously stored immutable LUT asset. */
export async function getLut(id: string): Promise<LutAsset> {
  const res = await safeFetch(`/api/luts/${encodeURIComponent(id)}`)
  await requireOk(res, `LUT lookup -> HTTP ${res.status}`)
  return res.json()
}

export async function getJob(jobId: string): Promise<Job> {
  const res = await safeFetch(`/api/jobs/${jobId}`)
  await requireOk(res, `job poll -> HTTP ${res.status}`)
  return res.json()
}

export async function getCapabilities(): Promise<Capabilities> {
  const res = await safeFetch('/api/capabilities')
  await requireOk(res, `capabilities -> HTTP ${res.status}`)
  return res.json()
}

/** List persisted sources and outputs, newest first. */
export async function getLibrary(): Promise<MediaEntry[]> {
  const res = await safeFetch('/api/library')
  await requireOk(res, `library -> HTTP ${res.status}`)
  return res.json()
}

/** Same-origin stable URL for a lazily loaded library thumbnail. */
export function libraryThumbnailUrl(id: string): string {
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(id)) {
    throw new TypeError('Invalid library media id')
  }
  return '/api/library/' + encodeURIComponent(id) + '/thumbnail'
}

/** Same-origin stable URL for the bounded eight-cell video filmstrip. */
export function libraryFilmstripUrl(id: string): string {
  if (!/^[A-Za-z0-9._-]{1,128}$/.test(id)) {
    throw new TypeError('Invalid library media id')
  }
  return '/api/library/' + encodeURIComponent(id) + '/filmstrip'
}

/** Enqueue a content-addressed proxy for one original source video. */
export async function createLibraryProxy(
  sourceId: string,
  profile: ProxyProfile,
): Promise<ProxyCreateResult> {
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies`
  const raw = await requestJson<unknown>(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(parseProxyProfile(profile)),
  })
  return parseProxyCreateResult(raw)
}

/** Return only verified proxies for the source's current fingerprint. */
export async function getLibraryProxies(sourceId: string): Promise<ProxyList> {
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies`
  return parseProxyList(await requestJson<unknown>(path), sourceId)
}

/** Cancel matching work and remove the source-owned proxy artifact. */
export async function deleteLibraryProxy(sourceId: string, key: string): Promise<void> {
  const safeKey = requireProxyKey(key)
  const path = `/api/library/${encodeURIComponent(sourceId)}/proxies/${safeKey}`
  const res = await safeFetch(path, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `proxy delete -> HTTP ${res.status}`)
  }
}

export interface LibraryMetadataPatch {
  title?: string | null
  favorite?: boolean
  tags?: string[]
}

export interface LibraryMetadataPut {
  title: string | null
  favorite: boolean
  tags: string[]
}

/** Update only the supplied local metadata fields for one library item. */
export function patchLibraryMetadata(
  id: string,
  metadata: LibraryMetadataPatch,
): Promise<MediaEntry> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/metadata`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(metadata),
  })
}

/** Replace all local metadata fields for one library item. */
export function putLibraryMetadata(
  id: string,
  metadata: LibraryMetadataPut,
): Promise<MediaEntry> {
  return requestJson(`/api/library/${encodeURIComponent(id)}/metadata`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(metadata),
  })
}

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(id: string): Promise<void> {
  const res = await safeFetch(`/api/library/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `delete -> HTTP ${res.status}`)
  }
}

/** A saved editing project: a clip plus its persisted edit recipe. */
export interface ProjectDto {
  id: string
  name: string
  videoId: string
  video: VideoInfo
  edit: Partial<EditState>
  createdAt: number
  updatedAt: number
}

/** Create or update (keyed by videoId) the saved project for a clip. */
export async function saveProject(body: Record<string, unknown>): Promise<ProjectDto> {
  const res = await safeFetch('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** Fetch the saved project for a clip, or null if none exists yet. */
export async function getProjectByVideo(videoId: string): Promise<ProjectDto | null> {
  const res = await safeFetch(`/api/projects/by-video/${encodeURIComponent(videoId)}`)
  if (res.status === 404) return null
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** List saved projects, most recently updated first. */
export async function getProjects(): Promise<ProjectDto[]> {
  const res = await safeFetch('/api/projects')
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

export async function deleteProject(id: string): Promise<void> {
  const res = await safeFetch(`/api/projects/${id}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `projects -> HTTP ${res.status}`)
  }
}

export interface CompositionProjectDto {
  id: string
  name: string
  schemaVersion: 2
  mode: 'composition'
  document: Composition
  sourceIds: string[]
  createdAt: number
  updatedAt: number
}

/** The API helper supplies the fixed project envelope fields. */
export interface CompositionProjectSaveRequest {
  name?: string
  document: Composition
}

export interface CompositionProjectArchiveImportResponse {
  project: CompositionProjectDto
  sourceMapping: Record<string, string>
}

/** Mirrors the backend's complete archive limit and fails before allocating FormData. */
export const MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES = 2 * 1024 * 1024 * 1024 + 4 * 1024 * 1024

function compositionProjectBody(body: CompositionProjectSaveRequest): Record<string, unknown> {
  return {
    schemaVersion: 2,
    mode: 'composition',
    ...(body.name === undefined ? {} : { name: body.name }),
    document: body.document,
  }
}

export function createCompositionProject(
  body: CompositionProjectSaveRequest,
): Promise<CompositionProjectDto> {
  return requestJson('/api/composition-projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(compositionProjectBody(body)),
  })
}

export function getCompositionProjects(): Promise<CompositionProjectDto[]> {
  return requestJson('/api/composition-projects')
}

export async function getCompositionProject(id: string): Promise<CompositionProjectDto | null> {
  const path = `/api/composition-projects/${encodeURIComponent(id)}`
  const res = await safeFetch(path)
  if (res.status === 404) return null
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

export function updateCompositionProject(
  id: string,
  body: CompositionProjectSaveRequest,
): Promise<CompositionProjectDto> {
  const path = `/api/composition-projects/${encodeURIComponent(id)}`
  return requestJson(path, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(compositionProjectBody(body)),
  })
}

export async function deleteCompositionProject(id: string): Promise<void> {
  const res = await safeFetch(`/api/composition-projects/${encodeURIComponent(id)}`, {
    method: 'DELETE',
  })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `composition project delete -> HTTP ${res.status}`)
  }
}

/**
 * Direct navigation keeps multi-gigabyte archives streaming to disk instead
 * of materializing them as an in-memory browser Blob.
 */
export function compositionProjectArchiveUrl(id: string): string {
  return `/api/composition-projects/${encodeURIComponent(id)}/archive`
}

/** Upload, verify and relink a portable `.veproj` into a new local project. */
export async function importCompositionProjectArchive(
  file: File,
): Promise<CompositionProjectArchiveImportResponse> {
  if (file.size > MAX_COMPOSITION_PROJECT_ARCHIVE_BYTES) {
    throw new Error('Архив проекта превышает лимит 2 ГиБ')
  }
  const body = new FormData()
  body.append('file', file)
  const path = '/api/composition-projects/import'
  const res = await safeFetch(path, { method: 'POST', body })
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

/** Ask the backend to cancel a running/pending job. Best-effort. */
export async function cancelJob(jobId: string): Promise<void> {
  try {
    await fetch(`/api/jobs/${jobId}/cancel`, { method: 'POST' })
  } catch {
    // The poll loop will surface the resulting state.
  }
}

/**
 * Poll a job until it finishes. Resolves with the completed job (status `done`),
 * rejects with the job's error, or rejects with Error('cancelled') so callers
 * can tell a user cancellation apart from a real failure.
 */
export function pollJob(jobId: string, onTick?: (job: Job) => void): Promise<Job> {
  return new Promise((resolve, reject) => {
    const tick = async () => {
      try {
        const job = await getJob(jobId)
        onTick?.(job)
        if (job.status === 'done') return resolve(job)
        if (job.status === 'cancelled') return reject(new Error('cancelled'))
        if (job.status === 'interrupted') {
          return reject(new Error('Задача прервана (сервер перезапущен)'))
        }
        if (job.status === 'error') {
          return reject(new Error(job.error || 'задача завершилась с ошибкой'))
        }
        setTimeout(tick, 500)
      } catch (e) {
        reject(e)
      }
    }
    void tick()
  })
}
