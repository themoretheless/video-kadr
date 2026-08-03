import type { VideoScopeKind, VideoScopesResult } from '../domain/video-scopes'

export type VideoScopesPresentationStatusKind =
  | 'idle'
  | 'analyzing'
  | 'live'
  | 'exact'
  | 'last-exact'
  | 'unavailable'
  | 'error'

export interface VideoScopesPresentationStatus {
  kind: VideoScopesPresentationStatusKind
  detail?: string
  mediaTime?: number
  sourceMode?: 'original' | 'proxy'
}

export interface VideoScopeSummaryRow {
  channel: string
  minimum: string
  median: string
  maximum: string
}

const EMPTY_ROW: VideoScopeSummaryRow = {
  channel: '—',
  minimum: '—',
  median: '—',
  maximum: '—',
}

function planeStats(values: ArrayLike<number>): Omit<VideoScopeSummaryRow, 'channel'> {
  let total = 0
  let minimum = -1
  let maximum = -1
  for (let index = 0; index < values.length; index++) {
    const count = values[index] ?? 0
    if (count <= 0) continue
    total += count
    if (minimum < 0) minimum = index
    maximum = index
  }
  if (!total || minimum < 0) return { minimum: '—', median: '—', maximum: '—' }
  const midpoint = total / 2
  let cumulative = 0
  let median = minimum
  for (let index = minimum; index <= maximum; index++) {
    cumulative += values[index] ?? 0
    if (cumulative >= midpoint) {
      median = index
      break
    }
  }
  return {
    minimum: String(minimum),
    median: String(median),
    maximum: String(maximum),
  }
}

function histogramRows(result: VideoScopesResult): VideoScopeSummaryRow[] {
  const histogram = result.histogram
  if (!histogram) return [{ ...EMPTY_ROW }]
  return ['Y′', 'R', 'G', 'B'].map((channel, plane) => ({
    channel,
    ...planeStats(histogram.subarray(plane * 256, (plane + 1) * 256)),
  }))
}

function waveformRows(result: VideoScopesResult): VideoScopeSummaryRow[] {
  const waveform = result.waveform
  if (!waveform) return [{ ...EMPTY_ROW }]
  const traceWidth = waveform.length / 256
  if (!Number.isInteger(traceWidth)) return [{ ...EMPTY_ROW }]
  const luma = new Uint32Array(256)
  for (let x = 0; x < traceWidth; x++) {
    for (let row = 0; row < 256; row++) luma[255 - row]! += waveform[x * 256 + row] ?? 0
  }
  return [{ channel: 'Y′', ...planeStats(luma) }]
}

function paradeRows(result: VideoScopesResult): VideoScopeSummaryRow[] {
  const parade = result.parade
  if (!parade) return [{ ...EMPTY_ROW }]
  const planeLength = parade.length / 3
  const traceWidth = planeLength / 256
  if (!Number.isInteger(planeLength) || !Number.isInteger(traceWidth)) return [{ ...EMPTY_ROW }]
  return ['R', 'G', 'B'].map((channel, plane) => {
    const values = new Uint32Array(256)
    const offset = plane * planeLength
    for (let x = 0; x < traceWidth; x++) {
      for (let row = 0; row < 256; row++) values[255 - row]! += parade[offset + x * 256 + row] ?? 0
    }
    return { channel, ...planeStats(values) }
  })
}

function vectorscopeRows(result: VideoScopesResult): VideoScopeSummaryRow[] {
  const vectorscope = result.vectorscope
  if (!vectorscope) return [{ ...EMPTY_ROW }]
  let peak = 0
  let peakIndex = 0
  for (let index = 0; index < vectorscope.length; index++) {
    if (vectorscope[index]! > peak) {
      peak = vectorscope[index]!
      peakIndex = index
    }
  }
  if (!peak) return [{ ...EMPTY_ROW }]
  const x = peakIndex % 256
  const y = Math.floor(peakIndex / 256)
  const cr = x / 255 - 0.5
  const cb = 0.5 - y / 255
  const chroma = Math.min(100, Math.hypot(cb, cr) * Math.SQRT2 * 100)
  const hue = (Math.atan2(cr, cb) * 180 / Math.PI + 360) % 360
  return [{
    channel: 'Cb/Cr peak',
    minimum: `${Math.round(hue)}°`,
    median: `${chroma.toFixed(1)}%`,
    maximum: peak.toLocaleString(),
  }]
}

export function videoScopeSummaryRows(
  scope: VideoScopeKind,
  result: VideoScopesResult | null | undefined,
): VideoScopeSummaryRow[] {
  if (!result) return [{ ...EMPTY_ROW }]
  switch (scope) {
    case 'histogram': return histogramRows(result)
    case 'waveform': return waveformRows(result)
    case 'parade': return paradeRows(result)
    case 'vectorscope': return vectorscopeRows(result)
  }
}

export const VIDEO_SCOPE_LABELS: Record<VideoScopeKind, string> = {
  histogram: 'Гистограмма',
  waveform: 'Waveform',
  parade: 'RGB Parade',
  vectorscope: 'Vectorscope',
}

export const VIDEO_SCOPE_SUMMARY_COLUMNS: Record<VideoScopeKind, [string, string, string]> = {
  histogram: ['Мин.', 'Медиана', 'Макс.'],
  waveform: ['Мин.', 'Медиана', 'Макс.'],
  parade: ['Мин.', 'Медиана', 'Макс.'],
  vectorscope: ['Тон', 'Хрома', 'Плотность'],
}

export const VIDEO_SCOPES_STATUS_LABELS: Record<VideoScopesPresentationStatusKind, string> = {
  idle: 'Ожидание кадра',
  analyzing: 'Обновление',
  live: 'Live · sampled',
  exact: 'Пауза · exact',
  'last-exact': 'Последний exact',
  unavailable: 'Недоступно',
  error: 'Ошибка',
}
