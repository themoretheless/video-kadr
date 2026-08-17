import type {
  AssetEntry,
  AssetKind,
  Capabilities,
  EditState,
  Job,
  LutAsset,
  MediaEntry,
  VideoInfo,
} from './types'
import * as browserMedia from './browser-media'

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

/** Build an ApiError from a raw error body (JSON `{error, code}` or plain text). */
function apiErrorFromText(text: string, status: number, fallback: string): ApiError {
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

  return new ApiError(message, status, code)
}

async function responseError(response: Response, fallback: string): Promise<ApiError> {
  const text = await response.text().catch(() => '')
  return apiErrorFromText(text, response.status, fallback)
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

// --- media assets (images, audio, video, fonts, subtitles) ---
// Assets are referenced by id everywhere; the backend resolves an id to a
// private path right before the render runs, exactly like a stored LUT.

/** Thrown in the browser-only build, which has no asset storage at all. */
export class AssetsRequireServerError extends Error {
  constructor() {
    super('Медиа-ассеты доступны только при запущенном сервере')
    this.name = 'AssetsRequireServerError'
  }
}

export interface UploadOptions {
  signal?: AbortSignal
  /** Called with 0..100 while the request body is being sent. */
  onProgress?: (percent: number) => void
}

/**
 * Multipart upload over XHR: `fetch` cannot report upload progress, and asset
 * bodies go up to 64 MiB. Errors are normalized to the same ApiError /
 * BackendUnavailableError pair the fetch helpers throw.
 */
function uploadMultipart(path: string, form: FormData, options?: UploadOptions): Promise<unknown> {
  return new Promise((resolve, reject) => {
    if (options?.signal?.aborted) {
      reject(new Error('cancelled'))
      return
    }
    const xhr = new XMLHttpRequest()
    const onAbort = () => xhr.abort()
    const cleanup = () => options?.signal?.removeEventListener('abort', onAbort)

    xhr.open('POST', path)
    if (options?.onProgress) {
      xhr.upload.addEventListener('progress', (event) => {
        if (!event.lengthComputable || event.total <= 0) return
        options.onProgress?.(Math.max(0, Math.min(100, Math.round((event.loaded / event.total) * 100))))
      })
    }
    xhr.addEventListener('load', () => {
      cleanup()
      const text = typeof xhr.responseText === 'string' ? xhr.responseText : ''
      if (xhr.status < 200 || xhr.status >= 300) {
        reject(apiErrorFromText(text, xhr.status, `${path} -> HTTP ${xhr.status}`))
        return
      }
      try {
        resolve(text.trim() ? JSON.parse(text) : null)
      } catch {
        reject(new ApiError(`${path} -> некорректный ответ сервера`, xhr.status))
      }
    })
    xhr.addEventListener('error', () => {
      cleanup()
      reject(new BackendUnavailableError())
    })
    xhr.addEventListener('timeout', () => {
      cleanup()
      reject(new BackendUnavailableError())
    })
    xhr.addEventListener('abort', () => {
      cleanup()
      reject(new Error('cancelled'))
    })
    options?.signal?.addEventListener('abort', onAbort)
    xhr.send(form)
  })
}

/** Upload an asset. The backend sniffs the content; `kind` is only a hint. */
export async function uploadAsset(
  file: File,
  kind: AssetKind,
  options?: UploadOptions,
): Promise<AssetEntry> {
  if (clientOnlyMode) throw new AssetsRequireServerError()
  const form = new FormData()
  form.append('file', file)
  form.append('kind', kind)
  return (await uploadMultipart('/api/assets', form, options)) as AssetEntry
}

/** List every stored asset. */
export async function getAssets(): Promise<AssetEntry[]> {
  if (clientOnlyMode) throw new AssetsRequireServerError()
  const res = await safeFetch('/api/assets')
  await requireOk(res, `assets -> HTTP ${res.status}`)
  const body: unknown = await res.json()
  if (typeof body !== 'object' || body === null) return []
  const assets = (body as { assets?: unknown }).assets
  return Array.isArray(assets) ? (assets as AssetEntry[]) : []
}

/** Resolve metadata for a single stored asset. */
export async function getAsset(id: string): Promise<AssetEntry> {
  if (clientOnlyMode) throw new AssetsRequireServerError()
  const res = await safeFetch(`/api/assets/${encodeURIComponent(id)}`)
  await requireOk(res, `asset lookup -> HTTP ${res.status}`)
  return res.json()
}

/** Delete a stored asset. A missing asset is treated as already deleted. */
export async function deleteAsset(id: string): Promise<void> {
  if (clientOnlyMode) throw new AssetsRequireServerError()
  const res = await safeFetch(`/api/assets/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `asset delete -> HTTP ${res.status}`)
  }
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

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(id: string): Promise<void> {
  if (clientOnlyMode) {
    browserMedia.deleteLibraryItem(id)
    return
  }
  const res = await safeFetch(`/api/library/${id}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) {
    throw await responseError(res, `delete -> HTTP ${res.status}`)
  }
}

/**
 * The persisted edit recipe. Feature module state rides inside `edit.modules`
 * because the projects endpoint denies unknown top-level fields; it is absent
 * for projects that use none of the new features.
 */
export type PersistedEdit = Partial<EditState> & { modules?: unknown }

/** A saved editing project: a clip plus its persisted edit recipe. */
export interface ProjectDto {
  id: string
  name: string
  videoId: string
  video: VideoInfo
  edit: PersistedEdit
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
    browserMedia.deleteProject(id)
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
