// Batch export queue: the same edit submitted once per platform preset.
//
// It deliberately owns no transport of its own. Each entry applies its preset to
// `state.edit` and calls the store's `doExport`, so submission, cancellation,
// progress polling and the media library refresh all stay in one place. The
// queue only records what happened and mirrors the store's progress onto the
// entry that is currently running.
//
// Lives under `components/spatial/` for the same ownership reason as
// `exportPresets.ts`; it belongs next to the export panel.

import { reactive, watch } from 'vue'
import { beginEditTransaction, doExport, endEditTransaction, state } from '../../store'
import type { EditState, ResultInfo } from '../../types'
import { findPlatform, platformPatch, type ExportPresetPatch, type PlatformId } from './exportPresets'

export type BatchStatus = 'pending' | 'running' | 'done' | 'error' | 'skipped'

export interface BatchJob {
  id: PlatformId
  label: string
  status: BatchStatus
  /** 0..100 while running, null otherwise. */
  progress: number | null
  error: string
  url: string
  filename: string
}

export const batchState = reactive({
  running: false,
  jobs: [] as BatchJob[],
})

export function clearBatch(): void {
  if (batchState.running) return
  batchState.jobs = []
}

const PATCHED_KEYS = [
  'cropEnabled',
  'crop',
  'pad',
  'scaleEnabled',
  'scale',
  'fps',
  'format',
  'codec',
] as const

function captureFraming(): ExportPresetPatch {
  const edit = state.edit
  return {
    cropEnabled: edit.cropEnabled,
    crop: { ...edit.crop },
    pad: edit.pad,
    scaleEnabled: edit.scaleEnabled,
    scale: { ...edit.scale },
    fps: edit.fps,
    format: edit.format,
    codec: edit.codec,
  }
}

function currentResult(): ResultInfo | null {
  return state.result
}

export function applyFraming(patch: ExportPresetPatch): void {
  const edit = state.edit as unknown as Record<string, unknown>
  for (const key of PATCHED_KEYS) {
    const value = patch[key as keyof ExportPresetPatch]
    edit[key] = typeof value === 'object' && value !== null ? { ...value } : value
  }
}

/**
 * Run every requested preset in order. The whole run is one undo transaction:
 * the framing is restored at the end, so a finished batch leaves the project
 * exactly as it was and records no undo step of its own.
 */
export async function runBatchExport(ids: readonly PlatformId[]): Promise<void> {
  if (batchState.running || state.exporting) return
  const video = state.video
  if (!video) return
  const presets = ids
    .map((id) => findPlatform(id))
    .filter((preset): preset is NonNullable<typeof preset> => Boolean(preset))
  if (!presets.length) return

  batchState.jobs = presets.map((preset) => ({
    id: preset.id,
    label: preset.label,
    status: 'pending' as BatchStatus,
    progress: null,
    error: '',
    url: '',
    filename: '',
  }))
  batchState.running = true

  const stopProgress = watch(
    () => state.exportProgress,
    (progress) => {
      const running = batchState.jobs.find((job) => job.status === 'running')
      if (running) running.progress = progress
    },
  )
  const restore = captureFraming()
  beginEditTransaction('batch-export')

  try {
    for (const [index, preset] of presets.entries()) {
      const job = batchState.jobs[index]
      if (!job) continue
      // A clip switch mid-batch would export the wrong source.
      if (state.video?.id !== video.id) {
        job.status = 'skipped'
        job.error = 'Открыт другой клип'
        continue
      }
      job.status = 'running'
      applyFraming(platformPatch(video, preset))
      state.result = null
      state.exportError = ''
      await doExport()
      job.progress = null
      // Read through a function: the assignment above narrows `state.result` to
      // null for the type checker, which cannot see that the export refills it.
      const result = currentResult()
      if (result && !state.exportError) {
        job.status = 'done'
        job.url = result.url
        job.filename = result.filename
      } else {
        job.status = 'error'
        job.error = state.exportError || 'Экспорт не удался'
      }
    }
  } finally {
    stopProgress()
    applyFraming(restore)
    endEditTransaction()
    batchState.running = false
  }
}

/** Exposed for tests: the framing fields a preset run overwrites. */
export type FramingKeys = (typeof PATCHED_KEYS)[number] & keyof EditState
