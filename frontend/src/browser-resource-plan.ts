import type { VideoInfo } from './types'

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
  const trim = typeof payload.trim === 'object' && payload.trim ? payload.trim as Record<string, unknown> : null
  const start = finite(trim?.start, 0)
  const end = finite(trim?.end, video.duration)
  const trimmedSeconds = Math.max(0.001, Math.min(video.duration || end, end) - Math.max(0, start))
  const segments = Array.isArray(payload.segments) ? payload.segments : []
  const firstSegment = typeof segments[0] === 'object' && segments[0] ? segments[0] as Record<string, unknown> : null
  const secondSegment = typeof segments[1] === 'object' && segments[1] ? segments[1] as Record<string, unknown> : null
  const removedSeconds = segments.length === 2
    ? Math.max(0, finite(secondSegment?.start, 0) - finite(firstSegment?.end, 0))
    : 0
  const selectedSeconds = Math.max(0.001, trimmedSeconds - Math.min(trimmedSeconds, removedSeconds))
  const speed = Math.max(0.5, Math.min(2, finite(payload.speed, 1)))
  const outputSeconds = selectedSeconds / speed
  const ratio = video.duration > 0 ? Math.min(1, selectedSeconds / video.duration) : 1
  const format = String(payload.format || 'mp4')
  const multiplier = format === 'gif' ? 2.5 : format === 'png' || format === 'jpg' ? 0.05 : format === 'mp3' ? 0.15 : 1.15
  const estimatedOutputBytes = Math.max(MIB, Math.ceil(inputBytes * ratio * multiplier))
  const crop = typeof payload.crop === 'object' && payload.crop ? payload.crop as Record<string, unknown> : null
  const scale = typeof payload.scale === 'object' && payload.scale ? payload.scale as Record<string, unknown> : null
  const sourceWidth = Math.max(1, finite(crop?.w, finite(video.width, 1920)))
  const sourceHeight = Math.max(1, finite(crop?.h, finite(video.height, 1080)))
  const requestedWidth = finite(scale?.w, sourceWidth)
  const requestedHeight = finite(scale?.h, sourceHeight)
  let width = Math.max(1, requestedWidth > 0 ? requestedWidth : Math.round(sourceWidth * (requestedHeight / sourceHeight)))
  let height = Math.max(1, requestedHeight > 0 ? requestedHeight : Math.round(sourceHeight * (requestedWidth / sourceWidth)))
  if (finite(payload.rotate, 0) === 90 || finite(payload.rotate, 0) === 270) [width, height] = [height, width]
  const pad = String(payload.pad || '')
  if (/^\d+:\d+$/.test(pad)) {
    const [ratioWidth, ratioHeight] = pad.split(':').map(Number)
    width = Math.max(width, Math.ceil(height * ratioWidth! / ratioHeight!))
    height = Math.max(height, Math.ceil(width * ratioHeight! / ratioWidth!))
  }
  // Reverse buffers source frames. When metadata has no FPS, fail closed with
  // a high-but-realistic capture rate instead of silently assuming 30 FPS.
  const sourceFps = Math.max(1, finite(video.fps, payload.reverse ? 120 : 30))
  const outputFps = Math.max(1, finite(payload.fps, sourceFps))
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
          : Math.max(estimatedOutputBytes, Math.ceil(outputSeconds * 8_000_000 / 8))
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
