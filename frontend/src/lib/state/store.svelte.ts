import * as api from '../api'
import {
  buildEditPayload as buildPayload,
  defaultEdit,
  hasMeaningfulChanges as hasMeaningfulEditChanges,
  isIdentityCurves,
  parseTime,
  resetColorAdjustments,
  sanitizeEditState,
  sanitizeRect,
} from '../domain/edit'
import { cloneValue, PatchCommand } from '../domain/history'
import {
  activeTimelineSegments,
  MAX_TIMELINE_SEGMENTS,
  nextTimelineSegmentId,
  sanitizeTimelineSegments,
  supportsTimelineFormat,
  TIMELINE_MIN_SEGMENT_DURATION,
  timelineSegmentsFromLegacy,
  totalTimelineDuration,
} from '../domain/timeline'
import { toast } from './toasts.svelte.js'
import type {
  Capabilities,
  EditState,
  Job,
  LutAsset,
  MediaEntry,
  MediaInfo,
  ResultInfo,
  TimelineSegment,
  VideoInfo,
} from '../types'

export {
  defaultEdit,
  identityColorWheels,
  identityCurve,
  identityCurves,
  identitySelectiveHsl,
  isIdentityColorWheels,
  isIdentityCurve,
  isIdentityCurves,
  isIdentitySelectiveHsl,
  parseTime,
  sanitizeCurve,
  sanitizeChromaKeyColor,
  sanitizeChromaSimilarity,
  sanitizeChromaUnit,
  sanitizeCurves,
  sanitizeColorWheels,
  sanitizeEditState,
  sanitizeSelectiveHsl,
  sanitizeLutIntensity,
  sanitizeLutId,
  sampleCurvePchip,
  sanitizeAudioCompressor,
  sanitizeAudioEq,
  sanitizeAudioLimiter,
  tierToCrf,
} from '../domain/edit'

export {
  activeTimelineSegments,
  MAX_TIMELINE_SEGMENTS,
  sanitizeTimelineSegments,
  supportsTimelineFormat,
  TIMELINE_MIN_SEGMENT_DURATION,
  timelineSegmentsFromLegacy,
  totalTimelineDuration,
}

export const MAX_LUT_UPLOAD_BYTES = 16 * 1024 * 1024

export const state = $state({
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
  lutUploading: false,
  lutUploadError: '',
  exporting: false,
  exportStatus: '',
  exportError: '',
  exportProgress: null as number | null,
  exportStage: null as string | null,
  exportJobId: null as string | null,
  result: null as ResultInfo | null,
  library: [] as MediaEntry[],
  /** True only while `library` is the latest successfully fetched snapshot. */
  librarySnapshotReady: false,
  capabilities: null as Capabilities | null,
  backendStatus: 'checking' as 'checking' | 'online' | 'offline',
  // Player bridge: VideoPreview owns the <video>; the rest of the app talks to
  // it through these fields.
  playerTime: 0,
  seekTo: null as number | null,
  seekTimelineSegmentId: null as string | null,
  timelineSelectedSegmentId: null as string | null,
  playToggle: 0,
})

// A synchronous revision guard lets async lookups prove that the user has not
// edited the current recipe while a response was in flight.
let editRevision = 0

function isCancel(e: unknown): boolean {
  return e instanceof Error && e.message === 'cancelled'
}

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
    const job = await api.pollJob(jobId, onImportTick)
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

function onImportTick(job: Job): void {
  state.importProgress = typeof job.progress === 'number' ? job.progress : null
  state.importStage = job.stage ?? null
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

function lutFileValidationError(file: File): string | null {
  if (!file.name.toLowerCase().endsWith('.cube')) return 'Выберите LUT в формате .cube'
  if (file.size <= 0) return 'Файл LUT пуст'
  if (file.size > MAX_LUT_UPLOAD_BYTES) return 'Файл LUT превышает лимит 16 МБ'
  return null
}

/** Upload a LUT and attach its durable asset reference to the current edit. */
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
    const job = await api.pollJob(jobId, onExportTick)
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

function onExportTick(job: Job): void {
  state.exportProgress = typeof job.progress === 'number' ? job.progress : null
  state.exportStage = job.stage ?? null
}

export async function cancelExport(): Promise<void> {
  if (state.exportJobId) await api.cancelJob(state.exportJobId)
}

// --- player control helpers (used by hotkeys and trim buttons) ---

export function seekTo(t: number, timelineSegmentId: string | null = null): void {
  const max = state.video?.duration ?? t
  state.seekTimelineSegmentId = timelineSegmentId
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

// --- single-source timeline ---

function timelineTransaction<T>(key: string, mutate: () => T): T {
  beginEditTransaction(key)
  try {
    return mutate()
  } finally {
    endEditTransaction()
  }
}

/** Promote the current legacy trim/cut recipe to the canonical ordered timeline. */
export function activateTimeline(): string | null {
  const duration = state.video?.duration ?? 0
  if (duration <= 0) return null
  if (state.edit.timelineEnabled) return state.edit.timelineSegments[0]?.id ?? null
  const segments = timelineSegmentsFromLegacy(state.edit, duration)
  if (!segments.length) return null
  return timelineTransaction('timeline-activate', () => {
    state.edit.timelineSegments = segments
    state.edit.timelineEnabled = true
    return segments[0]!.id
  })
}

/** Replace one range while preserving its id and the exact UI/playback order. */
export function updateTimelineSegmentRange(
  id: string,
  patch: Partial<Pick<TimelineSegment, 'start' | 'end'>>,
): boolean {
  const duration = state.video?.duration ?? 0
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const current = state.edit.timelineSegments[index]
  if (!state.edit.timelineEnabled || !current || duration <= 0) return false

  if (
    (patch.start !== undefined && !Number.isFinite(patch.start)) ||
    (patch.end !== undefined && !Number.isFinite(patch.end))
  ) {
    return false
  }
  let start = current.start
  let end = current.end
  if (patch.start !== undefined) {
    start = Math.max(0, Math.min(patch.start, end - TIMELINE_MIN_SEGMENT_DURATION))
  }
  if (patch.end !== undefined) {
    end = Math.min(duration, Math.max(patch.end, start + TIMELINE_MIN_SEGMENT_DURATION))
  }
  if (start === current.start && end === current.end) return false

  timelineTransaction('timeline-range', () => {
    state.edit.timelineSegments = state.edit.timelineSegments.map((segment, segmentIndex) =>
      segmentIndex === index ? { ...segment, start, end } : segment,
    )
  })
  return true
}

/** Split the selected range at an absolute source time. Returns the new right-hand id. */
export function splitTimelineSegment(id: string, at = state.playerTime): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const segment = state.edit.timelineSegments[index]
  if (
    !state.edit.timelineEnabled ||
    !segment ||
    state.edit.timelineSegments.length >= MAX_TIMELINE_SEGMENTS ||
    !Number.isFinite(at) ||
    at - segment.start < TIMELINE_MIN_SEGMENT_DURATION ||
    segment.end - at < TIMELINE_MIN_SEGMENT_DURATION
  ) {
    return null
  }

  const newId = nextTimelineSegmentId(state.edit.timelineSegments)
  return timelineTransaction('timeline-split', () => {
    const next = [...state.edit.timelineSegments]
    next.splice(
      index,
      1,
      { ...segment, end: at },
      { id: newId, start: at, end: segment.end },
    )
    state.edit.timelineSegments = next
    return newId
  })
}

/** Duplicate a range immediately after itself without collapsing equal source ranges. */
export function duplicateTimelineSegment(id: string): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const segment = state.edit.timelineSegments[index]
  if (
    !state.edit.timelineEnabled ||
    !segment ||
    state.edit.timelineSegments.length >= MAX_TIMELINE_SEGMENTS
  ) {
    return null
  }
  const newId = nextTimelineSegmentId(state.edit.timelineSegments)
  return timelineTransaction('timeline-duplicate', () => {
    const next = [...state.edit.timelineSegments]
    next.splice(index + 1, 0, { ...segment, id: newId })
    state.edit.timelineSegments = next
    return newId
  })
}

/** Delete a range, retaining at least one. Returns the nearest remaining selection. */
export function deleteTimelineSegment(id: string): string | null {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  if (!state.edit.timelineEnabled || index < 0 || state.edit.timelineSegments.length <= 1) {
    return null
  }
  return timelineTransaction('timeline-delete', () => {
    const next = state.edit.timelineSegments.filter((segment) => segment.id !== id)
    state.edit.timelineSegments = next
    return next[Math.min(index, next.length - 1)]!.id
  })
}

/** Move a range one position in the UI/playback order. */
export function moveTimelineSegment(id: string, direction: -1 | 1): boolean {
  const index = state.edit.timelineSegments.findIndex((segment) => segment.id === id)
  const target = index + direction
  if (
    !state.edit.timelineEnabled ||
    index < 0 ||
    target < 0 ||
    target >= state.edit.timelineSegments.length
  ) {
    return false
  }
  timelineTransaction('timeline-move', () => {
    const next = [...state.edit.timelineSegments]
    ;[next[index], next[target]] = [next[target]!, next[index]!]
    state.edit.timelineSegments = next
  })
  return true
}

// --- media library ---

let libraryLoadRevision = 0

export async function loadLibrary(): Promise<boolean> {
  const revision = ++libraryLoadRevision
  state.librarySnapshotReady = false
  try {
    const entries = await api.getLibrary()
    if (revision !== libraryLoadRevision) return false
    state.library = entries
    state.librarySnapshotReady = true
    return true
  } catch {
    if (revision === libraryLoadRevision) state.librarySnapshotReady = false
    // Non-fatal: retain the last visible snapshot, but never use it to prune
    // composition bindings after this failed refresh.
    return false
  }
}

/** Persist a partial local metadata update and replace the matching entry in-place. */
export async function updateLibraryMetadata(
  id: string,
  metadata: api.LibraryMetadataPatch,
): Promise<MediaEntry> {
  const updated = await api.patchLibraryMetadata(id, metadata)
  state.library = state.library.map((entry) => (entry.id === id ? updated : entry))
  return updated
}

export async function loadCapabilities(): Promise<void> {
  try {
    state.capabilities = await api.getCapabilities()
    state.backendStatus = 'online'
  } catch (error) {
    // Older/offline backends keep the existing optimistic UI as a fallback.
    state.capabilities = null
    const proxyOutage = error instanceof api.ApiError && error.status >= 500 && !error.code
    state.backendStatus =
      error instanceof api.BackendUnavailableError || proxyOutage ? 'offline' : 'online'
  }
}

export function selectedExportUnavailableReason(): string | null {
  const format = state.capabilities?.formats.find((option) => option.id === state.edit.format)
  if (format && !format.available) return format.reason || 'Выбранный формат недоступен'

  if (state.edit.timelineEnabled && !supportsTimelineFormat(state.edit.format)) {
    return 'Монтажная линия экспортируется только в MP4, WebM, AV1 или ProRes'
  }

  if (state.edit.format === 'mp3') return null

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

function colorCapabilityUnavailableReason(ids: string[], missing: string): string | null {
  const capabilities = state.capabilities
  if (!capabilities) return missing
  const option = capabilities.filters.find((candidate) =>
    ids.includes(candidate.id.toLowerCase()),
  )
  if (!option) return missing
  return option.available ? null : option.reason || missing
}

/** Reopen a stored source clip in the editor. */
export function openFromLibrary(entry: MediaEntry): void {
  if (entry.kind !== 'source' || (entry.mediaType && entry.mediaType !== 'video')) return
  restoringProjectFor = entry.id
  clearProjectSaveTimer()
  const v: VideoInfo = {
    id: entry.id,
    url: entry.url,
    filename: entry.filename,
    duration: entry.duration ?? 0,
    width: entry.width ?? 0,
    height: entry.height ?? 0,
    title: entry.title ?? null,
    sizeBytes: entry.sizeBytes ?? null,
    mediaType: 'video',
  }
  state.video = v
  state.result = null
  const edit = defaultEdit()
  edit.trimEnd = v.duration
  edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
  edit.scale = { w: v.width, h: -2 }
  state.edit = edit
  resetHistory()
  // Restore any saved edit for this clip (overrides the defaults above).
  void restoreProject(v.id)
  toast('info', v.title ? `Открыто: ${v.title}` : 'Клип открыт')
}

export async function deleteFromLibrary(id: string): Promise<void> {
  try {
    await api.deleteLibraryItem(id)
    state.library = state.library.filter((e) => e.id !== id)
    if (state.video?.id === id) state.video = null
    toast('info', 'Удалено')
  } catch (e) {
    toast('error', e instanceof Error ? e.message : String(e))
  }
}

// --- edit history (undo / redo) ---
// Commands retain only changed EditState fields. Explicit interaction
// transactions make every pointer drag one undo step; other rapid controls are
// grouped by the debounce boundary.

type EditCommand = PatchCommand<EditState>
export const history = $state({ past: [] as EditCommand[], future: [] as EditCommand[] })
let historyBaseline = cloneValue(state.edit)
let historyTimer: ReturnType<typeof setTimeout> | null = null
let historyTransaction: { key: string; before: EditState; depth: number } | null = null

function commitHistory(command: EditCommand | null): void {
  if (!command) return
  history.past.push(command)
  if (history.past.length > 100) history.past.shift()
  history.future = []
}

function recordChange(): void {
  historyTimer = null
  const current = cloneValue(state.edit)
  commitHistory(PatchCommand.between(historyBaseline, current, 'debounced-edit'))
  historyBaseline = current
}

/** Drop history and pin the baseline to the current edit (on load/open). */
export function resetHistory(): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  history.past = []
  history.future = []
  historyTransaction = null
  historyBaseline = cloneValue(state.edit)
}

function applyHistory(command: EditCommand): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  state.edit = command.apply(state.edit)
  // Pin the baseline so the watch fired by this assignment records no command.
  historyBaseline = cloneValue(state.edit)
}

export function undo(): void {
  // Flush any pending edit into history before stepping back.
  flushPendingHistory()
  const command = history.past.pop()
  if (!command) return
  history.future.push(command)
  applyHistory(command.invert() as EditCommand)
}

export function redo(): void {
  flushPendingHistory()
  const command = history.future.pop()
  if (!command) return
  history.past.push(command)
  applyHistory(command)
}

function flushPendingHistory(): void {
  if (historyTransaction) {
    historyTransaction.depth = 1
    endEditTransaction()
  }
  if (!historyTimer) return
  clearTimeout(historyTimer)
  recordChange()
}

export function beginEditTransaction(key: string): void {
  flushPendingHistory()
  if (historyTransaction) {
    historyTransaction.depth++
    return
  }
  historyTransaction = { key, before: cloneValue(state.edit), depth: 1 }
}

export function endEditTransaction(): void {
  const transaction = historyTransaction
  if (!transaction) return
  transaction.depth--
  if (transaction.depth > 0) return
  const current = cloneValue(state.edit)
  commitHistory(PatchCommand.between(transaction.before, current, transaction.key))
  historyBaseline = current
  historyTransaction = null
}


// --- effect presets (reusable "looks", persisted to localStorage) ---
// A preset captures the reusable effect fields, not clip-specific geometry
// (trim/cut/crop/censor/scale stay tied to the current video).

export interface Preset {
  name: string
  edit: Partial<EditState>
}

const PRESET_KEYS: (keyof EditState)[] = [
  'speed',
  'rotate',
  'flipH',
  'flipV',
  'mute',
  'volume',
  'fadeIn',
  'fadeOut',
  'normalizeAudio',
  'highpass',
  'brightness',
  'contrast',
  'saturation',
  'chromaKeyEnabled',
  'chromaKeyColor',
  'chromaKeySimilarity',
  'chromaKeyBlend',
  'chromaKeySpill',
  'filter',
  'lutId',
  'lutName',
  'lutSize',
  'lutIntensity',
  'curves',
  'reverse',
  'fps',
  'vignette',
  'denoise',
  'sharpen',
  'grain',
  'censorColor',
  'pad',
]

export const presets = $state({ list: [] as Preset[] })
let presetApplySequence = 0

function capturePreset(): Partial<EditState> {
  const e = state.edit as unknown as Record<string, unknown>
  const out: Record<string, unknown> = {}
  for (const k of PRESET_KEYS) out[k] = cloneValue(e[k])
  return out as Partial<EditState>
}

function persistPresets(): void {
  try {
    localStorage.setItem('ve_presets', JSON.stringify(presets.list))
  } catch {
    // Private mode / quota: presets just stay in-memory for this session.
  }
}

export function savePreset(name: string): void {
  const n = name.trim()
  if (!n) return
  const entry: Preset = { name: n, edit: capturePreset() }
  const idx = presets.list.findIndex((p) => p.name === n)
  if (idx >= 0) presets.list[idx] = entry
  else presets.list.push(entry)
  persistPresets()
  toast('success', `Пресет «${n}» сохранён`)
}

export async function applyPreset(p: Preset): Promise<void> {
  const sequence = ++presetApplySequence
  const targetVideoId = state.video?.id ?? null
  const targetEdit = state.edit
  const startingRevision = editRevision
  const source = p.edit as Record<string, unknown>
  const target = targetEdit as unknown as Record<string, unknown>
  const sanitizedEdit = sanitizeEditState(source, targetEdit)
  const missingLut = Object.hasOwn(source, 'lutId')
    ? await resolvePersistedLut(sanitizedEdit)
    : null
  if (
    sequence !== presetApplySequence ||
    state.video?.id !== targetVideoId ||
    state.edit !== targetEdit ||
    editRevision !== startingRevision
  ) {
    return
  }
  const sanitized = sanitizedEdit as unknown as Record<string, unknown>
  beginEditTransaction('preset-apply')
  for (const key of PRESET_KEYS) {
    if (Object.hasOwn(source, key)) target[key] = cloneValue(sanitized[key])
  }
  endEditTransaction()
  if (missingLut) toastMissingLut(missingLut)
  toast('info', `Пресет «${p.name}» применён`)
}

export function deletePreset(name: string): void {
  presets.list = presets.list.filter((p) => p.name !== name)
  persistPresets()
}

export function loadPresets(): void {
  try {
    const raw = localStorage.getItem('ve_presets')
    if (!raw) return
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return
    presets.list = parsed.flatMap((candidate): Preset[] => {
      if (typeof candidate !== 'object' || candidate === null || Array.isArray(candidate)) return []
      const record = candidate as Record<string, unknown>
      if (typeof record.name !== 'string' || !record.name.trim()) return []
      const source =
        typeof record.edit === 'object' && record.edit !== null && !Array.isArray(record.edit)
          ? record.edit as Record<string, unknown>
          : {}
      const sanitized = sanitizeEditState(source) as unknown as Record<string, unknown>
      const edit: Record<string, unknown> = {}
      for (const key of PRESET_KEYS) {
        if (Object.hasOwn(source, key)) edit[key] = cloneValue(sanitized[key])
      }
      return [{ name: record.name.trim(), edit: edit as Partial<EditState> }]
    })
  } catch {
    // Ignore malformed storage; start with an empty preset list.
  }
}

// --- theme (dark default, light alternative; persisted) ---

export const ui = $state({ theme: 'dark' as 'dark' | 'light' })

function applyTheme(t: 'dark' | 'light'): void {
  ui.theme = t
  document.documentElement.dataset.theme = t
  try {
    localStorage.setItem('ve_theme', t)
  } catch {
    // Non-fatal: the theme just won't persist across reloads.
  }
}

export function initTheme(): void {
  let t: 'dark' | 'light' = 'dark'
  try {
    const saved = localStorage.getItem('ve_theme')
    if (saved === 'light' || saved === 'dark') t = saved
  } catch {
    // Ignore and keep the default.
  }
  applyTheme(t)
}

export function toggleTheme(): void {
  applyTheme(ui.theme === 'dark' ? 'light' : 'dark')
}

// --- project autosave / restore (persisted server-side in SQLite) ---
// The current clip + edit autosave (debounced) to a project keyed by the clip.
// Reopening a clip from the library restores its saved edit instead of resetting
// to defaults. Failures are non-fatal: the editor still works without the backend.

let projectSaveTimer: ReturnType<typeof setTimeout> | null = null
let restoringProjectFor: string | null = null
let restoredProjectFor: string | null = null
let projectRestoreSequence = 0

function clearProjectSaveTimer(): void {
  if (projectSaveTimer) {
    clearTimeout(projectSaveTimer)
    projectSaveTimer = null
  }
}

/** Load the saved project for a clip (if any) and apply its edit recipe. */
async function restoreProject(videoId: string): Promise<void> {
  const sequence = ++projectRestoreSequence
  const startingRevision = editRevision
  const baseEdit = cloneValue(state.edit)
  let applied = false
  try {
    const p = await api.getProjectByVideo(videoId)
    // Guard against a clip switch while the lookup was in flight.
    if (!p?.edit || state.video?.id !== videoId || sequence !== projectRestoreSequence) return
    if (editRevision !== startingRevision) return
    const restored = sanitizeEditState(p.edit, baseEdit)
    const missingLut = await resolvePersistedLut(restored)
    if (
      state.video?.id !== videoId ||
      sequence !== projectRestoreSequence ||
      editRevision !== startingRevision
    ) {
      return
    }
    state.edit = restored
    resetHistory()
    applied = true
    if (missingLut) toastMissingLut(missingLut)
  } catch {
    // Non-fatal: keep the default edit if the lookup fails.
  } finally {
    if (
      state.video?.id === videoId &&
      restoringProjectFor === videoId &&
      sequence === projectRestoreSequence
    ) {
      const changedWhileLoading = !applied && editRevision !== startingRevision
      restoredProjectFor = changedWhileLoading ? null : videoId
      restoringProjectFor = null
      clearProjectSaveTimer()
      if (changedWhileLoading) scheduleProjectSave()
    }
  }
}

async function resolvePersistedLut(edit: EditState): Promise<string | null> {
  const id = edit.lutId
  if (!id) return null
  try {
    const asset = await api.getLut(id)
    if (edit.lutId !== id) return null
    edit.lutName = asset.name
    edit.lutSize = asset.cubeSize
  } catch (error) {
    if (!(error instanceof api.ApiError) || error.status !== 404 || edit.lutId !== id) return null
    const label = edit.lutName || id
    edit.lutId = null
    edit.lutName = ''
    edit.lutSize = null
    edit.lutIntensity = 1
    return label
  }
  return null
}

function toastMissingLut(label: string): void {
  toast('info', `LUT «${label}» больше недоступен и был отключён`)
}

async function persistProject(): Promise<void> {
  const v = state.video
  if (!v) return
  try {
    await api.saveProject({
      videoId: v.id,
      video: v,
      edit: state.edit,
      name: v.title || v.filename,
    })
  } catch {
    // Non-fatal: the next edit change retries the autosave.
  }
}

function scheduleProjectSave(): void {
  clearProjectSaveTimer()
  projectSaveTimer = setTimeout(() => {
    projectSaveTimer = null
    void persistProject()
  }, 1000)
}



let disposeStateEffects: (() => void) | null = null

/**
 * Start the two browser-side deep rune effects:
 * edit history/revision tracking and debounced project autosave.
 *
 * Calling this more than once is harmless. The returned function is mainly
 * useful to isolated tests and HMR; the app keeps the effects for its lifetime.
 */
export function initStateEffects(): () => void {
  if (disposeStateEffects) return disposeStateEffects

  const disposeRoot = $effect.root(() => {
    let editEffectReady = false
    $effect(() => {
      // JSON.stringify deliberately touches every nested rune property.
      JSON.stringify(state.edit)
      if (!editEffectReady) {
        editEffectReady = true
        return
      }

      editRevision += 1
      if (historyTransaction) return
      if (historyTimer) clearTimeout(historyTimer)
      historyTimer = setTimeout(recordChange, 350)
    })

    let autosaveEffectReady = false
    $effect(() => {
      const videoId = state.video?.id ?? null
      JSON.stringify(state.edit)
      if (!autosaveEffectReady) {
        autosaveEffectReady = true
        return
      }
      if (!videoId) {
        clearProjectSaveTimer()
        return
      }
      if (restoringProjectFor === videoId) {
        clearProjectSaveTimer()
        return
      }
      if (restoredProjectFor === videoId) {
        restoredProjectFor = null
        clearProjectSaveTimer()
        return
      }
      scheduleProjectSave()
    })

    return () => {
      if (historyTimer) {
        clearTimeout(historyTimer)
        historyTimer = null
      }
      clearProjectSaveTimer()
    }
  })

  disposeStateEffects = () => {
    disposeStateEffects = null
    disposeRoot()
  }
  return disposeStateEffects
}
