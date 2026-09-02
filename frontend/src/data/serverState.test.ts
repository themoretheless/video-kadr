import { afterEach, describe, expect, it, vi } from 'vitest'
import * as api from '$lib/api.js'
import { pollJob } from './jobs.js'
import { queryClient, serverKeys } from './queryClient.js'

describe('server-state cache', () => {
  afterEach(() => {
    queryClient.clear()
    vi.restoreAllMocks()
    vi.useRealTimers()
  })

  it('mirrors polling ticks and terminal state into a keyed cache', async () => {
    vi.spyOn(api, 'pollJob').mockImplementation(async (_id, onTick) => {
      onTick?.({ id: 'job-a', status: 'running', progress: 0.5 } as never)
      return { id: 'job-a', status: 'done', result: { ok: true } } as never
    })
    const result = pollJob('job-a')
    await expect(result).resolves.toMatchObject({ status: 'done' })
    expect(queryClient.getQueryData(serverKeys.job('job-a'))).toMatchObject({ status: 'done' })
  })
})
