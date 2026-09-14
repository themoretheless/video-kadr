import * as api from '../../api.js'
import {
  pollJob
} from '../../../data/jobs.js'
import {
  toast
} from '../toasts.svelte.js'
import {
  loadLibrary
} from './library.svelte.js'
import {
  buildEditPayload as buildPayload,
  hasMeaningfulChanges as hasMeaningfulEditChanges,
  isIdentityCurves,
  sanitizeRect
} from '../../domain/edit.js'
import {
  supportsTimelineFormat
} from '../../domain/timeline.js'
import type {
  ResultInfo
} from '../../types'
import {
  state,
  isCancel,
  onExportTick,
  colorCapabilityUnavailableReason
} from './core.svelte.js'

export function buildEditPayload(): Record<string, unknown> {
  return buildPayload(state.edit, state.video)
}


export function hasMeaningfulChanges(): boolean {
  return hasMeaningfulEditChanges(state.edit, state.video)
}


export function normalizeCrop(): void {
  if (!state.video) return
  state.edit.crop = sanitizeRect(state.edit.crop, state.video.width, state.video.height)
}


export async function doExport(): Promise<void> {
  if (!state.video || state.exporting) return

  const unavailable = selectedExportUnavailableReason()
  if (unavailable) {
    state.exportError = unavailable
    toast('error', unavailable)
    return
  }

  state.exporting = true
  state.exportError = ''
  state.exportStatus = 'Обрабатываю видео…'
  state.exportProgress = null
  state.exportStage = null
  state.result = null

  try {
    const { jobId } = await api.edit(buildEditPayload())
    state.exportJobId = jobId
    const job = await pollJob(jobId, { onTick: onExportTick })
    state.result = job.result as ResultInfo
    state.exportStatus = ''
    void loadLibrary()
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


export async function cancelExport(): Promise<void> {
  if (state.exportJobId) await api.cancelJob(state.exportJobId)
}

// --- player control helpers (used by hotkeys and trim buttons) ---


export function selectedExportUnavailableReason(): string | null {
  const format = state.capabilities?.formats.find((option) => option.id === state.edit.format)
  if (format && !format.available) return format.reason || 'Выбранный формат недоступен'

  if (state.edit.timelineEnabled && !supportsTimelineFormat(state.edit.format)) {
    return 'Монтажная линия экспортируется только в MP4, WebM, AV1 или ProRes'
  }

  if (state.edit.format === 'mp3' || state.edit.format === 'wav') return null

  if (state.edit.format === 'mp4') {
    const codec = state.capabilities?.codecs.find((option) => option.id === state.edit.codec)
    if (codec && !codec.available) return codec.reason || 'Выбранный кодек недоступен'
  }

  if (state.edit.chromaKeyEnabled) {
    const reason = colorCapabilityUnavailableReason(
      ['chroma-key'],
      'Chroma key недоступен: нужен обновлённый сервер',
    )
    if (reason) return reason
    if (state.edit.chromaKeySpill > 1e-9) {
      const spillReason = colorCapabilityUnavailableReason(
        ['chroma-spill'],
        'Подавление chroma spill недоступно: нужен обновлённый сервер',
      )
      if (spillReason) return spillReason
    }
  }

  if (state.edit.lutId && state.edit.lutIntensity > 0) {
    const reason = colorCapabilityUnavailableReason(
      ['lut', 'lut3d', 'cube-lut'],
      '3D LUT недоступны: нужен обновлённый сервер',
    )
    if (reason) return reason
    if (state.edit.lutIntensity < 1 - 1e-9) {
      const intensityReason = colorCapabilityUnavailableReason(
        ['lut-intensity', 'lut3d-blend'],
        'Частичная интенсивность LUT недоступна: нужен обновлённый сервер',
      )
      if (intensityReason) return intensityReason
    }
  }
  if (!isIdentityCurves(state.edit.curves)) {
    const reason = colorCapabilityUnavailableReason(
      ['curves', 'color-curves', 'custom-curves'],
      'Кривые недоступны: нужен обновлённый сервер',
    )
    if (reason) return reason
  }
  return null
}

