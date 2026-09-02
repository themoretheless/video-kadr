import * as api from '$lib/api.js'
import type { Job } from '$lib/types.js'
import { queryClient, serverKeys } from './queryClient.js'

export interface PollJobOptions {
  intervalMs?: number
  onTick?: (job: Job) => void
  signal?: AbortSignal
}

export async function pollJob(
  jobId: string,
  { onTick, signal }: PollJobOptions = {},
): Promise<Job> {
  signal?.throwIfAborted()
  const polling = api.pollJob(jobId, (job) => {
    queryClient.setQueryData(serverKeys.job(jobId), job)
    onTick?.(job)
  })
  const job = signal
    ? await Promise.race([
        polling,
        new Promise<never>((_, reject) => signal.addEventListener('abort', () => reject(signal.reason), { once: true })),
      ])
    : await polling
  queryClient.setQueryData(serverKeys.job(jobId), job)
  return job
}
