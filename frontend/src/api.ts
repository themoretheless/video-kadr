import type { Capabilities, EditState, Job, LutAsset, MediaEntry, ProjectDocument, ProjectEnvelope, VideoInfo } from './types'
import * as browserMedia from './browser-media'
import { decodeProjectEnvelope } from './project-schema'

export const clientOnlyMode = browserMedia.isBrowserProcessing()

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

async function postJson(path: string, body: unknown): Promise<{ jobId: string }> {
  const res = await safeFetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  await requireOk(res, `${path} -> HTTP ${res.status}`)
  return res.json()
}

export function importUrl(body: Record<string, unknown>): Promise<{ jobId: string }> {
  if (clientOnlyMode) return Promise.reject(new browserMedia.LinkImportRequiresServerError())
  return postJson('/api/import', body)
}

export function edit(payload: unknown): Promise<{ jobId: string }> {
  if (clientOnlyMode) return Promise.resolve(browserMedia.edit(payload as Record<string, unknown>))
  return postJson('/api/edit', payload)
}

/** Upload a local video file; the backend probes it and returns VideoInfo. */
export async function uploadFile(file: File): Promise<VideoInfo> {
  if (clientOnlyMode) return browserMedia.uploadFile(file)
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/upload', { method: 'POST', body: fd })
  await requireOk(res, `upload -> HTTP ${res.status}`)
  return res.json()
}

/** Upload and validate a 3D `.cube` LUT. */
export async function uploadLut(file: File, signal?: AbortSignal): Promise<LutAsset> {
  if (clientOnlyMode) return browserMedia.uploadLut(file)
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/luts', { method: 'POST', body: fd, signal })
  await requireOk(res, `LUT upload -> HTTP ${res.status}`)
  return res.json()
}

/** Resolve metadata for a previously stored immutable LUT asset. */
export async function getLut(id: string): Promise<LutAsset> {
  if (clientOnlyMode) return browserMedia.getLut(id)
  const res = await safeFetch(`/api/luts/${encodeURIComponent(id)}`)
  await requireOk(res, `LUT lookup -> HTTP ${res.status}`)
  return res.json()
}

export async function getJob(jobId: string): Promise<Job> {
  if (clientOnlyMode) return browserMedia.getJob(jobId)
  const res = await safeFetch(`/api/jobs/${jobId}`)
  await requireOk(res, `job poll -> HTTP ${res.status}`)
  return res.json()
}

export async function getCapabilities(): Promise<Capabilities> {
  if (clientOnlyMode) return browserMedia.getCapabilities()
  const res = await safeFetch('/api/capabilities')
  await requireOk(res, `capabilities -> HTTP ${res.status}`)
  return res.json()
}

/** List persisted sources and outputs, newest first. */
export async function getLibrary(): Promise<MediaEntry[]> {
  if (clientOnlyMode) return browserMedia.getLibrary()
  const res = await safeFetch('/api/library')
  await requireOk(res, `library -> HTTP ${res.status}`)
  return res.json()
}

export async function resolveLibrarySource(entry: MediaEntry): Promise<VideoInfo> {
  if (clientOnlyMode) return browserMedia.resolveSource(entry.id)
  return {
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    duration: entry.duration ?? 0,
    width: entry.width ?? 0,
    height: entry.height ?? 0,
    title: entry.title,
    fps: entry.fps,
    vcodec: entry.vcodec,
    acodec: entry.acodec,
    mediaKind: entry.mediaKind,
    assetId: entry.assetId,
    fingerprint: entry.fingerprint,
    sizeBytes: entry.sizeBytes,
  }
}

export async function relinkLibrarySource(
  id: string,
  file: File,
  handle?: FileSystemFileHandle,
): Promise<VideoInfo> {
  if (!clientOnlyMode) throw new Error('Relink через браузер доступен только в статической версии')
  return browserMedia.relinkSource(id, file, handle)
}

export async function restoreExternalLibrarySource(id: string): Promise<VideoInfo> {
  if (!clientOnlyMode) throw new Error('External browser handles are available only in local mode')
  return browserMedia.restoreExternalSource(id)
}

export function getBrowserStorageStatus() {
  return clientOnlyMode ? browserMedia.getStorageStatus() : null
}

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(id: string): Promise<void> {
  if (clientOnlyMode) {
    await browserMedia.deleteLibraryItem(id)
    return
  }
  const res = await safeFetch(`/api/library/${id}`, { method: 'DELETE' })
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
  document?: ProjectDocument
  /** Monotonic autosave revision; absent only on legacy responses. */
  revision?: number
  createdAt: number
  updatedAt: number
}

/** Create or update (keyed by videoId) the saved project for a clip. */
export async function saveProject(body: Record<string, unknown>): Promise<ProjectDto> {
  if (clientOnlyMode) return browserMedia.saveProject(body)
  const res = await safeFetch('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** CAS-save the canonical multitrack document. `expectedRevision: 0` creates it. */
export async function saveProjectDocument(
  projectId: string,
  expectedRevision: number,
  document: ProjectDocument,
): Promise<ProjectEnvelope> {
  if (clientOnlyMode) {
    let saved: ProjectDto
    try {
      saved = await browserMedia.saveProject({
        projectId,
        expectedRevision,
        videoId: document.primaryMediaId,
        name: document.name,
        video: document.media.find((media) => media.id === document.primaryMediaId)?.metadata ?? {},
        edit: {},
        document,
      })
    } catch (error) {
      if (error instanceof browserMedia.ProjectRevisionConflictError) {
        throw new ApiError(error.message, 409, 'project_revision_conflict')
      }
      throw error
    }
    return decodeProjectEnvelope({
      schemaVersion: 1,
      projectId: saved.id,
      revision: saved.revision ?? 1,
      createdAt: saved.createdAt,
      updatedAt: saved.updatedAt,
      document: saved.document ?? document,
    })
  }
  const response = await safeFetch('/api/projects/documents', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ projectId, expectedRevision, document }),
  })
  await requireOk(response, `project document -> HTTP ${response.status}`)
  return decodeProjectEnvelope(await response.json())
}

export async function getProjectDocument(projectId: string): Promise<ProjectEnvelope | null> {
  if (clientOnlyMode) {
    const projects = await browserMedia.getProjects()
    const saved = projects.find((project) => project.id === projectId)
    if (!saved?.document) return null
    return decodeProjectEnvelope({
      schemaVersion: 1,
      projectId: saved.id,
      revision: saved.revision ?? 1,
      createdAt: saved.createdAt,
      updatedAt: saved.updatedAt,
      document: saved.document,
    })
  }
  const response = await safeFetch(`/api/projects/documents/${encodeURIComponent(projectId)}`)
  if (response.status === 404) return null
  await requireOk(response, `project document -> HTTP ${response.status}`)
  return decodeProjectEnvelope(await response.json())
}

/** Fetch the saved project for a clip, or null if none exists yet. */
export async function getProjectByVideo(videoId: string): Promise<ProjectDto | null> {
  if (clientOnlyMode) return browserMedia.getProjectByVideo(videoId)
  const res = await safeFetch(`/api/projects/by-video/${encodeURIComponent(videoId)}`)
  if (res.status === 404) return null
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

/** List saved projects, most recently updated first. */
export async function getProjects(): Promise<ProjectDto[]> {
  if (clientOnlyMode) return browserMedia.getProjects()
  const res = await safeFetch('/api/projects')
  await requireOk(res, `projects -> HTTP ${res.status}`)
  return res.json()
}

export async function deleteProject(id: string): Promise<void> {
  if (clientOnlyMode) {
    await browserMedia.deleteProject(id)
    return
  }
  const res = await safeFetch(`/api/projects/${id}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `projects -> HTTP ${res.status}`)
  }
}

/** Ask the backend to cancel a running/pending job. Best-effort. */
export async function cancelJob(jobId: string): Promise<void> {
  if (clientOnlyMode) {
    browserMedia.cancelJob(jobId)
    return
  }
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
