import * as api from '../../api.js'
import {
  pollJob
} from '../../../data/jobs.js'
import {
  defaultEdit,
  parseTime,
  resetColorAdjustments
} from '../../domain/edit.js'
import {
  toast
} from '../toasts.svelte.js'
import type {
  LutAsset,
  MediaInfo,
  VideoInfo
} from '../../types'
import {
  loadLibrary
} from './library.svelte.js'
import {
  resetHistory,
  beginEditTransaction,
  endEditTransaction
} from './history.svelte.js'
import {
  state,
  isCancel,
  onImportTick,
  lutFileValidationError
} from './core.svelte.js'

export async function doImport(): Promise<VideoInfo | null> {
  const url = state.url.trim()
  if (!url || state.importing) return null

  // Optional import range (download only a section of long videos).
  const start = parseTime(state.importStart)
  const end = parseTime(state.importEnd)
  if (start !== null && end !== null && end <= start) {
    state.importError = 'Конец диапазона должен быть больше начала'
    toast('error', state.importError)
    return null
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
    const job = await pollJob(jobId, { onTick: onImportTick })
    const v = job.result as VideoInfo
    state.video = v

    // Reset edit controls to the full clip.
    const edit = defaultEdit()
    edit.trimEnd = v.duration
    edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
    edit.scale = { w: v.width, h: -2 }
    state.edit = edit
    resetHistory()
    state.importStatus = ''
    void loadLibrary()
    toast('success', v.title ? `Загружено: ${v.title}` : 'Видео загружено')
    return v
  } catch (e) {
    if (isCancel(e)) {
      state.importStatus = ''
      toast('info', 'Импорт отменён')
    } else {
      state.importError = e instanceof Error ? e.message : String(e)
      state.importStatus = ''
      toast('error', state.importError)
    }
    return null
  } finally {
    state.importing = false
    state.importProgress = null
    state.importStage = null
    state.importJobId = null
  }
}


export async function cancelImport(): Promise<void> {
  if (state.importJobId) await api.cancelJob(state.importJobId)
}

/** Import a local file via multipart upload (no job: it returns directly). */

export async function doUpload(file: File, openLegacy = true): Promise<MediaInfo | null> {
  if (state.importing) return null
  state.importing = true
  state.importError = ''
  state.importStatus = 'Загружаю файл…'
  state.importProgress = null
  state.importStage = 'uploading'
  state.result = null

  try {
    const media = await api.uploadFile(file)
    if (openLegacy && (media.mediaType === undefined || media.mediaType === 'video')) {
      const video: VideoInfo = { ...media, mediaType: 'video' }
      state.video = video
      const edit = defaultEdit()
      edit.trimEnd = video.duration
      edit.crop = { x: 0, y: 0, w: video.width, h: video.height }
      edit.scale = { w: video.width, h: -2 }
      state.edit = edit
      resetHistory()
    }
    state.importStatus = ''
    void loadLibrary()
    toast('success', media.title ? `Загружено: ${media.title}` : 'Файл загружен')
    return media
  } catch (e) {
    state.importError = e instanceof Error ? e.message : String(e)
    state.importStatus = ''
    toast('error', state.importError)
    return null
  } finally {
    state.importing = false
    state.importProgress = null
    state.importStage = null
  }
}


export async function doUploadLut(file: File): Promise<LutAsset | null> {
  if (state.lutUploading) return null
  const validationError = lutFileValidationError(file)
  if (validationError) {
    state.lutUploadError = validationError
    toast('error', validationError)
    return null
  }

  const targetVideoId = state.video?.id ?? null
  const targetEdit = state.edit
  state.lutUploading = true
  state.lutUploadError = ''
  try {
    const asset = await api.uploadLut(file)
    const id = typeof asset.id === 'string' ? asset.id.trim() : ''
    const cubeSize = Number.isFinite(asset.cubeSize) ? Math.round(asset.cubeSize) : 0
    if (!id || cubeSize < 2 || cubeSize > 65) {
      throw new Error('Сервер вернул некорректные данные LUT')
    }
    if (state.video?.id !== targetVideoId || state.edit !== targetEdit) {
      toast('info', 'LUT загружен, но не применён: открыт другой клип')
      return asset
    }
    beginEditTransaction('lut-upload')
    state.edit.lutId = id
    state.edit.lutName =
      typeof asset.name === 'string' && asset.name.trim() ? asset.name.trim() : file.name
    state.edit.lutSize = cubeSize
    state.edit.lutIntensity = 1
    endEditTransaction()
    toast('success', `LUT «${state.edit.lutName}» загружен`)
    return asset
  } catch (error) {
    state.lutUploadError = error instanceof Error ? error.message : String(error)
    toast('error', state.lutUploadError)
    return null
  } finally {
    state.lutUploading = false
  }
}

/** Detach a LUT without deleting the shared stored asset. */

export function clearLut(): void {
  beginEditTransaction('lut-clear')
  state.edit.lutId = null
  state.edit.lutName = ''
  state.edit.lutSize = null
  state.edit.lutIntensity = 1
  endEditTransaction()
  state.lutUploadError = ''
}

/** Reset the complete colour stack as one undoable action. */

export function resetColor(): void {
  beginEditTransaction('color-reset')
  resetColorAdjustments(state.edit)
  endEditTransaction()
}

