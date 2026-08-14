import type { Capabilities, EditState, Job, LutAsset, MediaEntry, VideoInfo } from './types'

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
  return postJson('/api/import', body)
}

export function edit(payload: unknown): Promise<{ jobId: string }> {
  return postJson('/api/edit', payload)
}

/** Upload a local video file; the backend probes it and returns VideoInfo. */
export async function uploadFile(file: File): Promise<VideoInfo> {
  const fd = new FormData()
  fd.append('file', file)
  const res = await safeFetch('/api/upload', { method: 'POST', body: fd })
  await requireOk(res, `upload -> HTTP ${res.status}`)
  return res.json()
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

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(id: string): Promise<void> {
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
