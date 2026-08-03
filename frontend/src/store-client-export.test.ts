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

import { defaultEdit, doExport, enqueueExportVariants, exportQueueState, state } from './store'

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

  it('snapshots target-size rate control into an ordinary queued export', async () => {
    const rateControl = { mode: 'target_size' as const, targetBytes: 5_000_000, videoBitrateBps: 8_000_000, audioBitrateBps: 128_000, estimatorVersion: 'size-v1' as const }
    await doExport(rateControl)
    expect(api.edit).toHaveBeenCalledWith(expect.objectContaining({ rateControl }))
    expect(api.edit.mock.calls.at(-1)?.[0]).not.toHaveProperty('quality')
  })

  it('fails closed before persisting an unavailable batch variant', async () => {
    state.capabilities = {
      schemaVersion: 1, toolFingerprint: 'test', filters: [], hardware: [], codecs: [],
      formats: [{ id: 'mp4', label: 'MP4', available: false, reason: 'MP4 unavailable in this runtime' }],
    }
    await expect(enqueueExportVariants([{ name: 'Blocked', format: 'mp4', codec: 'h264', qualityTier: '' }]))
      .rejects.toThrow('MP4 unavailable in this runtime')
    expect(exportQueueState.tasks.some(task => task.name === 'Blocked')).toBe(false)
  })

  it('fails closed on a blocked per-variant browser resource plan', async () => {
    state.capabilities = null
    state.video = { ...state.video!, duration: 120, sizeBytes: 2_000_000_000, width: 3840, height: 2160, fps: 60 }
    state.edit.trimEnd = 120
    await expect(enqueueExportVariants([{ name: 'Too large', format: 'mp4', codec: 'h264', qualityTier: 'high' }]))
      .rejects.toThrow(/памяти|лимит|MEMFS/i)
    expect(exportQueueState.tasks.some(task => task.name === 'Too large')).toBe(false)
  })

  it('fails closed on a blocked MP3 browser resource plan', async () => {
    state.capabilities = null
    state.video = { ...state.video!, duration: 120, sizeBytes: 2_000_000_000, width: 3840, height: 2160, fps: 60 }
    state.edit.trimEnd = 120
    await expect(enqueueExportVariants([{ name: 'MP3 too large', format: 'mp3', codec: 'h264', qualityTier: '' }]))
      .rejects.toThrow(/memory|limit|MEMFS|памяти|лимит/i)
    expect(exportQueueState.tasks.some(task => task.name === 'MP3 too large')).toBe(false)
  })
})
