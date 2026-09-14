import * as api from '../../api'
import {
  defaultEdit,
  sanitizeEditState
} from '../../domain/edit'
import {
  cloneValue
} from '../../domain/history'
import {
  activeTimelineSegments,
  MAX_TIMELINE_SEGMENTS,
  sanitizeTimelineSegments,
  supportsTimelineFormat,
  TIMELINE_MIN_SEGMENT_DURATION,
  timelineSegmentsFromLegacy,
  totalTimelineDuration
} from '../../domain/timeline'
import {
  toast
} from '../toasts.svelte.js'
import type {
  Capabilities,
  EditState,
  Job,
  MediaEntry,
  ResultInfo,
  VideoInfo
} from '../../types'

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
} from '../../domain/edit'

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
export let editRevision = 0

export function bumpEditRevision(): void {
  editRevision += 1
}

export function clearRestoredProjectFor(): void {
  restoredProjectFor = null
}

export function isCancel(e: unknown): boolean {
  return e instanceof Error && e.message === 'cancelled'
}

export function onImportTick(job: Job): void {
  state.importProgress = typeof job.progress === 'number' ? job.progress : null
  state.importStage = job.stage ?? null
}

export function lutFileValidationError(file: File): string | null {
  if (!file.name.toLowerCase().endsWith('.cube')) return 'Выберите LUT в формате .cube'
  if (file.size <= 0) return 'Файл LUT пуст'
  if (file.size > MAX_LUT_UPLOAD_BYTES) return 'Файл LUT превышает лимит 16 МБ'
  return null
}

/** Upload a LUT and attach its durable asset reference to the current edit. */
export function onExportTick(job: Job): void {
  state.exportProgress = typeof job.progress === 'number' ? job.progress : null
  state.exportStage = job.stage ?? null
}

export function colorCapabilityUnavailableReason(ids: string[], missing: string): string | null {
  const capabilities = state.capabilities
  if (!capabilities) return missing
  const option = capabilities.filters.find((candidate) =>
    ids.includes(candidate.id.toLowerCase()),
  )
  if (!option) return missing
  return option.available ? null : option.reason || missing
}

export let projectSaveTimer: ReturnType<typeof setTimeout> | null = null
export let restoringProjectFor: string | null = null
export let restoredProjectFor: string | null = null
export let projectRestoreSequence = 0

export function clearProjectSaveTimer(): void {
  if (projectSaveTimer) {
    clearTimeout(projectSaveTimer)
    projectSaveTimer = null
  }
}

/** Load the saved project for a clip (if any) and apply its edit recipe. */
export async function restoreProject(videoId: string): Promise<void> {
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
    const { resetHistory } = await import('./history.svelte.js')
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

export async function resolvePersistedLut(edit: EditState): Promise<string | null> {
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

export function toastMissingLut(label: string): void {
  toast('info', `LUT «${label}» больше недоступен и был отключён`)
}

export async function persistProject(): Promise<void> {
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

export function scheduleProjectSave(): void {
  clearProjectSaveTimer()
  projectSaveTimer = setTimeout(() => {
    projectSaveTimer = null
    void persistProject()
  }, 1000)
}



export function markRestoringProject(videoId: string): void {
  restoringProjectFor = videoId
  clearProjectSaveTimer()
}
