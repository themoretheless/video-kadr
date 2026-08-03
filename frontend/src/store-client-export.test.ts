import 'fake-indexeddb/auto'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const api = vi.hoisted(() => ({ edit: vi.fn(), pollJob: vi.fn(), prepare: vi.fn() }))
vi.mock('./api', () => {
  class ApiError extends Error { constructor(message: string, readonly status: number, readonly code?: string) { super(message) } }
  class BackendUnavailableError extends Error {}
  return {
    clientOnlyMode: true, ApiError, BackendUnavailableError,
    edit: api.edit, pollJob: api.pollJob, prepareQueuedExportSource: api.prepare,
    cancelJob: vi.fn(), getLibrary: vi.fn(async () => []), getProjects: vi.fn(async () => []),
    getBrowserStorageStatus: vi.fn(() => null), getCapabilities: vi.fn(async () => null),
  }
})

import { defaultEdit, doExport, exportQueueState, state } from './store'

describe('ordinary client export persistence', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal('Worker', class Worker {})
    state.video = { id: `source-${crypto.randomUUID()}`, url: 'blob:source', filename: 'source.mp4', duration: 2, width: 320, height: 180, fingerprint: 'a'.repeat(64), sizeBytes: 1_000 }
    state.edit = defaultEdit(); state.edit.trimEnd = 2; state.edit.crop = { x: 0, y: 0, w: 320, h: 180 }
    state.capabilities = null; state.exporting = false; state.result = null
    api.prepare.mockResolvedValue(undefined)
    api.edit.mockResolvedValue({ jobId: 'engine-job' })
    api.pollJob.mockImplementation(async (_id: string, onTick?: (job: Record<string, unknown>) => void) => {
      onTick?.({ progress: 40, stage: 'encode' })
      return { status: 'done', result: { id: 'result', url: 'blob:result', filename: 'result.mp4', sizeBytes: 10 } }
    })
  })

  it('persists a one-variant definition before running the engine', async () => {
    await doExport()
    expect(state.exportError).toBe('')
    expect(api.edit).toHaveBeenCalledOnce()
    expect(exportQueueState.tasks.at(-1)).toMatchObject({ status: 'done', result: { id: 'result' } })
    expect(state.result).toMatchObject({ id: 'result', filename: 'result.mp4' })
  })
})
