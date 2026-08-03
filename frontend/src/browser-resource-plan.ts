import type { VideoInfo } from './types'
import { estimateExportSize, resolveExportSizing } from './domain/export-size'

export const MIB = 1024 * 1024
export const OPFS_INGEST_OVERHEAD_BYTES = MIB
export const IDB_MAX_BYTES = 128 * MIB
export const MEMFS_FALLBACK_MAX_INPUT_BYTES = 128 * MIB
export const MEMFS_MAX_OUTPUT_BYTES = 128 * MIB
export const DEFAULT_BROWSER_MEMORY_BUDGET = 384 * MIB
export const MAX_BROWSER_MEMORY_BUDGET = 1536 * MIB

export type ResourceRisk = 'safe' | 'warning' | 'blocked'

export interface RuntimeResourceCapabilities {
  worker: boolean
  wasm: boolean
  workerFs: boolean
  opfs: boolean
  webCrypto: boolean
  deviceMemoryGiB: number | null
}

export interface ImportResourcePlan {
  inputBytes: number
  transientStorageBytes: number
  freeStorageBytes: number | null
  risk: ResourceRisk
  reason: string | null
}

export interface ExportResourcePlan {
  inputBytes: number
  selectedSeconds: number
  estimatedOutputSeconds: number
  estimatedOutputBytes: number
  estimatedPeakMemoryBytes: number
  memoryBudgetBytes: number
  inputMode: 'workerfs' | 'memfs'
  risk: ResourceRisk
  reason: string | null
  suggestions: string[]
}

type NavigatorWithDeviceMemory = Navigator & { deviceMemory?: number }

export function runtimeResourceCapabilities(): RuntimeResourceCapabilities {
  const memory = typeof navigator !== 'undefined'
    ? (navigator as NavigatorWithDeviceMemory).deviceMemory
    : undefined
  const firefox = typeof navigator !== 'undefined' && /Firefox\//.test(navigator.userAgent)
  return {
    worker: typeof Worker !== 'undefined',
    wasm: typeof WebAssembly !== 'undefined',
    // @ffmpeg/core 0.12 WORKERFS aborts inside Firefox; use the explicitly
    // bounded MEMFS fallback there until a runtime-probed compatible core is shipped.
    workerFs: !firefox && typeof Worker !== 'undefined' && typeof File !== 'undefined',
    opfs: Boolean(typeof navigator !== 'undefined' && navigator.storage?.getDirectory),
    webCrypto: Boolean(globalThis.crypto?.subtle),
    deviceMemoryGiB: typeof memory === 'number' && Number.isFinite(memory) ? memory : null,
  }
}

export function browserMemoryBudget(capabilities: RuntimeResourceCapabilities): number {
  if (!capabilities.deviceMemoryGiB) return DEFAULT_BROWSER_MEMORY_BUDGET
  return Math.max(64 * MIB, Math.min(MAX_BROWSER_MEMORY_BUDGET,
    Math.floor(capabilities.deviceMemoryGiB * 1024 * MIB * 0.35)))
}

export function planBrowserImport(
  inputBytes: number,
  estimate: { usage?: number; quota?: number } | null,
): ImportResourcePlan {
  const transientStorageBytes = inputBytes * 2 + OPFS_INGEST_OVERHEAD_BYTES
  const freeStorageBytes = estimate && typeof estimate.usage === 'number' && typeof estimate.quota === 'number'
    ? Math.max(0, estimate.quota - estimate.usage)
    : null
  const blocked = freeStorageBytes !== null && transientStorageBytes > freeStorageBytes
  return {
    inputBytes,
    transientStorageBytes,
    freeStorageBytes,
    risk: blocked ? 'blocked' : freeStorageBytes !== null && transientStorageBytes > freeStorageBytes * 0.8 ? 'warning' : 'safe',
    reason: blocked
      ? `Для безопасного импорта нужно ${transientStorageBytes} байт временного места, доступно ${freeStorageBytes}.`
      : null,
  }
}

function finite(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

export function planBrowserExport(
  video: VideoInfo,
  payload: Record<string, unknown>,
  capabilities = runtimeResourceCapabilities(),
): ExportResourcePlan {
  const inputBytes = finite(video.sizeBytes, 0)
  const sizing = resolveExportSizing(video, payload)
  const { selectedSeconds, durationSeconds: outputSeconds } = sizing
  const format = String(payload.format || 'mp4')
  const { width, height } = sizing
  // Reverse buffers source frames. When metadata has no FPS, fail closed with
  // a high-but-realistic capture rate instead of silently assuming 30 FPS.
  const sourceFps = Math.max(1, finite(video.fps, payload.reverse ? 120 : 30))
  const outputFps = sizing.fps
  const rateControl = payload.rateControl && typeof payload.rateControl === 'object' ? payload.rateControl as Record<string, unknown> : null
  const sizeEstimate = estimateExportSize({
    durationSeconds: outputSeconds, width, height, fps: outputFps, format,
    codec: typeof payload.codec === 'string' ? payload.codec : undefined,
    crf: rateControl?.mode === 'quality' ? finite(rateControl.crf, finite(payload.quality, Number.NaN)) : finite(payload.quality, Number.NaN),
    muted: Boolean(payload.mute), browser: true,
    targetBytes: rateControl?.mode === 'target_size' ? finite(rateControl.targetBytes, 0) : null,
  })
  if (rateControl?.mode === 'target_size'
    && (rateControl.estimatorVersion !== sizeEstimate.contract
      || finite(rateControl.videoBitrateBps, -1) !== sizeEstimate.videoBitrateBps
      || finite(rateControl.audioBitrateBps, -1) !== sizeEstimate.audioBitrateBps)) {
    throw new Error('target-size derived bitrate mismatch')
  }
  const estimatedOutputBytes = sizeEstimate.highBytes
  // Eight bytes/pixel covers high-bit-depth/alpha filter intermediates.
  const frameBytes = Math.ceil(width * height * 8)
  const rawVideoBytes = frameBytes * outputFps * outputSeconds
  const reverseVideoBytes = payload.reverse ? frameBytes * sourceFps * selectedSeconds : 0
  const audioReverseBytes = payload.reverse && !payload.mute
    // Unknown audio layouts must cover 96 kHz, 8-channel float intermediates;
    // atempo precedes areverse, so slow motion increases the buffered duration.
    ? Math.ceil(96_000 * 8 * 4 * outputSeconds * 2)
    : 0
  const reverseBytes = payload.reverse ? Math.ceil(reverseVideoBytes * 1.5) + audioReverseBytes : 0
  const filterWorkingSet = frameBytes * (format === 'gif' ? 16 : 8)
  const inputMode = capabilities.workerFs ? 'workerfs' : 'memfs'
  const inputMaterialization = inputMode === 'memfs' ? inputBytes * 2 : 0
  const wasmBaseline = 192 * MIB
  const conservativeOutput = format === 'gif' ? Math.ceil(rawVideoBytes * 0.5)
    : format === 'png' ? frameBytes
      : format === 'jpg' ? Math.ceil(frameBytes * 0.5)
        : format === 'mp3' ? Math.ceil(outputSeconds * 32 * 1024)
          : estimatedOutputBytes
  const boundedOutputBytes = Math.max(estimatedOutputBytes, conservativeOutput)
  const estimatedPeakMemoryBytes = wasmBaseline + inputMaterialization + boundedOutputBytes * 2 + filterWorkingSet + reverseBytes
  const memoryBudgetBytes = browserMemoryBudget(capabilities)
  const suggestions = ['Сократите диапазон', 'Уменьшите разрешение', 'Используйте полноценную серверную версию']
  let reason: string | null = null
  if (!capabilities.wasm || !capabilities.worker) reason = 'Браузер не поддерживает WebAssembly Worker, необходимый для локального экспорта.'
  else if (inputMode === 'memfs' && inputBytes > MEMFS_FALLBACK_MAX_INPUT_BYTES) {
    reason = `Без потокового WORKERFS локальный экспорт ограничен ${MEMFS_FALLBACK_MAX_INPUT_BYTES} байт входного файла.`
  } else if (boundedOutputBytes > MEMFS_MAX_OUTPUT_BYTES) {
    reason = `Ожидаемый результат превышает безопасный лимит ${MEMFS_MAX_OUTPUT_BYTES} байт для bounded MEMFS output.`
  } else if (estimatedPeakMemoryBytes > memoryBudgetBytes) {
    reason = `Оценка пикового потребления памяти ${estimatedPeakMemoryBytes} байт превышает безопасный бюджет ${memoryBudgetBytes} байт.`
  }
  return {
    inputBytes, selectedSeconds, estimatedOutputSeconds: outputSeconds, estimatedOutputBytes: boundedOutputBytes, estimatedPeakMemoryBytes, memoryBudgetBytes, inputMode,
    risk: reason ? 'blocked' : estimatedPeakMemoryBytes > memoryBudgetBytes * 0.75 ? 'warning' : 'safe',
    reason, suggestions,
  }
}

export function isLikelyOutOfMemory(error: unknown): boolean {
  const message = error instanceof Error ? `${error.name}: ${error.message}` : String(error)
  return /out of memory|memory access out of bounds|cannot enlarge memory|allocation failed|wasm.*memory|runtimeerror/i.test(message)
}

export function assertBoundedBrowserOutput(byteLength: number, limit = MEMFS_MAX_OUTPUT_BYTES): void {
  if (byteLength >= limit * 0.99) {
    throw new Error('Результат достиг безопасного лимита bounded MEMFS и мог быть усечён. Сократите диапазон или используйте серверную версию.')
  }
}

export function assertCompleteMediaDuration(actualSeconds: number, expectedSeconds: number, toleranceSeconds = 1 / 30): void {
  if (!Number.isFinite(actualSeconds) || actualSeconds <= 0 || actualSeconds + toleranceSeconds < expectedSeconds) {
    throw new Error('FFmpeg создал неполный или усечённый контейнер; результат не опубликован.')
  }
}
