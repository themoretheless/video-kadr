import type { Job } from './types'

async function postJson(path: string, body: unknown): Promise<{ jobId: string }> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) {
    throw new Error(`${path} -> HTTP ${res.status}`)
  }
  return res.json()
}

export function importUrl(body: Record<string, unknown>): Promise<{ jobId: string }> {
  return postJson('/api/import', body)
}

export function edit(payload: unknown): Promise<{ jobId: string }> {
  return postJson('/api/edit', payload)
}

export async function getJob(jobId: string): Promise<Job> {
  const res = await fetch(`/api/jobs/${jobId}`)
  if (!res.ok) throw new Error(`job poll -> HTTP ${res.status}`)
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
