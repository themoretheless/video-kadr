import { getLocalTtsJob } from '$lib/api'
import type { Job } from '$lib/types'
/** Aborting observation never cancels durable work. Cancellation is an explicit API action. */
export async function observeTtsJob(token: string, id: string, signal: AbortSignal, update: (job: Job) => void): Promise<Job> {
  const deadline = Date.now() + 30 * 60 * 1000
  while (true) {
    signal.throwIfAborted()
    const job = await getLocalTtsJob(token, id, signal)
    signal.throwIfAborted()
    if (!job || !['pending', 'running', 'done', 'error', 'cancelled', 'interrupted'].includes(job.status)) throw new Error('Не удалось прочитать состояние озвучивания')
    update(job)
    if (job.status !== 'pending' && job.status !== 'running') return job
    if (Date.now() >= deadline) throw new Error('Наблюдение приостановлено. Обновите состояние задания.')
    await new Promise<void>((resolve, reject) => {
      const abort = () => { clearTimeout(timer); signal.removeEventListener('abort', abort); reject(signal.reason) }
      const timer = setTimeout(() => { signal.removeEventListener('abort', abort); resolve() }, 800)
      signal.addEventListener('abort', abort, { once: true })
      if (signal.aborted) abort()
    })
  }
}
