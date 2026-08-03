import { reactive } from 'vue'
import { BrowserDerivedQueue, DerivedPermissionRequiredError, type DerivedTask, type DerivedTaskInput } from './browser-derived-queue'
import * as browserMedia from './browser-media'
import type { VideoInfo } from './types'
import * as api from './api'

const queue = new BrowserDerivedQueue()
let pumping: Promise<void> | null = null
let serverPoll: number | undefined

export const derivedTaskState = reactive({
  tasks: [] as DerivedTask[],
  restoring: false,
  message: '',
})

async function refresh(): Promise<void> {
  if (api.clientOnlyMode) {
    derivedTaskState.tasks = (await queue.list()).sort((a, b) => b.createdAt - a.createdAt)
    return
  }
  const tasks = await api.listDerivedJobs()
  derivedTaskState.tasks = tasks.map(task => ({
    id: task.taskId, projectId: task.projectId, kind: task.kind, idempotencyKey: task.artifactKey,
    payloadVersion: 1, payload: null, priority: task.priority, priorityRevision: task.priorityRevision,
    dependencies: task.dependencies, state: task.state === 'pending' ? 'queued' : task.state,
    consumerProjectIds: task.consumerProjectIds,
    generation: task.generation, attempt: task.attempt, createdAt: task.enqueuedAt * 1_000,
    enqueueSequence: task.enqueuedAt,
    availableAt: task.availableAt * 1_000, leaseUntil: task.leaseUntil ? task.leaseUntil * 1_000 : undefined,
    result: task.result, error: task.error,
  }))
}

async function execute(task: DerivedTask, signal: AbortSignal): Promise<unknown> {
  if (signal.aborted) throw new DOMException('cancelled', 'AbortError')
  const payload = task.payload as { mediaId?: unknown; fingerprint?: unknown }
  if (typeof payload.mediaId !== 'string') {
    throw new Error(`Исполнитель ${task.kind} ещё недоступен в статической сборке`)
  }
  if (task.kind === 'proxy') return createBrowserProxy(payload.mediaId, typeof payload.fingerprint === 'string' ? payload.fingerprint : undefined, signal)
  if (task.kind !== 'probe') throw new Error(`Исполнитель ${task.kind} ещё недоступен в статической сборке`)
  const info = await resolveDerivedSource(payload.mediaId, typeof payload.fingerprint === 'string' ? payload.fingerprint : undefined)
  if (signal.aborted) throw new DOMException('cancelled', 'AbortError')
  return {
    duration: info.duration, width: info.width, height: info.height,
    fps: info.fps ?? null, vcodec: info.vcodec ?? null, acodec: info.acodec ?? null,
    fingerprint: info.fingerprint ?? null,
  }
}

async function resolveDerivedSource(mediaId: string, fingerprint?: string): Promise<VideoInfo> {
  try {
    return await browserMedia.resolveSource(mediaId, fingerprint)
  } catch (error) {
    const entry = (await browserMedia.getLibrary()).find(item => item.id === mediaId)
    if (entry?.availability === 'permission-required') {
      throw new DerivedPermissionRequiredError('Разрешите доступ к исходному файлу, чтобы продолжить анализ.')
    }
    throw error
  }
}

async function createBrowserProxy(mediaId: string, fingerprint: string | undefined, signal: AbortSignal): Promise<unknown> {
  if (typeof MediaRecorder === 'undefined' || !HTMLCanvasElement.prototype.captureStream) throw new Error('Браузер не поддерживает фоновый proxy recorder')
  const info = await resolveDerivedSource(mediaId, fingerprint)
  const video = document.createElement('video'); video.playsInline = true; video.muted = true; video.src = info.url
  const canvas = document.createElement('canvas')
  canvas.width = Math.min(640, info.width)
  canvas.height = Math.max(2, Math.round(info.height * canvas.width / info.width / 2) * 2)
  const context = canvas.getContext('2d'); if (!context) throw new Error('Canvas proxy unavailable')
  const stream = canvas.captureStream(15)
  const mimeType = MediaRecorder.isTypeSupported('video/webm;codecs=vp8') ? 'video/webm;codecs=vp8' : 'video/webm'
  const recorder = new MediaRecorder(stream, { mimeType, videoBitsPerSecond: 600_000 })
  const chunks: Blob[] = []; let bytes = 0; let drawing: number | undefined
  const stop = () => { video.pause(); if (recorder.state !== 'inactive') recorder.stop() }
  const abort = () => stop(); signal.addEventListener('abort', abort, { once: true })
  try {
    await new Promise<void>((resolve, reject) => { video.onloadedmetadata = () => resolve(); video.onerror = () => reject(new Error('Исходник proxy недоступен')) })
    const finished = new Promise<void>((resolve, reject) => {
      recorder.ondataavailable = event => {
        if (!event.data.size) return
        bytes += event.data.size
        if (bytes > 64 * 1024 * 1024) { reject(new Error('Proxy превысил bounded лимит 64 МБ')); stop(); return }
        chunks.push(event.data)
      }
      recorder.onerror = () => reject(new Error('Proxy recorder завершился с ошибкой'))
      recorder.onstop = () => signal.aborted ? reject(new DOMException('cancelled', 'AbortError')) : resolve()
    })
    recorder.start(1_000)
    drawing = window.setInterval(() => context.drawImage(video, 0, 0, canvas.width, canvas.height), 1000 / 15)
    video.onended = stop
    await video.play(); await finished
    const blob = new Blob(chunks, { type: mimeType })
    if (!blob.size) throw new Error('Proxy recorder создал пустой artifact')
    return { schemaVersion: 1, kind: 'proxy', width: canvas.width, height: canvas.height, duration: info.duration, mimeType, sizeBytes: blob.size, blob }
  } finally {
    if (drawing !== undefined) window.clearInterval(drawing)
    signal.removeEventListener('abort', abort); stop(); stream.getTracks().forEach(track => track.stop())
    video.removeAttribute('src'); video.load()
  }
}

async function pump(): Promise<void> {
  if (pumping) return pumping
  pumping = (async () => {
    try {
      while (document.visibilityState !== 'hidden') {
        const ran = await queue.runNext(execute)
        await refresh()
        if (!ran) break
      }
    } finally { pumping = null }
  })()
  return pumping
}

export async function initializeDerivedTasks(): Promise<void> {
  derivedTaskState.restoring = true
  try {
    const recovered = api.clientOnlyMode ? await queue.recoverExpired() : 0
    derivedTaskState.message = recovered
      ? `Возобновлено фоновых задач после перезапуска: ${recovered}`
      : 'Фоновые задачи сохраняются и возобновятся при следующем открытии страницы.'
    if (api.clientOnlyMode) {
      for (const source of (await browserMedia.getLibrary()).filter(entry => entry.kind === 'source')) {
        await enqueueSourceAnalysis(source as VideoInfo, `media:${source.assetId ?? source.id}`, false).catch(() => undefined)
      }
    }
    await refresh()
    if (api.clientOnlyMode) void pump()
    else {
      if (serverPoll !== undefined) window.clearTimeout(serverPoll)
      serverPoll = window.setTimeout(() => void pollServer(), 2_000)
    }
  } catch (error) {
    derivedTaskState.message = `Очередь фоновых задач недоступна: ${error instanceof Error ? error.message : String(error)}`
  } finally { derivedTaskState.restoring = false }
}

async function pollServer(): Promise<void> {
  try { await refresh() }
  catch (error) { derivedTaskState.message = `Очередь фоновых задач недоступна: ${error instanceof Error ? error.message : String(error)}` }
  finally { serverPoll = window.setTimeout(() => void pollServer(), 2_000) }
}

export async function enqueueSourceAnalysis(video: VideoInfo, projectId: string, start = true): Promise<void> {
  if (!api.clientOnlyMode) return
  const identity = video.fingerprint ?? `${video.filename}:${video.sizeBytes ?? 0}:${video.duration}`
  const probeId = crypto.randomUUID()
  const tasks: DerivedTaskInput[] = [{
    id: probeId, projectId, kind: 'probe' as const,
    idempotencyKey: `probe:v1:${identity}`, payloadVersion: 1,
    payload: { mediaId: video.id, fingerprint: video.fingerprint }, priority: 10,
  }]
  if (video.width > 0) tasks.push({
    id: crypto.randomUUID(), projectId, kind: 'proxy', idempotencyKey: `proxy:vp8-640-v1:${identity}`,
    payloadVersion: 1, payload: { mediaId: video.id, fingerprint: video.fingerprint }, priority: -10, dependencies: [probeId],
  })
  await queue.enqueueGraph(tasks)
  await refresh()
  if (start) void pump()
}

export async function cancelDerivedTask(id: string): Promise<void> {
  if (api.clientOnlyMode) {
    const task = derivedTaskState.tasks.find(item => item.id === id)
    const consumer = task?.consumerProjectIds.includes(task.projectId) ? task.projectId : task?.consumerProjectIds[0]
    await queue.cancel(id, consumer)
  } else {
    const task = derivedTaskState.tasks.find(item => item.id === id)
    const consumer = task?.consumerProjectIds.includes(task.projectId) ? task.projectId : task?.consumerProjectIds[0]
    await api.cancelDerivedJob(id, consumer)
  }
  await refresh()
}

export async function retryDerivedTask(id: string): Promise<void> {
  if (api.clientOnlyMode) await queue.retry(id); else await api.retryDerivedJob(id)
  await refresh(); if (api.clientOnlyMode) void pump()
}

export async function resumeDerivedPermission(task: DerivedTask): Promise<void> {
  if (!api.clientOnlyMode) return
  const payload = task.payload as { mediaId?: unknown; fingerprint?: unknown }
  if (typeof payload.mediaId !== 'string') throw new Error('У задачи нет исходного файла')
  await browserMedia.restoreExternalSource(payload.mediaId, typeof payload.fingerprint === 'string' ? payload.fingerprint : undefined)
  await queue.resumePermission(task.id); await refresh(); void pump()
}

export async function setDerivedTaskPriority(task: DerivedTask, priority: number): Promise<void> {
  if (api.clientOnlyMode) await queue.reprioritize(task.id, priority, task.priorityRevision)
  else await api.reprioritizeDerivedJob(task.id, priority, task.priorityRevision)
  await refresh(); if (api.clientOnlyMode) void pump()
}

export function onDerivedVisibilityChange(): void {
  if (document.visibilityState === 'visible') void initializeDerivedTasks()
}
