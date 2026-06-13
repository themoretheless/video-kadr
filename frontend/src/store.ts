import { reactive } from 'vue'
import * as api from './api'
import { toast } from './toasts'
import type { EditState, Job, ResultInfo, VideoInfo } from './types'

export function defaultEdit(): EditState {
  return {
    trimStart: 0,
    trimEnd: 0,
    cropEnabled: false,
    crop: { x: 0, y: 0, w: 0, h: 0 },
    scaleEnabled: false,
    scale: { w: 1280, h: -2 },
    mute: false,
    speed: 1,
    rotate: 0,
    flipH: false,
    flipV: false,
    volume: 1,
    fadeIn: 0,
    fadeOut: 0,
    brightness: 0,
    contrast: 1,
    saturation: 1,
    filter: '',
    reverse: false,
    fps: null,
  }
}

/**
 * Parse a time string into seconds. Accepts "ss", "mm:ss", "hh:mm:ss", and a
 * fractional seconds part (e.g. "1:23.45"). Returns null for empty/invalid input.
 */
export function parseTime(input: string): number | null {
  const t = input.trim()
  if (!t) return null
  const parts = t.split(':').map((p) => p.trim())
  if (parts.some((p) => p === '' || !/^\d+(\.\d+)?$/.test(p))) return null
  let seconds = 0
  for (const p of parts) seconds = seconds * 60 + Number(p)
  return seconds
}

export const state = reactive({
  url: '',
  importStart: '',
  importEnd: '',
  importing: false,
  importStatus: '',
  importError: '',
  importProgress: null as number | null,
  importStage: null as string | null,
  importJobId: null as string | null,
  video: null as VideoInfo | null,
  edit: defaultEdit(),
  exporting: false,
  exportStatus: '',
  exportError: '',
  exportProgress: null as number | null,
  exportStage: null as string | null,
  exportJobId: null as string | null,
  result: null as ResultInfo | null,
  // Player bridge: VideoPreview owns the <video>; the rest of the app talks to
  // it through these fields.
  playerTime: 0,
  seekTo: null as number | null,
  playToggle: 0,
})

function isCancel(e: unknown): boolean {
  return e instanceof Error && e.message === 'cancelled'
}

export async function doImport(): Promise<void> {
  const url = state.url.trim()
  if (!url || state.importing) return

  // Optional import range (download only a section of long videos).
  const start = parseTime(state.importStart)
  const end = parseTime(state.importEnd)
  if (start !== null && end !== null && end <= start) {
    state.importError = 'Конец диапазона должен быть больше начала'
    toast('error', state.importError)
    return
  }
  const body: Record<string, unknown> = { url }
  if (start !== null) body.start = start
  if (end !== null) body.end = end

  state.importing = true
  state.importError = ''
  state.importStatus = 'Отправляю ссылку…'
  state.importProgress = null
  state.importStage = null
  state.result = null

  try {
    const { jobId } = await api.importUrl(body)
    state.importJobId = jobId
    state.importStatus = 'Скачиваю видео (это может занять время)…'
    const job = await api.pollJob(jobId, onImportTick)
    const v = job.result as VideoInfo
    state.video = v

    // Reset edit controls to the full clip.
    const edit = defaultEdit()
    edit.trimEnd = v.duration
    edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
    edit.scale = { w: v.width, h: -2 }
    state.edit = edit
    state.importStatus = ''
    toast('success', v.title ? `Загружено: ${v.title}` : 'Видео загружено')
  } catch (e) {
    if (isCancel(e)) {
      state.importStatus = ''
      toast('info', 'Импорт отменён')
    } else {
      state.importError = e instanceof Error ? e.message : String(e)
      state.importStatus = ''
      toast('error', state.importError)
    }
  } finally {
    state.importing = false
    state.importProgress = null
    state.importStage = null
    state.importJobId = null
  }
}

function onImportTick(job: Job): void {
  state.importProgress = typeof job.progress === 'number' ? job.progress : null
  state.importStage = job.stage ?? null
}

export async function cancelImport(): Promise<void> {
  if (state.importJobId) await api.cancelJob(state.importJobId)
}

export function buildEditPayload(): Record<string, unknown> {
  const e = state.edit
  const v = state.video
  if (!v) return {}

  const payload: Record<string, unknown> = {
    videoId: v.id,
    mute: e.mute,
    speed: e.speed,
  }
  // Only send trim when it actually narrows the clip.
  if (e.trimStart > 0.05 || e.trimEnd < v.duration - 0.05) {
    payload.trim = { start: e.trimStart, end: e.trimEnd }
  }
  if (e.cropEnabled) {
    payload.crop = { x: e.crop.x, y: e.crop.y, w: e.crop.w, h: e.crop.h }
  }
  if (e.scaleEnabled) {
    payload.scale = { w: e.scale.w, h: e.scale.h }
  }
  // Effects: only send what differs from the defaults to keep payloads small.
  if (e.rotate) payload.rotate = e.rotate
  if (e.flipH) payload.flipH = true
  if (e.flipV) payload.flipV = true
  if (e.volume !== 1) payload.volume = e.volume
  if (e.fadeIn > 0) payload.fadeIn = e.fadeIn
  if (e.fadeOut > 0) payload.fadeOut = e.fadeOut
  if (e.brightness !== 0) payload.brightness = e.brightness
  if (e.contrast !== 1) payload.contrast = e.contrast
  if (e.saturation !== 1) payload.saturation = e.saturation
  if (e.filter) payload.filter = e.filter
  if (e.reverse) payload.reverse = true
  if (e.fps) payload.fps = e.fps
  return payload
}

export async function doExport(): Promise<void> {
  if (!state.video || state.exporting) return

  state.exporting = true
  state.exportError = ''
  state.exportStatus = 'Обрабатываю видео…'
  state.exportProgress = null
  state.exportStage = null
  state.result = null

  try {
    const { jobId } = await api.edit(buildEditPayload())
    state.exportJobId = jobId
    const job = await api.pollJob(jobId, onExportTick)
    state.result = job.result as ResultInfo
    state.exportStatus = ''
    toast('success', 'Готово! Видео обработано')
  } catch (e) {
    if (isCancel(e)) {
      state.exportStatus = ''
      toast('info', 'Экспорт отменён')
    } else {
      state.exportError = e instanceof Error ? e.message : String(e)
      state.exportStatus = ''
      toast('error', state.exportError)
    }
  } finally {
    state.exporting = false
    state.exportProgress = null
    state.exportStage = null
    state.exportJobId = null
  }
}

function onExportTick(job: Job): void {
  state.exportProgress = typeof job.progress === 'number' ? job.progress : null
  state.exportStage = job.stage ?? null
}

export async function cancelExport(): Promise<void> {
  if (state.exportJobId) await api.cancelJob(state.exportJobId)
}

// --- player control helpers (used by hotkeys and trim buttons) ---

export function seekTo(t: number): void {
  const max = state.video?.duration ?? t
  state.seekTo = Math.max(0, Math.min(t, max))
}

export function seekRelative(delta: number): void {
  seekTo(state.playerTime + delta)
}

export function togglePlay(): void {
  state.playToggle++
}

export function setTrimStartFromPlayer(): void {
  if (!state.video) return
  state.edit.trimStart = Math.max(0, Math.min(state.playerTime, state.edit.trimEnd - 0.1))
}

export function setTrimEndFromPlayer(): void {
  if (!state.video) return
  state.edit.trimEnd = Math.min(state.video.duration, Math.max(state.playerTime, state.edit.trimStart + 0.1))
}
