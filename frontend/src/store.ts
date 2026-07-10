import { reactive, watch } from 'vue'
import * as api from './api'
import {
  buildEditPayload as buildPayload,
  defaultEdit,
  hasMeaningfulChanges as hasMeaningfulEditChanges,
  parseTime,
  sanitizeRect,
} from './domain/edit'
import { toast } from './toasts'
import type { EditState, Job, MediaEntry, ResultInfo, VideoInfo } from './types'

export { defaultEdit, parseTime, tierToCrf } from './domain/edit'

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
  library: [] as MediaEntry[],
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
    resetHistory()
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
  if (state.importing) return
  state.importing = true
  state.importError = ''
  state.importStatus = 'Загружаю файл…'
  state.importProgress = null
  state.importStage = 'uploading'
  state.result = null

  try {
    const v = await api.uploadFile(file)
    state.video = v
    const edit = defaultEdit()
    edit.trimEnd = v.duration
    edit.crop = { x: 0, y: 0, w: v.width, h: v.height }
    edit.scale = { w: v.width, h: -2 }
    state.edit = edit
    resetHistory()
    state.importStatus = ''
    void loadLibrary()
    toast('success', v.title ? `Загружено: ${v.title}` : 'Файл загружен')
  } catch (e) {
    state.importError = e instanceof Error ? e.message : String(e)
    state.importStatus = ''
    toast('error', state.importError)
  } finally {
    state.importing = false
    state.importProgress = null
    state.importStage = null
  }
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
// Snapshots of state.edit (as JSON) are pushed onto an undo stack, debounced so
// a drag or a burst of slider moves collapses into a single history step.

export const history = reactive({ past: [] as string[], future: [] as string[] })
let lastSnapshot = JSON.stringify(state.edit)
let historyTimer: ReturnType<typeof setTimeout> | null = null

function snapshot(): string {
  return JSON.stringify(state.edit)
}

function recordChange(): void {
  historyTimer = null
  const snap = snapshot()
  if (snap === lastSnapshot) return
  history.past.push(lastSnapshot)
  if (history.past.length > 100) history.past.shift()
  history.future = []
  lastSnapshot = snap
}

/** Drop history and pin the baseline to the current edit (on load/open). */
export function resetHistory(): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  history.past = []
  history.future = []
  lastSnapshot = snapshot()
}

function applySnapshot(json: string): void {
  if (historyTimer) {
    clearTimeout(historyTimer)
    historyTimer = null
  }
  state.edit = JSON.parse(json) as EditState
  // Pin the baseline so the watch fired by this assignment is a no-op.
  lastSnapshot = json
}

export function undo(): void {
  // Flush any pending edit into history before stepping back.
  flushPendingHistory()
  const prev = history.past.pop()
  if (prev === undefined) return
  history.future.push(snapshot())
  applySnapshot(prev)
}

export function redo(): void {
  flushPendingHistory()
  const next = history.future.pop()
  if (next === undefined) return
  history.past.push(snapshot())
  applySnapshot(next)
}

function flushPendingHistory(): void {
  if (!historyTimer) return
  clearTimeout(historyTimer)
  recordChange()
}

watch(
  () => state.edit,
  () => {
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

function capturePreset(): Partial<EditState> {
  const e = state.edit as unknown as Record<string, unknown>
  const out: Record<string, unknown> = {}
  for (const k of PRESET_KEYS) out[k] = e[k]
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

export function applyPreset(p: Preset): void {
  const source = p.edit as Record<string, unknown>
  const target = state.edit as unknown as Record<string, unknown>
  for (const key of PRESET_KEYS) {
    if (Object.hasOwn(source, key)) target[key] = source[key]
  }
  toast('info', `Пресет «${p.name}» применён`)
}

export function deletePreset(name: string): void {
  presets.list = presets.list.filter((p) => p.name !== name)
  persistPresets()
}

export function loadPresets(): void {
  try {
    const raw = localStorage.getItem('ve_presets')
    if (raw) presets.list = JSON.parse(raw) as Preset[]
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

function clearProjectSaveTimer(): void {
  if (projectSaveTimer) {
    clearTimeout(projectSaveTimer)
    projectSaveTimer = null
  }
}

/** Load the saved project for a clip (if any) and apply its edit recipe. */
async function restoreProject(videoId: string): Promise<void> {
  try {
    const p = await api.getProjectByVideo(videoId)
    // Guard against a clip switch while the lookup was in flight.
    if (p && p.edit && state.video?.id === videoId) {
      state.edit = { ...defaultEdit(), ...p.edit }
      resetHistory()
    }
  } catch {
    // Non-fatal: keep the default edit if the lookup fails.
  } finally {
    if (state.video?.id === videoId && restoringProjectFor === videoId) {
      restoredProjectFor = videoId
      restoringProjectFor = null
      clearProjectSaveTimer()
    }
  }
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
    clearProjectSaveTimer()
    projectSaveTimer = setTimeout(() => {
      projectSaveTimer = null
      void persistProject()
    }, 1000)
  },
  { deep: true },
)
