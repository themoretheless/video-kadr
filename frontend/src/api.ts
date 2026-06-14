import type { Job, MediaEntry, VideoInfo } from './types'

// The import/edit endpoints answer 200 + a jobId by design, so a 5xx (or a
// thrown fetch) from them means the request never reached the backend - in dev
// that is the Vite proxy failing to connect. Surface that plainly instead of a
// cryptic "HTTP 500".
const BACKEND_DOWN = 'Сервер недоступен. Запущен ли бэкенд? (cargo run на :8080)'

/** fetch that turns a network/proxy-level failure into a clear "backend down" error. */
async function safeFetch(path: string, init?: RequestInit): Promise<Response> {
  try {
    return await fetch(path, init)
  } catch {
    throw new Error(BACKEND_DOWN)
  }
}

async function postJson(path: string, body: unknown): Promise<{ jobId: string }> {
  const res = await safeFetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    if (res.status >= 500) throw new Error(BACKEND_DOWN)
    const text = await res.text().catch(() => '')
    throw new Error(text.trim() || `${path} -> HTTP ${res.status}`)
  }
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
  if (!res.ok) {
    const text = await res.text().catch(() => '')
    if (text.trim()) throw new Error(text.trim())
    throw new Error(res.status >= 500 ? BACKEND_DOWN : `upload -> HTTP ${res.status}`)
  }
  return res.json()
}

export async function getJob(jobId: string): Promise<Job> {
  const res = await safeFetch(`/api/jobs/${jobId}`)
  if (!res.ok) {
    if (res.status >= 500) throw new Error(BACKEND_DOWN)
    throw new Error(`job poll -> HTTP ${res.status}`)
  }
  return res.json()
}

/** List persisted sources and outputs, newest first. */
export async function getLibrary(): Promise<MediaEntry[]> {
  const res = await safeFetch('/api/library')
  if (!res.ok) throw new Error(`library -> HTTP ${res.status}`)
  return res.json()
}

/** Delete a library entry (and its file on disk). */
export async function deleteLibraryItem(id: string): Promise<void> {
  const res = await safeFetch(`/api/library/${id}`, { method: 'DELETE' })
  if (!res.ok && res.status !== 404) throw new Error(`delete -> HTTP ${res.status}`)
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
