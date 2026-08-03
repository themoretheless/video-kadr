import { reactive, watch } from 'vue'
import * as api from './api'
import {
  buildEditPayload as buildPayload,
  defaultEdit,
  hasMeaningfulChanges as hasMeaningfulEditChanges,
  isIdentityCurves,
  parseTime,
  resetColorAdjustments,
  sanitizeEditState,
  sanitizeRect,
} from './domain/edit'
import { cloneValue, PatchCommand } from './domain/history'
import { StructuralHistory, type TimelineCommand } from './domain/timeline'
import {
  createProjectDocumentFromLegacy,
  ensureCreatorTrackLayout,
  legacyProjectValues,
  updateLegacyProjectValues,
} from './project-schema'
import { toast } from './toasts'
import type { Capabilities, EditState, Job, LutAsset, MediaEntry, ProjectDocument, ResultInfo, VideoInfo } from './types'

export {
  defaultEdit,
  identityCurve,
  identityCurves,
  isIdentityCurve,
  isIdentityCurves,
  parseTime,
  sanitizeCurve,
  sanitizeCurves,
  sanitizeEditState,
  sanitizeLutIntensity,
  sanitizeLutId,
  sampleCurvePchip,
  tierToCrf,
} from './domain/edit'

export const MAX_LUT_UPLOAD_BYTES = 16 * 1024 * 1024
export const clientOnlyMode = api.clientOnlyMode

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
  capabilities: null as Capabilities | null,
  backendStatus: (clientOnlyMode ? 'client' : 'checking') as 'checking' | 'online' | 'offline' | 'client',
  // Player bridge: VideoPreview owns the <video>; the rest of the app talks to
  // it through these fields.
  playerTime: 0,
  seekTo: null as number | null,
  playToggle: 0,
})

// A synchronous revision guard lets async lookups prove that the user has not
// edited the current recipe while a response was in flight.
let editRevision = 0
watch(
  () => state.edit,
  () => {
    editRevision += 1
  },
  { deep: true, flush: 'sync' },
)

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
  state.importStatus = clientOnlyMode ? 'Проверяю режим импорта…' : 'Отправляю ссылку…'
  state.importProgress = null
  state.importStage = null
  state.result = null

  try {
    const { jobId } = await api.importUrl(body)
    state.importJobId = jobId
    state.importStatus = 'Скачиваю видео (это может занять время)…'
    const job = await api.pollJob(jobId, onImportTick)
    const v = job.result as VideoInfo
    resetProjectPersistenceContext()
    state.video = v

    // Reset edit controls to the full clip.
    const edit = defaultEdit()
    edit.trimEnd = v.duration
    edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
    edit.scale = { w: v.width, h: -2 }
    state.edit = edit
    resetHistory()
    initializeTimelineDocument(v)
    state.importStatus = ''
    void loadLibrary()
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

/** Import a local file via multipart upload (no job: it returns directly). */
export async function doUpload(file: File): Promise<void> {
  await doUploadFiles([file])
}

/** Upload one or more files; the first creates a project and the rest join it. */
export async function doUploadFiles(files: readonly File[]): Promise<void> {
  if (state.importing) return
  if (files.length === 0) return
  state.importing = true
  state.importError = ''
  state.importStatus = clientOnlyMode ? 'Читаю файл на устройстве…' : 'Загружаю файл…'
  state.importProgress = null
  state.importStage = 'uploading'
  state.result = null
  let targetPrimaryMediaId = timelineState.document?.primaryMediaId ?? null
  let targetSessionId = projectSessionId
  const failures: string[] = []

  try {
    for (const [index, file] of files.entries()) {
      state.importStatus = `${clientOnlyMode ? 'Читаю' : 'Загружаю'} ${index + 1} из ${files.length}: ${file.name}`
      try {
        const v = await api.uploadFile(file)
        if (projectSessionId !== targetSessionId) {
          throw new Error('проект изменился во время загрузки; файл оставлен в медиатеке')
        }
        if (targetPrimaryMediaId) {
          if (timelineState.document?.primaryMediaId !== targetPrimaryMediaId) {
            throw new Error('проект изменился во время загрузки; файл оставлен в медиатеке')
          }
          if (!addMediaToTimeline(v)) throw new Error(timelineState.error || 'не удалось добавить файл')
        } else {
          if (timelineState.document) {
            throw new Error('проект был открыт во время загрузки; файл оставлен в медиатеке')
          }
          resetProjectPersistenceContext()
          targetSessionId = projectSessionId
          state.video = v
          const edit = defaultEdit()
          edit.trimEnd = v.duration
          edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
          edit.scale = { w: v.width, h: -2 }
          state.edit = edit
          resetHistory()
          initializeTimelineDocument(v)
          targetPrimaryMediaId = v.id
        }
      } catch (error) {
        failures.push(`${file.name}: ${error instanceof Error ? error.message : String(error)}`)
      }
    }
    state.importStatus = ''
    void loadLibrary()
    if (failures.length > 0) {
      state.importError = failures.join('\n')
      toast('error', `Не добавлено файлов: ${failures.length}`)
    }
    const successes = files.length - failures.length
    if (successes > 0) toast('success', `Добавлено файлов: ${successes}`)
  } finally {
    state.importing = false
    state.importProgress = null
    state.importStage = null
  }
}

/** Add a durable library/upload source to the active project without replacing it. */
export function addMediaToTimeline(source: VideoInfo | MediaEntry): boolean {
  const document = timelineState.document
  if (!document) {
    timelineState.error = 'Сначала откройте или создайте проект'
    return false
  }
  if (restoringProjectFor === document.primaryMediaId) {
    timelineState.error = 'Дождитесь загрузки сохранённого проекта'
    return false
  }
  const duration = source.duration ?? 0
  if (!Number.isFinite(duration) || duration <= 0) {
    timelineState.error = 'У медиафайла неизвестна длительность'
    return false
  }
  const kind = source.mediaKind ??
    (source.vcodec ? 'video' : source.acodec && !source.vcodec ? 'audio' : (source.width ?? 0) > 0 ? 'video' : null)
  if (!kind) {
    timelineState.error = 'Не удалось определить тип медиафайла'
    return false
  }
  const sequence = document.sequences.find((item) => item.id === document.activeSequenceId)
  if (!sequence) {
    timelineState.error = 'Активная последовательность не найдена'
    return false
  }
  const selectedTrack = sequence.tracks.find((track) =>
    track.clips.some((clip) => clip.id === timelineState.selectedClipId),
  )
  const targetTrack =
    (selectedTrack?.kind === kind && selectedTrack.locked !== true ? selectedTrack : undefined) ??
    sequence.tracks.find((track) => track.kind === kind && track.locked !== true)
  if (!targetTrack) {
    timelineState.error = `Нет доступной ${kind === 'video' ? 'видеодорожки' : 'аудиодорожки'}`
    return false
  }
  const durationTicks = Math.round(duration * sequence.settings.timeBase)
  if (!Number.isSafeInteger(durationTicks) || durationTicks <= 0) {
    timelineState.error = 'Некорректная длительность медиафайла'
    return false
  }
  const timelineStartTick = Math.max(
    0,
    ...targetTrack.clips.map((clip) => clip.timelineStartTick + clip.durationTicks),
  )
  const clipId = `clip-${crypto.randomUUID()}`
  const metadata = cloneValue(source) as unknown as Record<string, unknown>
  const inserted = executeTimelineCommand({
    kind: 'insert_media_clip',
    sequenceId: sequence.id,
    trackId: targetTrack.id,
    index: targetTrack.clips.length,
    media: { id: source.id, kind, metadata },
    clip: {
      id: clipId,
      mediaId: source.id,
      timelineStartTick,
      durationTicks,
      sourceInTick: 0,
      sourceOutTick: durationTicks,
      effects: [],
    },
  })
  if (inserted) timelineState.selectedClipId = clipId
  return inserted
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

// --- media library ---

export async function loadLibrary(): Promise<void> {
  try {
    state.library = await api.getLibrary()
  } catch {
    // Non-fatal: the library panel just stays empty.
  }
}

export async function loadCapabilities(): Promise<void> {
  try {
    state.capabilities = await api.getCapabilities()
    state.backendStatus = clientOnlyMode ? 'client' : 'online'
  } catch (error) {
    // Older/offline backends keep the existing optimistic UI as a fallback.
    state.capabilities = null
    const proxyOutage = error instanceof api.ApiError && error.status >= 500 && !error.code
    state.backendStatus =
      error instanceof api.BackendUnavailableError || proxyOutage ? 'offline' : 'online'
  }
}

export function selectedExportUnavailableReason(): string | null {
  const document = timelineState.document
  if (document) {
    const activeSequence = document.sequences.find(
      (sequence) => sequence.id === document.activeSequenceId,
    )
    const clips = activeSequence?.tracks.flatMap((track) => track.clips) ?? []
    if (clips.length !== 1 || clips[0]?.mediaId !== document.primaryMediaId) {
      return 'Экспорт изменённой topology timeline появится после подключения render graph'
    }
  }
  const format = state.capabilities?.formats.find((option) => option.id === state.edit.format)
  if (format && !format.available) return format.reason || 'Выбранный формат недоступен'

  if (state.edit.format === 'mp3') return null

  if (state.edit.format === 'mp4') {
    const codec = state.capabilities?.codecs.find((option) => option.id === state.edit.codec)
    if (codec && !codec.available) return codec.reason || 'Выбранный кодек недоступен'
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
  if (entry.kind !== 'source') return
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
    fps: entry.fps ?? null,
    vcodec: entry.vcodec ?? null,
    acodec: entry.acodec ?? null,
    mediaKind: entry.mediaKind,
  }
  resetProjectPersistenceContext()
  state.video = v
  state.result = null
  const edit = defaultEdit()
  edit.trimEnd = v.duration
  edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
  edit.scale = { w: v.width, h: -2 }
  state.edit = edit
  resetHistory()
  initializeTimelineDocument(v)
  // Restore any saved edit for this clip (overrides the defaults above).
  void restoreProject(v.id)
  toast('info', v.title ? `Открыто: ${v.title}` : 'Клип открыт')
}

export async function deleteFromLibrary(id: string): Promise<void> {
  const referencedByActiveProject = timelineState.document?.media.some((media) => media.id === id)
  if (referencedByActiveProject) {
    toast('error', 'Файл закреплён в открытом проекте; сначала удалите или закройте проект')
    return
  }
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
export const history = reactive({ past: [] as EditCommand[], future: [] as EditCommand[] })
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

watch(
  () => state.edit,
  () => {
    if (historyTransaction) return
    if (historyTimer) clearTimeout(historyTimer)
    historyTimer = setTimeout(recordChange, 350)
  },
  { deep: true },
)

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

export const presets = reactive({ list: [] as Preset[] })
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

export const ui = reactive({ theme: 'dark' as 'dark' | 'light' })

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
let activeProjectId: string | null = null
let activeProjectRevision = 0
let activeProjectDocument: ProjectDocument | null = null
let projectSaveInFlight: Promise<void> | null = null
let projectSaveQueued = false
let projectSessionId = 0

export const timelineState = reactive({
  document: null as ProjectDocument | null,
  selectedClipId: null as string | null,
  error: '',
  revision: 0,
  canUndo: false,
  canRedo: false,
})
const structuralHistory = new StructuralHistory(32 * 1024 * 1024)

function initializeTimelineDocument(video: VideoInfo): void {
  const document = ensureCreatorTrackLayout(createProjectDocumentFromLegacy(
    video.id,
    video.title || video.filename,
    cloneValue(video) as unknown as Record<string, unknown>,
    cloneValue(state.edit) as unknown as Record<string, unknown>,
  ))
  timelineState.document = document
  timelineState.selectedClipId = document.sequences[0]?.tracks[0]?.clips[0]?.id ?? null
  timelineState.error = ''
  timelineState.revision++
  syncStructuralHistoryState()
}

function syncStructuralHistoryState(): void {
  timelineState.canUndo = structuralHistory.canUndo
  timelineState.canRedo = structuralHistory.canRedo
}

export function executeTimelineCommand(command: TimelineCommand, group?: string): boolean {
  const document = timelineState.document
  if (!document) return false
  try {
    timelineState.document = structuralHistory.execute(document, command, group)
    timelineState.error = ''
    timelineState.revision++
    syncStructuralHistoryState()
    scheduleProjectSave()
    return true
  } catch (error) {
    timelineState.error = error instanceof Error ? error.message : String(error)
    return false
  }
}

export function undoTimeline(): boolean {
  const document = timelineState.document
  if (!document || !structuralHistory.canUndo) return false
  try {
    timelineState.document = structuralHistory.undo(document)
    timelineState.revision++
    timelineState.error = ''
    syncStructuralHistoryState()
    scheduleProjectSave()
    return true
  } catch (error) {
    timelineState.error = error instanceof Error ? error.message : String(error)
    return false
  }
}

export function redoTimeline(): boolean {
  const document = timelineState.document
  if (!document || !structuralHistory.canRedo) return false
  try {
    timelineState.document = structuralHistory.redo(document)
    timelineState.revision++
    timelineState.error = ''
    syncStructuralHistoryState()
    scheduleProjectSave()
    return true
  } catch (error) {
    timelineState.error = error instanceof Error ? error.message : String(error)
    return false
  }
}

function resetProjectPersistenceContext(): void {
  projectSessionId++
  activeProjectId = null
  activeProjectRevision = 0
  activeProjectDocument = null
  projectSaveQueued = false
  timelineState.document = null
  timelineState.selectedClipId = null
  timelineState.error = ''
  structuralHistory.clear()
  syncStructuralHistoryState()
}

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
  const startingTimelineRevision = timelineState.revision
  const baseEdit = cloneValue(state.edit)
  let applied = false
  try {
    const p = await api.getProjectByVideo(videoId)
    // Guard against a clip switch while the lookup was in flight.
    if (!p?.edit || state.video?.id !== videoId || sequence !== projectRestoreSequence) return
    if (editRevision !== startingRevision || timelineState.revision !== startingTimelineRevision) return
    const envelope = await api.getProjectDocument(p.id)
    const document = envelope?.document ?? p.document ?? null
    const persistedEdit = document ? legacyProjectValues(document).edit : p.edit
    const restored = sanitizeEditState(persistedEdit, baseEdit)
    const missingLut = await resolvePersistedLut(restored)
    if (
      state.video?.id !== videoId ||
      sequence !== projectRestoreSequence ||
      editRevision !== startingRevision ||
      timelineState.revision !== startingTimelineRevision
    ) {
      return
    }
    state.edit = restored
    activeProjectId = p.id
    activeProjectRevision = envelope?.revision ?? p.revision ?? 0
    activeProjectDocument = document
    if (document) {
      timelineState.document = ensureCreatorTrackLayout(document)
      timelineState.selectedClipId =
        document.sequences[0]?.tracks.flatMap((track) => track.clips)[0]?.id ?? null
      timelineState.revision++
      structuralHistory.clear()
      syncStructuralHistoryState()
    }
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
      restoredProjectFor = applied && !changedWhileLoading ? videoId : null
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

async function performProjectSave(): Promise<void> {
  const v = state.video
  if (!v) return
  const timelineRevisionAtStart = timelineState.revision
  try {
    const projectId = activeProjectId ?? crypto.randomUUID()
    const baseDocument = timelineState.document ?? activeProjectDocument
    const document = baseDocument
      ? updateLegacyProjectValues(
          baseDocument,
          v.title || v.filename,
          cloneValue(v) as unknown as Record<string, unknown>,
          cloneValue(state.edit) as unknown as Record<string, unknown>,
        )
      : createProjectDocumentFromLegacy(
          v.id,
          v.title || v.filename,
          cloneValue(v) as unknown as Record<string, unknown>,
          cloneValue(state.edit) as unknown as Record<string, unknown>,
    )
    const saved = await api.saveProjectDocument(projectId, activeProjectRevision, document)
    if (state.video?.id !== v.id) return
    activeProjectId = saved.projectId
    activeProjectRevision = saved.revision
    activeProjectDocument = saved.document
    if (timelineState.revision === timelineRevisionAtStart) {
      timelineState.document = structuredClone(saved.document)
      timelineState.revision++
    } else {
      // A structural edit landed while this immutable snapshot was in flight.
      // Keep the newer local document and immediately CAS-save it at the new revision.
      projectSaveQueued = true
    }
  } catch (error) {
    timelineState.error = error instanceof Error ? error.message : String(error)
    if (error instanceof api.ApiError && error.status === 409) {
      toast('info', 'Проект изменён в другой вкладке; автосохранение приостановлено')
    }
    // Non-fatal: the next edit change retries the autosave.
  }
}

function persistProject(): Promise<void> {
  if (projectSaveInFlight) {
    projectSaveQueued = true
    return projectSaveInFlight
  }
  const run = (async () => {
    do {
      projectSaveQueued = false
      await performProjectSave()
    } while (projectSaveQueued)
  })()
  const tracked = run.finally(() => {
    if (projectSaveInFlight === tracked) projectSaveInFlight = null
  })
  projectSaveInFlight = tracked
  return tracked
}

/** Flush pending autosave work, coalescing with any save already in flight. */
export async function flushProjectSave(): Promise<void> {
  clearProjectSaveTimer()
  await persistProject()
}

function scheduleProjectSave(): void {
  clearProjectSaveTimer()
  projectSaveTimer = setTimeout(() => {
    projectSaveTimer = null
    void persistProject()
  }, 1000)
}

watch(
  () => [state.video, state.edit],
  () => {
    if (!state.video) return
    if (restoringProjectFor === state.video.id) {
      clearProjectSaveTimer()
      return
    }
    if (restoredProjectFor === state.video.id) {
      restoredProjectFor = null
      clearProjectSaveTimer()
      return
    }
    scheduleProjectSave()
  },
  { deep: true },
)
