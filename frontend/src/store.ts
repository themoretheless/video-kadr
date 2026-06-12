import { reactive } from 'vue'
import * as api from './api'
import type { EditState, ResultInfo, VideoInfo } from './types'

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
  }
}

/**
 * Parse a time string into seconds. Accepts "ss", "mm:ss", or "hh:mm:ss".
 * Returns null for empty/invalid input.
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
  video: null as VideoInfo | null,
  edit: defaultEdit(),
  exporting: false,
  exportStatus: '',
  exportError: '',
  result: null as ResultInfo | null,
})

export async function doImport(): Promise<void> {
  const url = state.url.trim()
  if (!url || state.importing) return

  // Optional import range (download only a section of long videos).
  const start = parseTime(state.importStart)
  const end = parseTime(state.importEnd)
  if (start !== null && end !== null && end <= start) {
    state.importError = 'Конец диапазона должен быть больше начала'
    return
  }
  const body: Record<string, unknown> = { url }
  if (start !== null) body.start = start
  if (end !== null) body.end = end

  state.importing = true
  state.importError = ''
  state.importStatus = 'Отправляю ссылку…'
  state.result = null

  try {
    const { jobId } = await api.importUrl(body)
    state.importStatus = 'Скачиваю видео (это может занять время)…'
    const job = await api.pollJob(jobId)
    const v = job.result as VideoInfo
    state.video = v

    // Reset edit controls to the full clip.
    const edit = defaultEdit()
    edit.trimEnd = v.duration
    edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
    edit.scale = { w: v.width, h: -2 }
    state.edit = edit
    state.importStatus = ''
  } catch (e) {
    state.importError = e instanceof Error ? e.message : String(e)
    state.importStatus = ''
  } finally {
    state.importing = false
  }
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
  return payload
}

export async function doExport(): Promise<void> {
  if (!state.video || state.exporting) return

  state.exporting = true
  state.exportError = ''
  state.exportStatus = 'Обрабатываю видео…'
  state.result = null

  try {
    const { jobId } = await api.edit(buildEditPayload())
    const job = await api.pollJob(jobId)
    state.result = job.result as ResultInfo
    state.exportStatus = ''
  } catch (e) {
    state.exportError = e instanceof Error ? e.message : String(e)
    state.exportStatus = ''
  } finally {
    state.exporting = false
  }
}
