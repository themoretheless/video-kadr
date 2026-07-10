import type { EditState, VideoInfo } from '../types'

export const EDIT_DEFAULTS = {
  trimStart: 0,
  trimEnd: 0,
  cutEnabled: false,
  cut: { start: 0, end: 0 },
  cropEnabled: false,
  crop: { x: 0, y: 0, w: 0, h: 0 },
  scaleEnabled: false,
  scale: { w: 1280, h: -2 },
  mute: false,
  speed: 1,
  rotate: 0,
  flipH: false,
  flipV: false,
  volume: 1,
  fadeIn: 0,
  fadeOut: 0,
  normalizeAudio: false,
  highpass: false,
  brightness: 0,
  contrast: 1,
  saturation: 1,
  filter: '',
  reverse: false,
  fps: null,
  censorEnabled: false,
  censor: { x: 0, y: 0, w: 0, h: 0 },
  censorColor: 'black',
  vignette: false,
  denoise: false,
  sharpen: 0,
  grain: 0,
  pad: '',
  format: 'mp4',
  codec: 'h264',
  qualityTier: '',
} satisfies EditState

export function defaultEdit(): EditState {
  return {
    ...EDIT_DEFAULTS,
    cut: { ...EDIT_DEFAULTS.cut },
    crop: { ...EDIT_DEFAULTS.crop },
    scale: { ...EDIT_DEFAULTS.scale },
    censor: { ...EDIT_DEFAULTS.censor },
  }
}

/** Map a quality tier to a CRF value appropriate for the target format. */
export function tierToCrf(tier: string, format: string): number | null {
  if (!tier) return null
  const table: Record<string, Record<string, number>> = {
    mp4: { high: 18, medium: 23, compact: 28 },
    webm: { high: 28, medium: 33, compact: 38 },
    av1: { high: 28, medium: 34, compact: 40 },
  }
  return table[format]?.[tier] ?? null
}

/** Parse `ss`, `mm:ss`, or `hh:mm:ss` (with optional fractional seconds). */
export function parseTime(input: string): number | null {
  const value = input.trim()
  if (!value) return null
  const parts = value.split(':').map((part) => part.trim())
  if (parts.some((part) => part === '' || !/^\d+(\.\d+)?$/.test(part))) return null
  let seconds = 0
  for (const part of parts) seconds = seconds * 60 + Number(part)
  return seconds
}

export function buildEditPayload(
  edit: EditState,
  video: VideoInfo | null,
): Record<string, unknown> {
  if (!video) return {}

  const payload: Record<string, unknown> = {
    videoId: video.id,
    mute: edit.mute,
    speed: edit.speed,
  }
  const videoFormat = edit.format === 'mp4' || edit.format === 'webm'
  const cutStart = Math.max(edit.trimStart, Math.min(edit.cut.start, edit.trimEnd))
  const cutEnd = Math.max(edit.trimStart, Math.min(edit.cut.end, edit.trimEnd))
  const segments: { start: number; end: number }[] = []
  if (edit.cutEnabled && videoFormat && cutEnd > cutStart + 0.05) {
    if (cutStart > edit.trimStart + 0.05) {
      segments.push({ start: edit.trimStart, end: cutStart })
    }
    if (edit.trimEnd > cutEnd + 0.05) {
      segments.push({ start: cutEnd, end: edit.trimEnd })
    }
  }
  if (segments.length) {
    payload.segments = segments
  } else if (edit.trimStart > 0.05 || edit.trimEnd < video.duration - 0.05) {
    payload.trim = { start: edit.trimStart, end: edit.trimEnd }
  }
  if (edit.cropEnabled) payload.crop = sanitizeRect(edit.crop, video.width, video.height)
  if (edit.scaleEnabled) payload.scale = { w: edit.scale.w, h: edit.scale.h }
  if (edit.rotate) payload.rotate = edit.rotate
  if (edit.flipH) payload.flipH = true
  if (edit.flipV) payload.flipV = true
  if (edit.volume !== EDIT_DEFAULTS.volume) payload.volume = edit.volume
  if (edit.fadeIn > 0) payload.fadeIn = edit.fadeIn
  if (edit.fadeOut > 0) payload.fadeOut = edit.fadeOut
  if (edit.normalizeAudio) payload.normalizeAudio = true
  if (edit.highpass) payload.highpass = true
  if (edit.brightness !== EDIT_DEFAULTS.brightness) payload.brightness = edit.brightness
  if (edit.contrast !== EDIT_DEFAULTS.contrast) payload.contrast = edit.contrast
  if (edit.saturation !== EDIT_DEFAULTS.saturation) payload.saturation = edit.saturation
  if (edit.filter) payload.filter = edit.filter
  if (edit.reverse) payload.reverse = true
  if (edit.fps) payload.fps = edit.fps
  if (edit.censorEnabled && edit.censor.w > 1 && edit.censor.h > 1) {
    payload.censor = { ...edit.censor }
    payload.censorColor = edit.censorColor
  }
  if (edit.vignette) payload.vignette = true
  if (edit.denoise) payload.denoise = true
  if (edit.sharpen > 0) payload.sharpen = edit.sharpen
  if (edit.grain > 0) payload.grain = edit.grain
  if (edit.pad) payload.pad = edit.pad
  if (edit.format && edit.format !== EDIT_DEFAULTS.format) payload.format = edit.format
  if (edit.format === 'mp4' && edit.codec === 'h265') payload.codec = 'h265'
  const crf = tierToCrf(edit.qualityTier, edit.format)
  if (crf !== null) payload.quality = crf
  return payload
}

/** True when export would alter media or non-default output settings. */
export function hasMeaningfulChanges(edit: EditState, video: VideoInfo | null): boolean {
  const payload = buildEditPayload(edit, video)
  return Object.entries(payload).some(([key, value]) => {
    if (key === 'videoId') return false
    if (key === 'mute') return value !== false
    if (key === 'speed') return value !== 1
    return true
  })
}

function finiteOr(value: unknown, fallback: number): number {
  return typeof value === 'number' && Number.isFinite(value) ? value : fallback
}

function clampInt(value: unknown, min: number, max: number, fallback: number): number {
  const rounded = Math.round(finiteOr(value, fallback))
  return Math.max(min, Math.min(rounded, max))
}

export function sanitizeRect(
  rect: { x: number; y: number; w: number; h: number },
  width: number,
  height: number,
): { x: number; y: number; w: number; h: number } {
  const safeWidth = Math.max(1, Math.round(finiteOr(width, 1)))
  const safeHeight = Math.max(1, Math.round(finiteOr(height, 1)))
  const minWidth = Math.min(2, safeWidth)
  const minHeight = Math.min(2, safeHeight)
  const rectWidth = clampInt(rect.w, minWidth, safeWidth, safeWidth)
  const rectHeight = clampInt(rect.h, minHeight, safeHeight, safeHeight)
  return {
    x: clampInt(rect.x, 0, safeWidth - rectWidth, 0),
    y: clampInt(rect.y, 0, safeHeight - rectHeight, 0),
    w: rectWidth,
    h: rectHeight,
  }
}
