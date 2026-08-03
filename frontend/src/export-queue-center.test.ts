import { beforeAll, describe, expect, it, vi } from 'vitest'
import 'fake-indexeddb/auto'

const calls = vi.hoisted(() => ({
  active: 0, peak: 0, order: [] as string[], sources: [] as string[], luts: [] as string[],
  editBarrier: Promise.resolve(), editEntered: undefined as undefined | (() => void),
  pollBarrier: Promise.resolve(), pollEntered: undefined as undefined | (() => void),
}))
vi.mock('./api', () => ({
  ExportDependencyUnavailableError: class ExportDependencyUnavailableError extends Error {
    constructor(public readonly kind: 'source' | 'lut', message: string) { super(message); this.name = 'ExportDependencyUnavailableError' }
  },
  clientOnlyMode: true,
  prepareQueuedExportSource: vi.fn(async (id: string) => { calls.sources.push(id) }),
  prepareQueuedExportLut: vi.fn(async (id: string) => { calls.luts.push(id) }),
  edit: vi.fn(async (payload: Record<string, unknown>) => {
    calls.editEntered?.()
    await calls.editBarrier
    return { jobId: String(payload.format) }
  }),
  pollJob: vi.fn(async (jobId: string, onTick?: (job: Record<string, unknown>) => void) => {
    calls.active++; calls.peak = Math.max(calls.peak, calls.active); calls.order.push(jobId)
    calls.pollEntered?.()
    onTick?.({ progress: 50, stage: 'encode' })
    await calls.pollBarrier
    await new Promise(resolve => setTimeout(resolve, 5))
    calls.active--
    return { id: jobId, status: 'done', result: { id: jobId, url: `blob:${jobId}`, filename: `${jobId}.mp4`, sizeBytes: 1 } }
  }),
  cancelJob: vi.fn(async () => undefined),
}))

import { cancelQueuedExport, enqueuePreparedExportBatch, exportQueueState, restoreExportQueue, waitForQueuedExport } from './export-queue-center'
import { BrowserExportQueue } from './browser-export-queue'
import { expandExportBatch } from './domain/export-variants'
import * as mockedApi from './api'

describe('persistent export queue runner', () => {
  beforeAll(async () => {
    while (exportQueueState.restoring) await new Promise(resolve => setTimeout(resolve, 1))
  })

  it('runs variants strictly sequentially and publishes fenced results', async () => {
    const suffix = crypto.randomUUID()
    const tasks = await enqueuePreparedExportBatch({
      id: `batch-${suffix}`,
      source: { assetRef: `asset-${suffix}`, fingerprint: 'a'.repeat(64) },
      basePayload: { videoId: `asset-${suffix}` },
      variants: [
        { id: 'first', label: 'First', overrides: { format: `first-${suffix}` } },
        { id: 'second', label: 'Second', overrides: { format: `second-${suffix}` } },
      ],
    })
    await Promise.all(tasks.map(task => waitForQueuedExport(task.id)))
    expect(calls.peak).toBe(1)
    expect(calls.order.slice(-2)).toEqual([`first-${suffix}`, `second-${suffix}`])
    expect(exportQueueState.tasks.filter(task => tasks.some(candidate => candidate.id === task.id)).map(task => task.status)).toEqual(['done', 'done'])
  })

  it('automatically restarts an expired WASM attempt from zero', async () => {
    const suffix = crypto.randomUUID()
    const direct = new BrowserExportQueue()
    const [definition] = expandExportBatch({
      id: `interrupted-${suffix}`,
      source: { assetRef: `asset-${suffix}`, fingerprint: 'b'.repeat(64) },
      basePayload: { videoId: `asset-${suffix}` },
      variants: [{ id: 'only', label: 'Interrupted', overrides: { format: 'mp4' } }],
    })
    await direct.enqueue([definition!])
    const claimed = await direct.claim(1)
    expect(claimed?.definition.id).toBe(definition!.id)
    await new Promise(resolve => setTimeout(resolve, 3))
    await restoreExportQueue()
    await waitForQueuedExport(definition!.id)
    expect(exportQueueState.tasks.find(job => job.id === definition!.id)).toMatchObject({ status: 'done', attempt: 2 })
    await direct.close()
  })

  it('verifies primary, multicam and LUT dependencies before execution', async () => {
    const suffix = crypto.randomUUID()
    const tasks = await enqueuePreparedExportBatch({
      id: `deps-${suffix}`,
      source: { assetRef: `primary-${suffix}`, fingerprint: 'a'.repeat(64) },
      dependencies: [
        { kind: 'source', assetRef: `angle-${suffix}`, fingerprint: 'b'.repeat(64) },
        { kind: 'lut', assetRef: `lut-${suffix}`, fingerprint: 'c'.repeat(64) },
      ],
      basePayload: {}, variants: [{ id: 'only', label: 'Dependencies', overrides: { format: `deps-${suffix}` } }],
    })
    await waitForQueuedExport(tasks[0]!.id)
    expect(calls.sources.slice(-2)).toEqual([`angle-${suffix}`, `primary-${suffix}`])
    expect(calls.luts.at(-1)).toBe(`lut-${suffix}`)
  })

  it.each([
    ['missing multicam source', 'source'],
    ['drifted LUT', 'lut'],
  ] as const)('fails before encode for a %s', async (_label, kind) => {
    const suffix = crypto.randomUUID()
    const editCalls = vi.mocked(mockedApi.edit).mock.calls.length
    const dependency = { kind, assetRef: `${kind === 'source' ? 'angle' : 'lut'}-${suffix}`, fingerprint: 'd'.repeat(64) }
    if (kind === 'source') vi.mocked(mockedApi.prepareQueuedExportSource).mockRejectedValueOnce(new Error('source missing'))
    else vi.mocked(mockedApi.prepareQueuedExportLut).mockRejectedValueOnce(new Error('LUT fingerprint drift'))
    const [task] = await enqueuePreparedExportBatch({
      id: `invalid-${suffix}`,
      source: { assetRef: `primary-${suffix}`, fingerprint: 'a'.repeat(64) },
      dependencies: [dependency], basePayload: {},
      variants: [{ id: 'only', label: 'Invalid dependency', overrides: { format: 'mp4' } }],
    })
    await expect(waitForQueuedExport(task!.id)).rejects.toThrow(kind === 'source' ? /missing/ : /drift/)
    expect(vi.mocked(mockedApi.edit).mock.calls.length).toBe(editCalls)
    const current = exportQueueState.tasks.find(item => item.id === task!.id)
    if (kind === 'source') expect(current?.status).toBe('error')
  })

  it.each([
    ['source', 'Fingerprint исходника не совпадает с сохранённым проектом. Выполните точный relink.'],
    ['lut', 'LUT lut-missing changed or is unavailable'],
  ] as const)('classifies the real %s recovery error without parsing its message', async (kind, message) => {
    const suffix = crypto.randomUUID()
    const error = new mockedApi.ExportDependencyUnavailableError(kind, message)
    if (kind === 'source') vi.mocked(mockedApi.prepareQueuedExportSource).mockRejectedValueOnce(error)
    else vi.mocked(mockedApi.prepareQueuedExportLut).mockRejectedValueOnce(error)
    const [task] = await enqueuePreparedExportBatch({
      id: `permission-${suffix}`,
      source: { assetRef: `asset-${suffix}`, fingerprint: 'e'.repeat(64) },
      dependencies: kind === 'lut' ? [{ kind: 'lut', assetRef: `lut-${suffix}`, fingerprint: 'd'.repeat(64) }] : [],
      basePayload: {}, variants: [{ id: 'only', label: 'Permission', overrides: { format: 'mp4' } }],
    })
    await expect(waitForQueuedExport(task!.id)).rejects.toThrow(message)
    const current = exportQueueState.tasks.find(item => item.id === task!.id)
    expect(current?.status).toBe('permission_required')
    expect(current?.recoveryKind).toBe(kind)
  })

  it('terminates a child created concurrently with cancellation and continues the queue', async () => {
    let releaseEdit!: () => void
    calls.editBarrier = new Promise<void>(resolve => { releaseEdit = resolve })
    const editEntered = new Promise<void>(resolve => { calls.editEntered = resolve })
    const suffix = crypto.randomUUID()
    const tasks = await enqueuePreparedExportBatch({
      id: `cancel-race-${suffix}`,
      source: { assetRef: `asset-${suffix}`, fingerprint: 'c'.repeat(64) },
      basePayload: {},
      variants: [
        { id: 'cancel', label: 'Cancel', overrides: { format: `cancel-${suffix}` } },
        { id: 'next', label: 'Next', overrides: { format: `next-${suffix}` } },
      ],
    })
    await editEntered
    await cancelQueuedExport(tasks[0]!.id)
    releaseEdit()
    calls.editBarrier = Promise.resolve()
    calls.editEntered = undefined
    await waitForQueuedExport(tasks[1]!.id)
    expect(mockedApi.cancelJob).toHaveBeenCalledWith(`cancel-${suffix}`)
    expect(exportQueueState.tasks.find(task => task.id === tasks[0]!.id)?.status).toBe('cancelled')
    expect(exportQueueState.tasks.find(task => task.id === tasks[1]!.id)?.status).toBe('done')
  })

  it('owner heartbeat terminates its child after another queue context cancels the lease', async () => {
    let releasePoll!: () => void
    calls.pollBarrier = new Promise<void>(resolve => { releasePoll = resolve })
    const pollEntered = new Promise<void>(resolve => { calls.pollEntered = resolve })
    const suffix = crypto.randomUUID()
    const [task] = await enqueuePreparedExportBatch({
      id: `foreign-cancel-${suffix}`,
      source: { assetRef: `asset-${suffix}`, fingerprint: 'f'.repeat(64) },
      basePayload: {}, variants: [{ id: 'only', label: 'Foreign cancel', overrides: { format: `foreign-${suffix}` } }],
    })
    await pollEntered
    const foreign = new BrowserExportQueue()
    expect(await foreign.cancel(task!.id)).toBe(true)
    await vi.waitFor(() => expect(mockedApi.cancelJob).toHaveBeenCalledWith(`foreign-${suffix}`), { timeout: 500 })
    releasePoll()
    calls.pollBarrier = Promise.resolve()
    calls.pollEntered = undefined
    await expect(waitForQueuedExport(task!.id)).rejects.toThrow('cancelled')
    await foreign.close()
  })
})
