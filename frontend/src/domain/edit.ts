import type { ColorCurves, CurvePoint, EditState, VideoInfo } from '../types'

const CURVE_MIN = 0
const CURVE_MAX = 255
export const MAX_CURVE_POINTS = 16
const SAFE_LUT_ID =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

export function identityCurve(): CurvePoint[] {
  return [
    { x: CURVE_MIN, y: CURVE_MIN },
    { x: CURVE_MAX, y: CURVE_MAX },
  ]
}

export function identityCurves(): ColorCurves {
  return {
    master: identityCurve(),
    red: identityCurve(),
    green: identityCurve(),
    blue: identityCurve(),
  }
}

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
  lutId: null,
  lutName: '',
  lutSize: null,
  lutIntensity: 1,
  curves: identityCurves(),
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

const SEGMENT_OUTPUT_FORMATS = new Set(['mp4', 'webm', 'av1', 'prores'])

export function defaultEdit(): EditState {
  return {
    ...EDIT_DEFAULTS,
    cut: { ...EDIT_DEFAULTS.cut },
    crop: { ...EDIT_DEFAULTS.crop },
    scale: { ...EDIT_DEFAULTS.scale },
    censor: { ...EDIT_DEFAULTS.censor },
    curves: cloneCurves(EDIT_DEFAULTS.curves),
  }
}

export function cloneCurves(curves: ColorCurves): ColorCurves {
  return {
    master: curves.master.map((point) => ({ ...point })),
    red: curves.red.map((point) => ({ ...point })),
    green: curves.green.map((point) => ({ ...point })),
    blue: curves.blue.map((point) => ({ ...point })),
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

/**
 * Build the `/api/edit` body. `modulePayload` is the merged contribution of the
 * feature store modules; it is passed in rather than imported so this file stays
 * framework- and transport-independent. Every module returns nothing while it
 * sits at its defaults, so an untouched project still produces exactly the
 * payload this app has always sent.
 */
export function buildEditPayload(
  edit: EditState,
  video: VideoInfo | null,
  modulePayload: Record<string, unknown> = {},
): Record<string, unknown> {
  if (!video) return {}

  const payload: Record<string, unknown> = {
    videoId: video.id,
    mute: edit.mute,
    speed: edit.speed,
  }
  const supportsSegments = SEGMENT_OUTPUT_FORMATS.has(edit.format)
  const cutStart = Math.max(edit.trimStart, Math.min(edit.cut.start, edit.trimEnd))
  const cutEnd = Math.max(edit.trimStart, Math.min(edit.cut.end, edit.trimEnd))
  const segments: { start: number; end: number }[] = []
  if (edit.cutEnabled && supportsSegments && cutEnd > cutStart + 0.05) {
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
  const lutId = sanitizeLutId(edit.lutId)
  const lutIntensity = sanitizeLutIntensity(edit.lutIntensity)
  if (lutId && lutIntensity > 0) payload.lut = { id: lutId, intensity: lutIntensity }
  const curves = sanitizeCurves(edit.curves)
  if (!isIdentityCurves(curves)) payload.curves = serializeCurves(curves)
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
  Object.assign(payload, modulePayload)
  return payload
}

/** True when export would alter media or non-default output settings. */
export function hasMeaningfulChanges(
  edit: EditState,
  video: VideoInfo | null,
  modulePayload: Record<string, unknown> = {},
): boolean {
  if (Object.keys(modulePayload).length > 0) return true
  const payload = buildEditPayload(edit, video, modulePayload)
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

export function sanitizeLutId(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const id = value.trim()
  return SAFE_LUT_ID.test(id) ? id : null
}

export function sanitizeLutIntensity(value: unknown): number {
  return Math.max(0, Math.min(1, finiteOr(value, EDIT_DEFAULTS.lutIntensity)))
}

/**
 * Normalize an untrusted curve into a canonical, deterministic representation.
 * Coordinates are rounded/clamped, duplicate X values use the last value, and
 * the two required endpoints are added when absent.
 */
export function sanitizeCurve(value: unknown): CurvePoint[] {
  if (!Array.isArray(value)) return identityCurve()
  const pointsByX = new Map<number, CurvePoint>()
  for (const candidate of value) {
    if (!isRecord(candidate)) continue
    if (
      typeof candidate.x !== 'number' ||
      !Number.isFinite(candidate.x) ||
      typeof candidate.y !== 'number' ||
      !Number.isFinite(candidate.y)
    ) {
      continue
    }
    const x = clampInt(candidate.x, CURVE_MIN, CURVE_MAX, CURVE_MIN)
    const y = clampInt(candidate.y, CURVE_MIN, CURVE_MAX, x)
    pointsByX.set(x, { x, y })
  }
  if (!pointsByX.has(CURVE_MIN)) pointsByX.set(CURVE_MIN, { x: CURVE_MIN, y: CURVE_MIN })
  if (!pointsByX.has(CURVE_MAX)) pointsByX.set(CURVE_MAX, { x: CURVE_MAX, y: CURVE_MAX })
  const points = [...pointsByX.values()].sort((a, b) => a.x - b.x)
  return points.length <= MAX_CURVE_POINTS ? points : identityCurve()
}

/**
 * Sample the same shape-preserving cubic interpolation requested from FFmpeg
 * (`curves=interp=pchip`) so the editor graph matches the exported result.
 */
export function sampleCurvePchip(value: unknown): CurvePoint[] {
  const points = sanitizeCurve(value)
  const last = points.length - 1
  const widths = Array.from({ length: last }, (_, index) => points[index + 1]!.x - points[index]!.x)
  const slopes = widths.map(
    (width, index) => (points[index + 1]!.y - points[index]!.y) / width,
  )
  const tangents = new Array<number>(points.length).fill(0)

  if (points.length === 2) {
    tangents[0] = slopes[0]!
    tangents[1] = slopes[0]!
  } else {
    tangents[0] = endpointPchipSlope(widths[0]!, widths[1]!, slopes[0]!, slopes[1]!)
    tangents[last] = endpointPchipSlope(
      widths[last - 1]!,
      widths[last - 2]!,
      slopes[last - 1]!,
      slopes[last - 2]!,
    )
    for (let index = 1; index < last; index += 1) {
      const before = slopes[index - 1]!
      const after = slopes[index]!
      if (before === 0 || after === 0 || Math.sign(before) !== Math.sign(after)) {
        tangents[index] = 0
        continue
      }
      const beforeWidth = widths[index - 1]!
      const afterWidth = widths[index]!
      const weightBefore = 2 * afterWidth + beforeWidth
      const weightAfter = afterWidth + 2 * beforeWidth
      tangents[index] =
        (weightBefore + weightAfter) / (weightBefore / before + weightAfter / after)
    }
  }

  const sampled: CurvePoint[] = []
  let segment = 0
  for (let x = CURVE_MIN; x <= CURVE_MAX; x += 1) {
    while (segment < last - 1 && x > points[segment + 1]!.x) segment += 1
    const left = points[segment]!
    const right = points[segment + 1]!
    const width = right.x - left.x
    const t = (x - left.x) / width
    const t2 = t * t
    const t3 = t2 * t
    const y =
      (2 * t3 - 3 * t2 + 1) * left.y +
      (t3 - 2 * t2 + t) * width * tangents[segment]! +
      (-2 * t3 + 3 * t2) * right.y +
      (t3 - t2) * width * tangents[segment + 1]!
    sampled.push({ x, y: Math.max(CURVE_MIN, Math.min(CURVE_MAX, y)) })
  }
  return sampled
}

function endpointPchipSlope(
  edgeWidth: number,
  adjacentWidth: number,
  edgeSlope: number,
  adjacentSlope: number,
): number {
  let tangent =
    ((2 * edgeWidth + adjacentWidth) * edgeSlope - edgeWidth * adjacentSlope) /
    (edgeWidth + adjacentWidth)
  if (Math.sign(tangent) !== Math.sign(edgeSlope)) return 0
  if (Math.sign(edgeSlope) !== Math.sign(adjacentSlope) && Math.abs(tangent) > 3 * Math.abs(edgeSlope)) {
    tangent = 3 * edgeSlope
  }
  return tangent
}

export function sanitizeCurves(value: unknown): ColorCurves {
  const source = isRecord(value) ? value : {}
  return {
    master: sanitizeCurve(source.master),
    red: sanitizeCurve(source.red),
    green: sanitizeCurve(source.green),
    blue: sanitizeCurve(source.blue),
  }
}

export function isIdentityCurve(curve: CurvePoint[]): boolean {
  return (
    curve.length === 2 &&
    curve[0]?.x === CURVE_MIN &&
    curve[0]?.y === CURVE_MIN &&
    curve[1]?.x === CURVE_MAX &&
    curve[1]?.y === CURVE_MAX
  )
}

export function isIdentityCurves(curves: ColorCurves): boolean {
  return (
    isIdentityCurve(curves.master) &&
    isIdentityCurve(curves.red) &&
    isIdentityCurve(curves.green) &&
    isIdentityCurve(curves.blue)
  )
}

/** Convert integer UI code values into the backend's normalized 0..1 wire format. */
function serializeCurves(curves: ColorCurves): Record<keyof ColorCurves, CurvePoint[]> {
  const serialize = (points: CurvePoint[]): CurvePoint[] =>
    points.map(({ x, y }) => ({ x: x / CURVE_MAX, y: y / CURVE_MAX }))
  return {
    master: serialize(curves.master),
    red: serialize(curves.red),
    green: serialize(curves.green),
    blue: serialize(curves.blue),
  }
}

/** Merge persisted/untrusted edit JSON over a known-safe clip-specific base. */
export function sanitizeEditState(value: unknown, base: EditState = defaultEdit()): EditState {
  const source = isRecord(value) ? value : {}
  const merged = { ...base, ...source } as EditState
  const lutId = sanitizeLutId(source.lutId ?? base.lutId)
  const inheritsLutMetadata = lutId !== null && lutId === base.lutId
  const rawLutName = Object.hasOwn(source, 'lutName')
    ? source.lutName
    : inheritsLutMetadata
      ? base.lutName
      : ''
  const rawLutSize = Object.hasOwn(source, 'lutSize')
    ? source.lutSize
    : inheritsLutMetadata
      ? base.lutSize
      : null
  const lutSize =
    typeof rawLutSize === 'number' && Number.isFinite(rawLutSize) && rawLutSize >= 2
      ? Math.min(65, Math.round(rawLutSize))
      : null
  return {
    ...merged,
    cut: isRecord(source.cut) ? { ...base.cut, ...source.cut } as EditState['cut'] : { ...base.cut },
    crop: isRecord(source.crop)
      ? { ...base.crop, ...source.crop } as EditState['crop']
      : { ...base.crop },
    scale: isRecord(source.scale)
      ? { ...base.scale, ...source.scale } as EditState['scale']
      : { ...base.scale },
    censor: isRecord(source.censor)
      ? { ...base.censor, ...source.censor } as EditState['censor']
      : { ...base.censor },
    lutId,
    lutName: lutId && typeof rawLutName === 'string' ? rawLutName.trim() : '',
    lutSize: lutId ? lutSize : null,
    lutIntensity: sanitizeLutIntensity(source.lutIntensity ?? base.lutIntensity),
    curves: sanitizeCurves(source.curves ?? base.curves),
  }
}

/** Reset every reusable colour adjustment, including LUT and custom curves. */
export function resetColorAdjustments(edit: EditState): void {
  edit.brightness = EDIT_DEFAULTS.brightness
  edit.contrast = EDIT_DEFAULTS.contrast
  edit.saturation = EDIT_DEFAULTS.saturation
  edit.filter = EDIT_DEFAULTS.filter
  edit.lutId = EDIT_DEFAULTS.lutId
  edit.lutName = EDIT_DEFAULTS.lutName
  edit.lutSize = EDIT_DEFAULTS.lutSize
  edit.lutIntensity = EDIT_DEFAULTS.lutIntensity
  edit.curves = identityCurves()
  edit.vignette = EDIT_DEFAULTS.vignette
  edit.denoise = EDIT_DEFAULTS.denoise
  edit.sharpen = EDIT_DEFAULTS.sharpen
  edit.grain = EDIT_DEFAULTS.grain
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
