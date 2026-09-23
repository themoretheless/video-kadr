export const EXPORT_SIZE_CONTRACT = 'size-v1' as const
export const DECIMAL_MB = 1_000_000
export type ExportRateControl =
  | { mode: 'quality'; crf: number }
  | { mode: 'target_size'; targetBytes: number; videoBitrateBps: number; audioBitrateBps: number; estimatorVersion: typeof EXPORT_SIZE_CONTRACT }

export type SizedExportFormat = 'mp4' | 'webm' | 'av1' | 'prores' | 'gif' | 'png' | 'jpg' | 'mp3'
export interface ExportSizeEstimate {
  contract: typeof EXPORT_SIZE_CONTRACT
  durationSeconds: number
  lowBytes: number
  centerBytes: number
  highBytes: number
  videoBitrateBps: number
  audioBitrateBps: number
  targetVideoBitrateKbps: number | null
  confidence: 'low' | 'medium'
  assumptions: string[]
}

export interface ExportSizeInput {
  durationSeconds: number
  width: number
  height: number
  fps: number
  format: string
  codec?: string
  crf?: number
  muted?: boolean
  targetBytes?: number | null
  browser?: boolean
}

const TARGET_FORMATS = new Set(['mp4:h264', 'mp4:h265', 'webm:vp9', 'av1:av1'])
const MIN_TARGET_BYTES = 128 * 1024
const MAX_SERVER_VIDEO_BPS = 100_000_000
const MAX_BROWSER_VIDEO_BPS = 8_000_000

const finite = (value: unknown, fallback: number) => typeof value === 'number' && Number.isFinite(value) ? value : fallback
const bytes = (bps: number, seconds: number, overhead = 1.02) => Math.max(1, Math.ceil(bps * seconds / 8 * overhead))
const safeBytes = (value: number) => Math.min(Number.MAX_SAFE_INTEGER, Math.max(1, Math.ceil(value)))

function renderContract(payload: Record<string, unknown>): Record<string, unknown> | null {
  for (const key of ['multicamFlatten', 'timelineRender']) {
    const value = payload[key]
    if (value && typeof value === 'object') return value as Record<string, unknown>
  }
  return null
}

export function finalOutputDuration(sourceDuration: number, payload: Record<string, unknown>): { selectedSeconds: number; outputSeconds: number } {
  const render = renderContract(payload)
  const timeBase = finite(render?.timeBase, 0)
  const durationTicks = finite(render?.durationTicks, 0)
  const baseDuration = timeBase > 0 && durationTicks > 0 ? durationTicks / timeBase : Math.max(0, finite(sourceDuration, 0))
  const trim = payload.trim && typeof payload.trim === 'object' ? payload.trim as Record<string, unknown> : null
  const start = Math.max(0, Math.min(baseDuration, finite(trim?.start, 0)))
  const end = Math.max(start, Math.min(baseDuration, finite(trim?.end, baseDuration)))
  const trimmed = Math.max(0.001, end - start)
  const segments = Array.isArray(payload.segments) ? payload.segments : []
  let selected = trimmed
  if (segments.length) {
    const retained = segments.reduce((sum, raw) => {
      if (!raw || typeof raw !== 'object') return sum
      const segment = raw as Record<string, unknown>
      const segmentStart = Math.max(start, finite(segment.start, start))
      const segmentEnd = Math.min(end, finite(segment.end, end))
      return sum + Math.max(0, segmentEnd - segmentStart)
    }, 0)
    if (retained > 0) selected = Math.min(trimmed, retained)
  }
  const speed = Math.max(0.5, Math.min(2, finite(payload.speed, 1)))
  return { selectedSeconds: selected, outputSeconds: selected / speed }
}

function normalizedCodec(format: string, codec?: string): string {
  if (format === 'mp4') return codec === 'h265' ? 'h265' : 'h264'
  if (format === 'webm') return 'vp9'
  if (format === 'av1') return 'av1'
  return format
}

function audioRate(format: string, muted: boolean): number {
  if (muted || ['gif', 'png', 'jpg'].includes(format)) return 0
  if (format === 'webm') return 96_000
  if (format === 'prores') return 1_536_000
  if (format === 'mp3') return 190_000
  return 128_000
}

export function solveTargetVideoBitrate(input: ExportSizeInput): number | null {
  if (input.targetBytes === null || input.targetBytes === undefined) return null
  const format = String(input.format)
  const codec = normalizedCodec(format, input.codec)
  if (!TARGET_FORMATS.has(`${format}:${codec}`)) throw new Error('target size is unsupported for this format')
  const targetBytes = finite(input.targetBytes, 0)
  const duration = finite(input.durationSeconds, 0)
  if (!Number.isSafeInteger(targetBytes) || targetBytes < MIN_TARGET_BYTES || duration <= 0) throw new Error('invalid target size or duration')
  const audio = audioRate(format, Boolean(input.muted))
  // Reserve 2% mux overhead and 3% one-pass rate-control safety before video.
  const videoBps = targetBytes * 8 * .95 / duration - audio
  const ceiling = input.browser ? MAX_BROWSER_VIDEO_BPS : MAX_SERVER_VIDEO_BPS
  if (!Number.isFinite(videoBps) || videoBps < 100_000 || videoBps > ceiling) throw new Error('target size is outside encoder bitrate bounds')
  return Math.round(videoBps / 1000)
}

export interface ExportSizingGeometry { width: number; height: number; fps: number; selectedSeconds: number; durationSeconds: number }

/** Resolve the exact output canvas and duration inputs shared by UI estimates and resource planning. */
export function resolveExportSizing(source: { duration: number; width: number; height: number; fps?: number | null }, payload: Record<string, unknown>): ExportSizingGeometry {
  const duration = finalOutputDuration(source.duration, payload)
  const render = renderContract(payload)
  const target = render?.target && typeof render.target === 'object' ? render.target as Record<string, unknown> : null
  const crop = payload.crop && typeof payload.crop === 'object' ? payload.crop as Record<string, unknown> : null
  const scale = payload.scale && typeof payload.scale === 'object' ? payload.scale as Record<string, unknown> : null
  const baseWidth = Math.max(1, finite(target?.width, finite(crop?.w, finite(source.width, 1920))))
  const baseHeight = Math.max(1, finite(target?.height, finite(crop?.h, finite(source.height, 1080))))
  const requestedWidth = finite(scale?.w, baseWidth), requestedHeight = finite(scale?.h, baseHeight)
  let width = Math.max(1, Math.round(requestedWidth > 0 ? requestedWidth : baseWidth * requestedHeight / baseHeight))
  let height = Math.max(1, Math.round(requestedHeight > 0 ? requestedHeight : baseHeight * requestedWidth / baseWidth))
  if (finite(payload.rotate, 0) === 90 || finite(payload.rotate, 0) === 270) [width, height] = [height, width]
  const pad = String(payload.pad || '')
  if (/^\d+:\d+$/.test(pad)) {
    const [rw, rh] = pad.split(':').map(Number)
    width = Math.max(width, Math.ceil(height * rw! / rh!))
    height = Math.max(height, Math.ceil(width * rh! / rw!))
  }
  const fps = Math.max(1, finite(payload.fps, finite(target?.fps, finite(source.fps, 30))))
  return { width, height, fps, selectedSeconds: duration.selectedSeconds, durationSeconds: duration.outputSeconds }
}

export function estimateExportSize(input: ExportSizeInput): ExportSizeEstimate {
  const duration = Math.max(.001, finite(input.durationSeconds, 0))
  const format = String(input.format || 'mp4') as SizedExportFormat
  const codec = normalizedCodec(format, input.codec)
  const width = Math.max(1, finite(input.width, 1920)), height = Math.max(1, finite(input.height, 1080)), fps = Math.max(1, finite(input.fps, 30))
  const audio = audioRate(format, Boolean(input.muted))
  const targetKbps = solveTargetVideoBitrate({ ...input, durationSeconds: duration, width, height, fps, format, codec })
  let video = 0, lowFactor = .5, highFactor = 1.85, confidence: 'low' | 'medium' = 'low'
  const assumptions: string[] = []
  if (targetKbps !== null) {
    video = targetKbps * 1000; lowFactor = duration < 10 ? .75 : .85; highFactor = duration < 10 ? 1.2 : 1.12; confidence = 'medium'
    assumptions.push('one-pass target bitrate; content and muxing can undershoot or overshoot')
  } else if (['h264', 'h265', 'vp9', 'av1'].includes(codec)) {
    const defaults: Record<string, number> = { h264: 23, h265: 28, vp9: 32, av1: 32 }
    const factors: Record<string, number> = { h264: 1, h265: .65, vp9: .70, av1: .50 }
    const crf = finite(input.crf, defaults[codec]!)
    video = width * height * fps * .075 * factors[codec]! * 2 ** ((defaults[codec]! - crf) / 6)
    if (input.browser) video = Math.min(video, MAX_BROWSER_VIDEO_BPS)
    if (codec === 'vp9' || codec === 'av1') { lowFactor = .45; highFactor = 2 }
    assumptions.push(`CRF ${crf}; bitrate varies with motion, detail and grain`)
  } else if (format === 'prores') {
    video = width * height * fps * 2.2; lowFactor = .8; highFactor = 1.25; confidence = 'medium'; assumptions.push('ProRes 422 HQ profile estimate')
  } else if (format === 'gif') {
    video = width * height * Math.min(fps, 12) * .35; lowFactor = .35; highFactor = 2.5; assumptions.push('palette complexity dominates GIF size')
  } else if (format === 'png' || format === 'jpg') {
    const center = width * height * (format === 'png' ? 2 : .35)
    return { contract: EXPORT_SIZE_CONTRACT, durationSeconds: duration, lowBytes: safeBytes(center * (format === 'png' ? .3 : .45)), centerBytes: safeBytes(center), highBytes: safeBytes(center * (format === 'png' ? 2.1 : 1.8)), videoBitrateBps: 0, audioBitrateBps: 0, targetVideoBitrateKbps: null, confidence: 'low', assumptions: ['single-frame image; visual complexity dominates size'] }
  } else if (format === 'mp3') {
    return { contract: EXPORT_SIZE_CONTRACT, durationSeconds: duration, lowBytes: bytes(150_000, duration), centerBytes: bytes(190_000, duration), highBytes: bytes(240_000, duration), videoBitrateBps: 0, audioBitrateBps: 190_000, targetVideoBitrateKbps: null, confidence: 'medium', assumptions: ['LAME quality 2 VBR, approximately 150–240 kbps'] }
  } else throw new Error('unsupported export format')
  return {
    contract: EXPORT_SIZE_CONTRACT, durationSeconds: duration,
    lowBytes: safeBytes(bytes(video * lowFactor + audio, duration)),
    centerBytes: safeBytes(bytes(video + audio, duration)),
    highBytes: safeBytes(bytes(video * highFactor + audio, duration)),
    videoBitrateBps: Math.round(video), audioBitrateBps: audio, targetVideoBitrateKbps: targetKbps, confidence, assumptions,
  }
}
