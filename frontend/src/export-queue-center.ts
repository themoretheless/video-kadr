import { reactive } from 'vue'

import * as api from './api'
import { BrowserExportQueue, type ExportQueueJob } from './browser-export-queue'
import { expandExportBatch, MAX_EXPORT_VARIANTS, type ExportBatchInput, type ExportVariantInput } from './domain/export-variants'
import type { ResultInfo } from './types'

export type ExportQueueTaskStatus = 'queued' | 'running' | 'done' | 'error' | 'cancelled' | 'interrupted' | 'permission_required'
export interface ExportQueueTaskView {
  id: string
  name: string
  status: ExportQueueTaskStatus
  progress?: number
  stage?: string
  error?: string
  result?: ResultInfo
  attempt: number
  restartCount: number
  recoveryKind?: 'source' | 'lut'
}

const queue = new BrowserExportQueue()
const activeApiJobs = new Map<string, { generation: number; jobId: string }>()
const activeAttempts = new Map<string, { generation: number; controller: AbortController }>()
const HEARTBEAT_INTERVAL_MS = import.meta.env.MODE === 'test' ? 10 : 10_000
let pumping = false
let repumpTimer: ReturnType<typeof setTimeout> | undefined

export const exportQueueState = reactive({
  tasks: [] as ExportQueueTaskView[],
  restoring: false,
  busy: false,
  message: '',
  maxVariants: MAX_EXPORT_VARIANTS,
})

function status(job: ExportQueueJob): ExportQueueTaskStatus {
  if (job.state === 'succeeded') return 'done'
  if (job.state === 'failed') return 'error'
  return job.state
}

async function refresh(): Promise<void> {
  exportQueueState.tasks = (await queue.list()).map(job => ({
    id: job.definition.id,
    name: job.definition.label,
    status: status(job),
    ...(job.progress === undefined ? {} : { progress: job.progress }),
    ...(job.stage === undefined ? {} : { stage: job.stage }),
    ...(job.error === undefined ? {} : { error: job.error }),
    ...(job.result === undefined ? {} : { result: job.result as unknown as ResultInfo }),
    attempt: job.attempt,
    restartCount: Math.max(0, job.attempt - 1),
    ...(job.recoveryKind === undefined ? {} : { recoveryKind: job.recoveryKind }),
  }))
  const remaining = exportQueueState.tasks.filter(task => ['queued', 'running', 'interrupted'].includes(task.status)).length
  exportQueueState.message = remaining ? `Осталось вариантов: ${remaining}` : exportQueueState.tasks.length ? 'Очередь завершена' : ''
}

async function abortAttempt(id: string, generation: number): Promise<void> {
  const attempt = activeAttempts.get(id)
  if (!attempt || attempt.generation !== generation || attempt.controller.signal.aborted) return
  attempt.controller.abort()
  const active = activeApiJobs.get(id)
  if (active?.generation === generation) await api.cancelJob(active.jobId)
}

async function execute(job: ExportQueueJob): Promise<void> {
  const { definition, generation } = job
  const controller = new AbortController()
  activeAttempts.set(definition.id, { generation, controller })
  let heartbeat: ReturnType<typeof setInterval> | undefined
  try {
    heartbeat = setInterval(() => {
      void queue.heartbeat(definition.id, generation).then(alive => {
        if (!alive) return abortAttempt(definition.id, generation)
      })
    }, HEARTBEAT_INTERVAL_MS)
    if (!await queue.heartbeat(definition.id, generation) || controller.signal.aborted) return
    for (const dependency of definition.dependencies) {
      if (dependency.kind === 'source') await api.prepareQueuedExportSource(dependency.assetRef, dependency.fingerprint)
      else await api.prepareQueuedExportLut(dependency.assetRef, dependency.fingerprint)
    }
    if (!await queue.heartbeat(definition.id, generation) || controller.signal.aborted) return
    const payload = structuredClone(definition.payload)
    payload.videoId = definition.source.assetRef
    const { jobId } = await api.edit(payload)
    activeApiJobs.set(definition.id, { generation, jobId })
    if (controller.signal.aborted || !await queue.heartbeat(definition.id, generation)) {
      await api.cancelJob(jobId)
      return
    }
    const completed = await api.pollJob(jobId, current => {
      void queue.progress(definition.id, generation, current.progress ?? 0, current.stage ?? 'Обработка…')
        .then(alive => { if (!alive) void abortAttempt(definition.id, generation); return refresh() })
    })
    if (!await queue.complete(definition.id, generation, completed.result as Record<string, unknown>)) {
      await api.cancelJob(jobId)
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    if (message === 'cancelled') return
    if (error instanceof api.ExportDependencyUnavailableError) await queue.requirePermission(definition.id, generation, message, error.kind)
    else await queue.fail(definition.id, generation, message)
  } finally {
    if (heartbeat) clearInterval(heartbeat)
    const active = activeApiJobs.get(definition.id)
    if (active?.generation === generation) activeApiJobs.delete(definition.id)
    const attempt = activeAttempts.get(definition.id)
    if (attempt?.generation === generation) activeAttempts.delete(definition.id)
    await refresh()
  }
}

async function pump(): Promise<void> {
  if (pumping || !api.clientOnlyMode) return
  pumping = true; exportQueueState.busy = true
  try {
    while (true) {
      const job = await queue.claim()
      if (!job) break
      await refresh()
      await execute(job)
    }
  } finally {
    pumping = false; exportQueueState.busy = false
    await refresh()
    if (exportQueueState.tasks.some(task => task.status === 'queued')) {
      if (repumpTimer) clearTimeout(repumpTimer)
      repumpTimer = setTimeout(() => { repumpTimer = undefined; void pump() }, 1_000)
    }
  }
}

export async function restoreExportQueue(): Promise<void> {
  if (!api.clientOnlyMode || exportQueueState.restoring) return
  exportQueueState.restoring = true
  try {
    const before = await queue.list()
    const liveLease = before.find(job => job.state === 'running' && (job.leaseUntil ?? 0) > Date.now())
    // Recovery is lease-aware: it leaves a live worker alone while still
    // invalidating expired attempts and session-only completed blob results.
    const recoveredIds = new Set(before.filter(job => {
      const expiredRun = job.state === 'running' && (job.leaseUntil ?? 0) <= Date.now()
      const durableResult = typeof job.result?.durableAssetRef === 'string' && job.result.durableAssetRef.length > 0
        || typeof job.result?.opfsPath === 'string' && job.result.opfsPath.length > 0
      return expiredRun || job.state === 'succeeded' && !durableResult
    }).map(job => job.definition.id))
    await queue.recoverRunning()
    // Browser WASM has no checkpoint. Recovered attempts are explicitly
    // requeued and the persisted attempt counter makes the restart visible.
    for (const id of recoveredIds) await queue.retry(id)
    await refresh()
    if (liveLease) {
      const delay = Math.max(250, Math.min(30_000, (liveLease.leaseUntil ?? Date.now()) - Date.now() + 25))
      setTimeout(() => void restoreExportQueue(), delay)
    }
  } finally { exportQueueState.restoring = false }
  void pump()
}

export async function enqueuePreparedExportBatch(input: ExportBatchInput): Promise<ExportQueueTaskView[]> {
  if (!api.clientOnlyMode) throw new Error('Persistent browser export queue доступна только в статической версии')
  const jobs = await queue.enqueue(expandExportBatch(input))
  await refresh(); void pump()
  return exportQueueState.tasks.filter(task => jobs.some(job => job.definition.id === task.id))
}

export async function cancelQueuedExport(id: string): Promise<void> {
  if (await queue.cancel(id)) {
    activeAttempts.get(id)?.controller.abort()
    const active = activeApiJobs.get(id)
    if (active) await api.cancelJob(active.jobId)
  }
  await refresh(); void pump()
}

export async function retryQueuedExport(id: string): Promise<void> {
  if (!await queue.retry(id)) throw new Error('Этот вариант нельзя запустить заново')
  await refresh(); void pump()
}

export async function waitForQueuedExport(id: string, onTick?: (task: ExportQueueTaskView) => void): Promise<ExportQueueTaskView> {
  while (true) {
    await refresh()
    const task = exportQueueState.tasks.find(item => item.id === id)
    if (!task) throw new Error('Вариант экспорта не найден')
    onTick?.(task)
    if (task.status === 'done') return task
    if (task.status === 'cancelled') throw new Error('cancelled')
    if (task.status === 'interrupted') throw new Error(task.error || 'Экспорт прерван; запустите вариант заново')
    if (task.status === 'error' || task.status === 'permission_required') throw new Error(task.error || 'Экспорт не выполнен')
    await new Promise(resolve => setTimeout(resolve, 250))
  }
}

export type { ExportVariantInput }

void restoreExportQueue().catch(error => { exportQueueState.message = error instanceof Error ? error.message : String(error); exportQueueState.restoring = false })
