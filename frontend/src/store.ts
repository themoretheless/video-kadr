import { reactive, watch } from 'vue'
import * as api from './api'
import type { ProjectDto } from './api'
import {
  discardProjectRecovery,
  getProjectDraftWatermark,
  inspectProjectRecoveryByVideo,
  inspectProjectRecovery,
  prepareProjectDraft,
  recoverProject as recoverBrowserProject,
  type ProjectRecoveryCandidate,
} from './browser-project-store'
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
import { fingerprintBlob } from './browser-asset-store'
import { planBrowserExport, type ExportResourcePlan } from './browser-resource-plan'
import type { Capabilities, EditState, Job, LutAsset, MediaEntry, ProjectDocument, ResultInfo, VideoInfo } from './types'
import { enqueueSourceAnalysis } from './derived-task-center'

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
  projects: [] as ProjectDto[],
  capabilities: null as Capabilities | null,
  backendStatus: (clientOnlyMode ? 'client' : 'checking') as 'checking' | 'online' | 'offline' | 'client',
  browserStorageWarning: '',
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
  libraryOpenSequence++

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
  libraryOpenSequence++
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
        if (clientOnlyMode) void enqueueSourceAnalysis(v, `media:${v.assetId ?? v.id}`).catch((error) => {
          toast('error', `Фоновый анализ не поставлен в очередь: ${error instanceof Error ? error.message : String(error)}`)
        })
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
  if ('availability' in source && (source.availability === 'offline' || source.availability === 'permission-required')) {
    timelineState.error = 'Сначала найдите исходный файл заново'
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
  for (const key of ['url', 'path', 'file', 'availability', 'assetId', 'fingerprint']) delete metadata[key]
  const inserted = executeTimelineCommand({
    kind: 'insert_media_clip',
    sequenceId: sequence.id,
    trackId: targetTrack.id,
    index: targetTrack.clips.length,
    media: {
      id: source.id,
      kind,
      assetRef: source.assetId ?? source.id,
      ...(source.fingerprint ? { contentFingerprint: source.fingerprint } : {}),
      metadata,
    },
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

export function streamingOutputSupported(): boolean {
  return api.streamingOutputSupported?.() ?? false
}

export async function doStreamingExport(): Promise<void> {
  if (!state.video || state.exporting) return
  state.exporting = true
  state.exportError = ''
  state.exportStatus = 'Потоково сохраняю исходный диапазон…'
  state.result = null
  try {
    state.result = await api.streamOriginalRange(buildEditPayload())
    state.exportStatus = ''
    await loadLibrary()
    toast('success', 'Потоковая WebM-копия сохранена без полного буфера в памяти')
  } catch (error) {
    if (!(error instanceof DOMException && error.name === 'AbortError')) {
      state.exportError = error instanceof Error ? error.message : String(error)
      toast('error', state.exportError)
    }
    state.exportStatus = ''
  } finally {
    state.exporting = false
  }
}

function onExportTick(job: Job): void {
  state.exportProgress = typeof job.progress === 'number' ? job.progress : null
  state.exportStage = job.stage ?? null
}

export async function cancelExport(): Promise<void> {
  if (state.exportJobId) await api.cancelJob(state.exportJobId)
  else if (state.exporting) api.cancelStreamingOutput?.()
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
    const [library, projects] = await Promise.all([api.getLibrary(), api.getProjects()])
    state.library = library
    state.projects = projects
    updateBrowserStorageWarning()
  } catch {
    // Non-fatal: the library panel just stays empty.
  }
}

function updateBrowserStorageWarning(): void {
  if (!clientOnlyMode) return
  const session = state.library.filter((entry) => entry.kind === 'source' && entry.availability === 'session')
  const missing = state.library.filter((entry) => entry.kind === 'source'
    && (entry.availability === 'offline' || entry.availability === 'permission-required'))
  const missingIds = new Set(missing.map((entry) => entry.assetId ?? entry.id))
  const affectedProjects = state.projects.filter((project) => project.document?.media.some((media) =>
    missingIds.has(media.assetRef ?? media.id),
  ))
  const warnings: string[] = []
  if (session.length) {
    const storage = api.getBrowserStorageStatus()
    const quota = storage?.risk === 'blocked' && storage.requiredBytes
      ? ` Нужно примерно ${Math.ceil(storage.requiredBytes / (1024 * 1024))} МБ, доступно ${Math.ceil((storage.availableBytes ?? 0) / (1024 * 1024))} МБ.`
      : ''
    warnings.push(`Только до закрытия вкладки: ${session.length} файл(ов) — ${session.map((entry) => entry.filename).join(', ')}.${quota}`)
  }
  if (missing.length) {
    warnings.push(`Недоступно исходников: ${missing.length}; затронуто проектов: ${affectedProjects.length}. Откройте проект и выполните точный relink.`)
  }
  if (!warnings.length) {
    const storageStatus = api.getBrowserStorageStatus()
    if (storageStatus && !storageStatus.persisted) warnings.push(
      'Браузер не гарантировал постоянное хранение. При очистке данных файлы станут offline; используйте «Найти файл» для восстановления.',
    )
  }
  state.browserStorageWarning = warnings.join(' ')
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
  const resourcePlan = currentBrowserExportPlan()
  if (resourcePlan?.risk === 'blocked') return resourcePlan.reason
  return null
}

export function currentBrowserExportPlan(): ExportResourcePlan | null {
  if (!clientOnlyMode || !state.video) return null
  return planBrowserExport(state.video, buildEditPayload())
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

let libraryOpenSequence = 0

/** Reopen a stored source clip in the editor. */
export function openFromLibrary(entry: MediaEntry): void {
  if (entry.kind !== 'source') return
  const sequence = ++libraryOpenSequence
  const session = projectSessionId
  if (!entry.url && (entry.availability === 'ready' || entry.availability === 'session' || entry.availability === undefined)) {
    void api.resolveLibrarySource(entry).then((source) => {
      if (sequence !== libraryOpenSequence || session !== projectSessionId) return
      Object.assign(entry, source, { availability: source.availability ?? 'ready' })
      openResolvedLibraryEntry(entry)
    }).catch((error: unknown) => {
      if (sequence !== libraryOpenSequence || session !== projectSessionId) return
      entry.availability = 'offline'
      toast('error', `Не удалось открыть «${entry.filename}»: ${error instanceof Error ? error.message : String(error)}`)
    })
    return
  }
  if (entry.availability === 'offline' || entry.availability === 'permission-required') entry.url = ''
  openResolvedLibraryEntry(entry)
}

function openResolvedLibraryEntry(entry: MediaEntry, selectedProject?: ProjectDto): void {
  relinkState.batchSummary = ''
  if (state.video && state.video.id !== entry.id) void flushProjectSave()
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
    assetId: entry.assetId,
    fingerprint: entry.fingerprint,
    availability: entry.availability,
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
  void restoreProject(v.id, selectedProject)
  toast('info', entry.url
    ? (v.title ? `Открыто: ${v.title}` : 'Клип открыт')
    : `Проект открыт без исходника «${entry.filename}» — найдите файл заново`)
}

export function openSavedProject(project: ProjectDto): void {
  const primary = project.document?.media.find((media) => media.id === project.document?.primaryMediaId)
  const assetRef = primary?.assetRef ?? primary?.id ?? project.videoId
  const entry = state.library.find((candidate) => (candidate.assetId ?? candidate.id) === assetRef)
  if (!entry || entry.kind !== 'source') {
    toast('error', `Исходник проекта «${project.name}» не найден в медиатеке`)
    return
  }
  const selectedFingerprint = primary?.contentFingerprint
  const sequence = ++libraryOpenSequence
  const session = projectSessionId
  if (selectedFingerprint) {
    void api.resolveLibrarySource(entry, selectedFingerprint).then((source) => {
      if (sequence !== libraryOpenSequence || session !== projectSessionId) return
      Object.assign(entry, source, { availability: source.availability ?? 'ready', fingerprint: selectedFingerprint })
      openResolvedLibraryEntry(entry, project)
    }).catch(() => {
      if (sequence !== libraryOpenSequence || session !== projectSessionId) return
      entry.url = ''
      entry.availability = 'offline'
      entry.fingerprint = selectedFingerprint
      openResolvedLibraryEntry(entry, project)
    })
    return
  }
  openResolvedLibraryEntry(entry, project)
}

export const relinkState = reactive({
  busy: {} as Record<string, boolean>,
  batchBusy: false,
  batchSummary: '',
})
const relinkTokens = new Map<string, number>()
const relinkQueues = new Map<string, Promise<boolean>>()

function nextRelinkToken(id: string): number {
  const token = (relinkTokens.get(id) ?? 0) + 1
  relinkTokens.set(id, token)
  return token
}

function activeProjectMedia(id: string) {
  return timelineState.document?.media.find((media) => (media.assetRef ?? media.id) === id)
}

export async function relinkLibraryMedia(
  entry: MediaEntry,
  file: File,
  handle?: FileSystemFileHandle,
): Promise<boolean> {
  const token = nextRelinkToken(entry.id)
  const session = projectSessionId
  const expectedMedia = cloneValue(activeProjectMedia(entry.assetId ?? entry.id))
  relinkState.busy[entry.id] = true
  const previous = relinkQueues.get(entry.id) ?? Promise.resolve(true)
  const operation = previous.catch(() => false).then(() => performRelinkLibraryMedia(entry, file, handle, token, session, expectedMedia))
  relinkQueues.set(entry.id, operation)
  return operation.finally(() => {
    if (relinkQueues.get(entry.id) === operation) relinkQueues.delete(entry.id)
  })
}

async function performRelinkLibraryMedia(
  entry: MediaEntry,
  file: File,
  handle: FileSystemFileHandle | undefined,
  token: number,
  session: number,
  expectedMedia: ReturnType<typeof activeProjectMedia>,
): Promise<boolean> {
  try {
    if (session !== projectSessionId) return false
    const source = await api.relinkLibrarySource(entry.id, file, handle, expectedMedia)
    if (clientOnlyMode) void enqueueSourceAnalysis(source, `media:${source.assetId ?? source.id}`)
    if (relinkTokens.get(entry.id) !== token || session !== projectSessionId) return false
    Object.assign(entry, source, { availability: 'ready' as const })
    if (state.video?.id === entry.id) state.video = source
    updateBrowserStorageWarning()
    toast('success', `Файл перепривязан: ${entry.filename}`)
    return true
  } catch (error) {
    if (relinkTokens.get(entry.id) !== token || session !== projectSessionId) return false
    const authoritative = await api.resolveLibrarySource(entry, expectedMedia?.contentFingerprint).catch(() => null)
    if (authoritative && relinkTokens.get(entry.id) === token && session === projectSessionId) {
      Object.assign(entry, authoritative, { availability: 'ready' as const })
      if (state.video?.id === entry.id) state.video = authoritative
    } else if (session === projectSessionId) entry.availability = 'offline'
    if (session !== projectSessionId) return false
    updateBrowserStorageWarning()
    toast('error', `Выбран другой файл для «${entry.filename}». Проект не изменён: ${error instanceof Error ? error.message : String(error)}`)
    return false
  } finally {
    if (relinkTokens.get(entry.id) === token) relinkState.busy[entry.id] = false
  }
}

export async function batchRelinkLibraryMedia(entries: MediaEntry[], files: File[]): Promise<void> {
  if (relinkState.batchBusy) return
  relinkState.batchBusy = true
  relinkState.batchSummary = `Проверяю ${files.length} файл(ов)…`
  const candidates = [...files]
  const fingerprints = new Map<string, File>()
  let recovered = 0
  const batchTokens = new Map<string, number>()
  // The caller supplies the active project's effective missing set. It can
  // include a globally-ready row whose bytes belong to another project.
  const targets = entries.filter((item) => item.kind === 'source')
  for (const entry of targets) {
    batchTokens.set(entry.id, nextRelinkToken(entry.id))
    relinkState.busy[entry.id] = true
  }
  const batchSession = projectSessionId
  const expectedMedia = new Map(targets.map((entry) => [entry.id, cloneValue(activeProjectMedia(entry.assetId ?? entry.id))]))
  try {
    // Sequential hashing keeps the working set bounded to one 4 MiB chunk (or
    // one worker) even when the picker contains many multi-gigabyte files.
    for (const file of candidates) {
      const fingerprint = await fingerprintBlob(file)
      if (!fingerprints.has(fingerprint)) fingerprints.set(fingerprint, file)
    }
    for (const entry of targets) {
      const token = batchTokens.get(entry.id)!
      let matched = false
      const media = expectedMedia.get(entry.id)
      const expectedFingerprint = media?.contentFingerprint ?? entry.fingerprint
      const orderedCandidates = expectedFingerprint && fingerprints.has(expectedFingerprint)
        ? [fingerprints.get(expectedFingerprint)!]
        : []
      for (let index = 0; index < orderedCandidates.length; index++) {
        if (projectSessionId !== batchSession || relinkTokens.get(entry.id) !== token) break
        try {
          const source = await api.relinkLibrarySource(entry.id, orderedCandidates[index]!, undefined, media)
          if (clientOnlyMode) void enqueueSourceAnalysis(source, `media:${source.assetId ?? source.id}`)
          if (relinkTokens.get(entry.id) !== token || projectSessionId !== batchSession) break
          Object.assign(entry, source, { availability: 'ready' as const })
          if (state.video?.id === entry.id) state.video = source
          recovered++
          matched = true
          break
        } catch (error) {
          if ((error as { reason?: string } | null)?.reason === 'fingerprint') continue
          const authoritative = await api.resolveLibrarySource(entry, expectedFingerprint).catch(() => null)
          if (authoritative && projectSessionId === batchSession && relinkTokens.get(entry.id) === token) {
            Object.assign(entry, authoritative, { availability: 'ready' as const })
            if (state.video?.id === entry.id) state.video = authoritative
            recovered++
            matched = true
            break
          }
          if (projectSessionId !== batchSession || relinkTokens.get(entry.id) !== token) break
          toast('error', `Не удалось сохранить замену для «${entry.filename}»: ${error instanceof Error ? error.message : String(error)}`)
          break
        }
      }
      if (projectSessionId === batchSession && relinkTokens.get(entry.id) === token) {
        relinkState.busy[entry.id] = false
        if (!matched) entry.availability = 'offline'
      }
    }
    const unresolved = targets.filter((item) => {
      const expected = expectedMedia.get(item.id)?.contentFingerprint
      return item.availability !== 'ready' || Boolean(expected && item.fingerprint !== expected)
    }).length
    if (projectSessionId === batchSession) {
      relinkState.batchSummary = unresolved
        ? `Восстановлено: ${recovered}. Осталось найти: ${unresolved}.`
        : `Все исходники восстановлены: ${recovered}.`
      toast(unresolved ? 'info' : 'success', relinkState.batchSummary)
      updateBrowserStorageWarning()
    }
  } finally {
    for (const entry of entries) if (relinkTokens.get(entry.id) === batchTokens.get(entry.id)) relinkState.busy[entry.id] = false
    relinkState.batchBusy = false
    if (projectSessionId !== batchSession) relinkState.batchSummary = ''
  }
}

export async function restoreExternalLibraryMedia(entry: MediaEntry): Promise<boolean> {
  const token = nextRelinkToken(entry.id)
  const session = projectSessionId
  const expectedFingerprint = cloneValue(activeProjectMedia(entry.assetId ?? entry.id))?.contentFingerprint
  relinkState.busy[entry.id] = true
  try {
    const source = await api.restoreExternalLibrarySource(entry.id, expectedFingerprint)
    if (clientOnlyMode) void enqueueSourceAnalysis(source, `media:${source.assetId ?? source.id}`)
    if (relinkTokens.get(entry.id) !== token || session !== projectSessionId) return false
    Object.assign(entry, source, { availability: 'ready' as const })
    if (state.video?.id === entry.id) state.video = source
    toast('success', `Доступ к «${entry.filename}» восстановлен`)
    updateBrowserStorageWarning()
    return true
  } catch (error) {
    if (relinkTokens.get(entry.id) !== token || session !== projectSessionId) return false
    entry.availability = 'permission-required'
    toast('error', error instanceof Error ? error.message : String(error))
    return false
  } finally {
    if (relinkTokens.get(entry.id) === token) relinkState.busy[entry.id] = false
  }
}

export async function deleteFromLibrary(id: string): Promise<void> {
  const referencedByActiveProject = timelineState.document?.media.some((media) => (media.assetRef ?? media.id) === id)
  if (referencedByActiveProject) {
    toast('error', 'Файл закреплён в открытом проекте; сначала удалите или закройте проект')
    return
  }
  try {
    await api.deleteLibraryItem(id)
    state.library = state.library.filter((e) => e.id !== id)
    if (state.video?.id === id) state.video = null
    if (state.result?.id === id) state.result = null
    updateBrowserStorageWarning()
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
let projectDraftInFlight: Promise<void> | null = null
let projectSaveQueued = false
let projectSessionId = 0

export const projectRecovery = reactive({
  candidate: null as ProjectRecoveryCandidate | null,
  busy: false,
  error: '',
  restoring: false,
})

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
  projectDraftInFlight = null
  projectRecovery.restoring = false
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
async function restoreProject(videoId: string, selectedProject?: ProjectDto): Promise<void> {
  projectRecovery.restoring = true
  const sequence = ++projectRestoreSequence
  const startingRevision = editRevision
  const baseEdit = cloneValue(state.edit)
  let applied = false
  try {
    if (clientOnlyMode) {
      const recovery = selectedProject
        ? await inspectProjectRecovery(selectedProject.id)
        : await inspectProjectRecoveryByVideo(videoId)
      if (state.video?.id !== videoId || sequence !== projectRestoreSequence) return
      if (recovery) {
        projectRecovery.candidate = recovery
        projectRecovery.error = ''
        return
      }
    }
    const p = selectedProject ?? await api.getProjectByVideo(videoId)
    // Guard against a clip switch while the lookup was in flight.
    if (!p?.edit || state.video?.id !== videoId || sequence !== projectRestoreSequence) return
    const envelope = await api.getProjectDocument(p.id)
    const document = envelope?.document ?? p.document ?? null
    const persistedEdit = document ? legacyProjectValues(document).edit : p.edit
    const restored = sanitizeEditState(persistedEdit, baseEdit)
    const missingLut = await resolvePersistedLut(restored)
    if (
      state.video?.id !== videoId ||
      sequence !== projectRestoreSequence
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
      restoredProjectFor = applied ? videoId : null
      restoringProjectFor = null
      clearProjectSaveTimer()
      if (!applied && (editRevision !== startingRevision || JSON.stringify(state.edit) !== JSON.stringify(baseEdit))) scheduleProjectSave()
    }
    if (sequence === projectRestoreSequence) projectRecovery.restoring = false
  }
}

export function leaveUnrecoverableProject(): void {
  clearProjectSaveTimer()
  projectRecovery.candidate = null
  projectRecovery.error = ''
  projectRecovery.restoring = false
  state.video = null
  resetProjectPersistenceContext()
}

async function resumeProjectAfterRecovery(videoId: string, projectId: string): Promise<void> {
  projectRecovery.candidate = null
  projectRecovery.error = ''
  restoredProjectFor = null
  restoringProjectFor = videoId
  state.projects = await api.getProjects()
  const project = state.projects.find((candidate) => candidate.id === projectId)
  if (!project) throw new Error('Восстановленный проект не найден; другой проект не будет открыт автоматически')
  await restoreProject(videoId, project)
}

export async function acceptProjectRecovery(): Promise<void> {
  const recovery = projectRecovery.candidate
  if (!recovery?.candidate || recovery.candidateRevision === null || projectRecovery.busy) return
  projectRecovery.busy = true
  projectRecovery.error = ''
  try {
    await recoverBrowserProject(recovery.projectId, recovery.corruptRevision, recovery.candidateRevision, recovery.journalId)
    await resumeProjectAfterRecovery(recovery.videoId, recovery.projectId)
  } catch (error) {
    projectRecovery.error = error instanceof Error ? error.message : String(error)
  } finally {
    projectRecovery.busy = false
  }
}

export async function discardAutosaveRecovery(): Promise<void> {
  const recovery = projectRecovery.candidate
  if (!recovery || projectRecovery.busy) return
  projectRecovery.busy = true
  projectRecovery.error = ''
  try {
    if (recovery.reason === 'draft' && recovery.journalId) await discardProjectRecovery(recovery.projectId, recovery.journalId)
    else if (recovery.candidate && recovery.candidateRevision !== null) {
      await recoverBrowserProject(recovery.projectId, recovery.corruptRevision, recovery.candidateRevision)
    }
    await resumeProjectAfterRecovery(recovery.videoId, recovery.projectId)
  } catch (error) {
    projectRecovery.error = error instanceof Error ? error.message : String(error)
  } finally {
    projectRecovery.busy = false
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
  if (projectRecovery.candidate || restoringProjectFor) return
  const v = state.video
  if (!v) return
  const sessionBeforeDraft = projectSessionId
  if (clientOnlyMode) await projectDraftInFlight
  if (state.video?.id !== v.id || projectSessionId !== sessionBeforeDraft || projectRecovery.candidate || restoringProjectFor) return
  const sessionAtStart = projectSessionId
  const timelineRevisionAtStart = timelineState.revision
  const projectId = activeProjectId ?? crypto.randomUUID()
  try {
    activeProjectId = projectId
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
    const writerWatermark = getProjectDraftWatermark(projectId)
    const saved = clientOnlyMode
      ? await api.saveProjectDocument(projectId, activeProjectRevision, document, writerWatermark)
      : await api.saveProjectDocument(projectId, activeProjectRevision, document)
    if (
      state.video?.id !== v.id
      || projectSessionId !== sessionAtStart
      || activeProjectId !== projectId
    ) return
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
      if (clientOnlyMode && activeProjectId) {
        const recovery = await inspectProjectRecoveryByVideo(v.id).catch(() => null)
        if (
          state.video?.id === v.id
          && projectSessionId === sessionAtStart
          && activeProjectId === projectId
        ) projectRecovery.candidate = recovery
      }
    }
    // Non-fatal: the next edit change retries the autosave.
  }
}

function currentProjectDraft(): ProjectDto | null {
  const video = state.video
  if (!video) return null
  const projectId = activeProjectId ?? crypto.randomUUID()
  activeProjectId = projectId
  const baseDocument = timelineState.document ?? activeProjectDocument
  const document = baseDocument
    ? updateLegacyProjectValues(
        baseDocument,
        video.title || video.filename,
        cloneValue(video) as unknown as Record<string, unknown>,
        cloneValue(state.edit) as unknown as Record<string, unknown>,
      )
    : createProjectDocumentFromLegacy(
        video.id,
        video.title || video.filename,
        cloneValue(video) as unknown as Record<string, unknown>,
        cloneValue(state.edit) as unknown as Record<string, unknown>,
      )
  return {
    id: projectId,
    name: document.name,
    videoId: video.id,
    video: cloneValue(video),
    edit: cloneValue(state.edit),
    document,
    revision: activeProjectRevision + 1,
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
}

async function persistRecoveryDraft(draft: ProjectDto | null, expectedRevision: number): Promise<void> {
  if (!clientOnlyMode) return
  if (!draft) return
  try {
    await prepareProjectDraft(draft, expectedRevision)
  } catch (error) {
    timelineState.error = error instanceof Error ? error.message : String(error)
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
  if (projectRecovery.candidate || restoringProjectFor) return
  await persistProject()
}

function scheduleProjectSave(): void {
  if (projectRecovery.candidate) return
  const expectedRevision = activeProjectRevision
  const recoveryDraft = clientOnlyMode ? currentProjectDraft() : null
  if (clientOnlyMode) {
    const previousDraft = projectDraftInFlight ?? Promise.resolve()
    const tracked = previousDraft.then(() => persistRecoveryDraft(recoveryDraft, expectedRevision))
    const completion = tracked.finally(() => {
      if (projectDraftInFlight === completion) projectDraftInFlight = null
    })
    projectDraftInFlight = completion
  }
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
