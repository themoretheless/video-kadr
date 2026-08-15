export const MAX_TRACK_FRAME_PIXELS = 16_777_216
export const MAX_TRACK_FRAMES = 10_000
export const MAX_TRACK_OPERATIONS = 100_000_000

export interface GrayFrame {
  readonly width: number
  readonly height: number
  readonly data: Uint8Array
}

export interface TrackPoint {
  readonly x: number
  readonly y: number
}

export interface PointTrackerOptions {
  readonly patchRadius: number
  readonly searchRadius: number
  readonly minimumConfidence?: number
}

export interface TrackedPoint extends TrackPoint {
  readonly frameIndex: number
  readonly confidence: number
}

export interface PointTrackResult {
  readonly status: 'completed' | 'lost'
  readonly points: readonly TrackedPoint[]
  readonly lostAtFrame?: number
}

/** Convert packed RGBA pixels to deterministic BT.601 luma bytes. */
export function rgbaToLuma(
  rgba: Uint8ClampedArray | Uint8Array,
  width: number,
  height: number,
): GrayFrame {
  validateDimensions(width, height)
  const pixels = width * height
  if (rgba.length !== pixels * 4) throw new Error('RGBA buffer size does not match frame dimensions')
  const data = new Uint8Array(pixels)
  for (let index = 0; index < pixels; index += 1) {
    const offset = index * 4
    data[index] = Math.round(
      0.299 * rgba[offset]! + 0.587 * rgba[offset + 1]! + 0.114 * rgba[offset + 2]!,
    )
  }
  return { width, height, data }
}

/**
 * Track a point frame-to-frame with zero-mean normalized cross-correlation.
 * Coordinates are integer pixel centres. The template is refreshed from the
 * previous frame, which supports gradual deterministic motion without models.
 */
export function trackPointSequence(
  frames: readonly GrayFrame[],
  initial: TrackPoint,
  options: PointTrackerOptions,
): PointTrackResult {
  if (!frames.length || frames.length > MAX_TRACK_FRAMES) throw new Error('Некорректное число кадров для tracking')
  const patchRadius = validateRadius(options.patchRadius, 'patch')
  const searchRadius = validateRadius(options.searchRadius, 'search')
  const minimumConfidence = options.minimumConfidence ?? 0.55
  if (!Number.isFinite(minimumConfidence) || minimumConfidence < 0 || minimumConfidence > 1) {
    throw new Error('Tracking confidence must be in 0..=1')
  }
  const { width, height } = validateFrame(frames[0]!)
  for (const frame of frames.slice(1)) {
    const dimensions = validateFrame(frame)
    if (dimensions.width !== width || dimensions.height !== height) {
      throw new Error('Tracking frames must have identical dimensions')
    }
  }
  const start = integerPoint(initial)
  ensurePatchInside(start, width, height, patchRadius)
  const patchWidth = patchRadius * 2 + 1
  const candidatesPerFrame = (searchRadius * 2 + 1) ** 2
  const operations = (frames.length - 1) * candidatesPerFrame * patchWidth * patchWidth
  if (!Number.isSafeInteger(operations) || operations > MAX_TRACK_OPERATIONS) {
    throw new Error('Tracking request exceeds the local operation budget')
  }

  const points: TrackedPoint[] = [{ ...start, frameIndex: 0, confidence: 1 }]
  let previousPoint = start
  for (let frameIndex = 1; frameIndex < frames.length; frameIndex += 1) {
    const previous = frames[frameIndex - 1]!
    const current = frames[frameIndex]!
    const template = extractPatch(previous, previousPoint, patchRadius)
    const match = matchTemplate(current, template, previousPoint, patchRadius, searchRadius)
    if (!match || match.confidence < minimumConfidence) {
      return { status: 'lost', points, lostAtFrame: frameIndex }
    }
    points.push({ ...match.point, frameIndex, confidence: match.confidence })
    previousPoint = match.point
  }
  return { status: 'completed', points }
}

interface TemplateMatch {
  readonly point: TrackPoint
  readonly confidence: number
}

function matchTemplate(
  frame: GrayFrame,
  template: Float64Array,
  origin: TrackPoint,
  patchRadius: number,
  searchRadius: number,
): TemplateMatch | null {
  const templateStats = centeredStats(template)
  if (templateStats.energy <= Number.EPSILON) return null
  const matches: Array<{ point: TrackPoint; score: number }> = []
  const minX = Math.max(patchRadius, origin.x - searchRadius)
  const maxX = Math.min(frame.width - patchRadius - 1, origin.x + searchRadius)
  const minY = Math.max(patchRadius, origin.y - searchRadius)
  const maxY = Math.min(frame.height - patchRadius - 1, origin.y + searchRadius)
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const candidate = extractPatch(frame, { x, y }, patchRadius)
      const candidateStats = centeredStats(candidate)
      if (candidateStats.energy <= Number.EPSILON) continue
      let dot = 0
      for (let index = 0; index < template.length; index += 1) {
        dot += (template[index]! - templateStats.mean) * (candidate[index]! - candidateStats.mean)
      }
      matches.push({
        point: { x, y },
        score: dot / Math.sqrt(templateStats.energy * candidateStats.energy),
      })
    }
  }
  if (!matches.length) return null
  matches.sort(
    (left, right) =>
      right.score - left.score ||
      squaredDistance(left.point, origin) - squaredDistance(right.point, origin) ||
      left.point.y - right.point.y ||
      left.point.x - right.point.x,
  )
  const best = matches[0]!
  if (best.score <= 0) return null
  const competitor = matches.find((match) => squaredDistance(match.point, best.point) > 2)
  const separation = Math.max(0, best.score - (competitor?.score ?? 0))
  const confidence = clampUnit(best.score * 0.75 + Math.min(1, separation * 3) * 0.25)
  return { point: best.point, confidence }
}

function extractPatch(frame: GrayFrame, point: TrackPoint, radius: number): Float64Array {
  const width = radius * 2 + 1
  const patch = new Float64Array(width * width)
  let output = 0
  for (let y = point.y - radius; y <= point.y + radius; y += 1) {
    for (let x = point.x - radius; x <= point.x + radius; x += 1) {
      patch[output] = frame.data[y * frame.width + x]!
      output += 1
    }
  }
  return patch
}

function centeredStats(values: Float64Array): { mean: number; energy: number } {
  let mean = 0
  for (const value of values) mean += value
  mean /= values.length
  let energy = 0
  for (const value of values) energy += (value - mean) ** 2
  return { mean, energy }
}

function validateFrame(frame: GrayFrame): { width: number; height: number } {
  validateDimensions(frame.width, frame.height)
  if (!(frame.data instanceof Uint8Array) || frame.data.length !== frame.width * frame.height) {
    throw new Error('Grayscale buffer size does not match frame dimensions')
  }
  return { width: frame.width, height: frame.height }
}

function validateDimensions(width: number, height: number): void {
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width <= 0 || height <= 0) {
    throw new Error('Frame dimensions must be positive integers')
  }
  const pixels = width * height
  if (!Number.isSafeInteger(pixels) || pixels > MAX_TRACK_FRAME_PIXELS) {
    throw new Error('Frame exceeds the local tracking pixel budget')
  }
}

function validateRadius(value: number, label: string): number {
  if (!Number.isSafeInteger(value) || value < 1 || value > 128) {
    throw new Error(`${label} radius must be an integer in 1..=128`)
  }
  return value
}

function integerPoint(point: TrackPoint): TrackPoint {
  if (!Number.isSafeInteger(point.x) || !Number.isSafeInteger(point.y)) {
    throw new Error('Tracking point must use integer pixel coordinates')
  }
  return { x: point.x, y: point.y }
}

function ensurePatchInside(point: TrackPoint, width: number, height: number, radius: number): void {
  if (
    point.x - radius < 0 ||
    point.y - radius < 0 ||
    point.x + radius >= width ||
    point.y + radius >= height
  ) {
    throw new Error('Tracking patch lies outside the frame')
  }
}

function squaredDistance(left: TrackPoint, right: TrackPoint): number {
  return (left.x - right.x) ** 2 + (left.y - right.y) ** 2
}

function clampUnit(value: number): number {
  return Math.max(0, Math.min(1, value))
}
