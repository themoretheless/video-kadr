import {
  trackedPointsToPositionKeyframes,
  type PositionKeyframeTracks,
} from './keyframes'
import type { TrackedPoint } from './tracker'

export interface StabilizationOptions {
  readonly smoothingRadiusFrames: number
  readonly strength?: number
  readonly maxCorrectionPixels?: number
  readonly keyframeTolerancePixels?: number
}

export interface StabilizationSample {
  readonly frameIndex: number
  readonly correctionX: number
  readonly correctionY: number
}

/**
 * Smooth a tracked camera trajectory with a triangular window and return the
 * translation that moves each observed point onto that smooth trajectory.
 */
export function stabilizeTrajectory(
  points: readonly TrackedPoint[],
  options: StabilizationOptions,
): StabilizationSample[] {
  validate(points, options)
  const radius = options.smoothingRadiusFrames
  const strength = options.strength ?? 1
  const maxCorrection = options.maxCorrectionPixels ?? 256
  return points.map((point, index) => {
    let totalWeight = 0
    let smoothX = 0
    let smoothY = 0
    const start = Math.max(0, index - radius)
    const end = Math.min(points.length - 1, index + radius)
    for (let sampleIndex = start; sampleIndex <= end; sampleIndex += 1) {
      const distance = Math.abs(sampleIndex - index)
      const weight = radius + 1 - distance
      const sample = points[sampleIndex]!
      totalWeight += weight
      smoothX += sample.x * weight
      smoothY += sample.y * weight
    }
    smoothX /= totalWeight
    smoothY /= totalWeight
    return {
      frameIndex: point.frameIndex,
      correctionX: clamp((smoothX - point.x) * strength, -maxCorrection, maxCorrection),
      correctionY: clamp((smoothY - point.y) * strength, -maxCorrection, maxCorrection),
    }
  })
}

/** Build composition-ready position tracks around an existing static offset. */
export function stabilizationPositionKeyframes(
  points: readonly TrackedPoint[],
  fps: number,
  baseX: number,
  baseY: number,
  options: StabilizationOptions,
  timeBase = 1_000_000,
): PositionKeyframeTracks {
  if (!Number.isFinite(baseX) || !Number.isFinite(baseY)) throw new Error('Invalid stabilization base position')
  const corrections = stabilizeTrajectory(points, options)
  const keyframeSamples: TrackedPoint[] = corrections.map((sample, index) => ({
    frameIndex: sample.frameIndex,
    x: baseX + sample.correctionX,
    y: baseY + sample.correctionY,
    confidence: points[index]!.confidence,
  }))
  return trackedPointsToPositionKeyframes(
    keyframeSamples,
    fps,
    timeBase,
    options.keyframeTolerancePixels ?? 0.75,
  )
}

/** RMS second difference; useful for fixture-based stabilization acceptance. */
export function trajectoryJitter(points: readonly { x: number; y: number }[]): number {
  if (points.length < 3) return 0
  let sumSquares = 0
  let samples = 0
  for (let index = 1; index < points.length - 1; index += 1) {
    const previous = points[index - 1]!
    const current = points[index]!
    const next = points[index + 1]!
    const dx = next.x - 2 * current.x + previous.x
    const dy = next.y - 2 * current.y + previous.y
    sumSquares += dx * dx + dy * dy
    samples += 1
  }
  return Math.sqrt(sumSquares / samples)
}

function validate(points: readonly TrackedPoint[], options: StabilizationOptions): void {
  if (!points.length) throw new Error('Stabilization trajectory is empty')
  if (
    !Number.isSafeInteger(options.smoothingRadiusFrames) ||
    options.smoothingRadiusFrames < 1 ||
    options.smoothingRadiusFrames > 300
  ) {
    throw new Error('Stabilization radius must be an integer in 1..=300')
  }
  const strength = options.strength ?? 1
  if (!Number.isFinite(strength) || strength < 0 || strength > 1) {
    throw new Error('Stabilization strength must be in 0..=1')
  }
  const maxCorrection = options.maxCorrectionPixels ?? 256
  if (!Number.isFinite(maxCorrection) || maxCorrection <= 0 || maxCorrection > 4_096) {
    throw new Error('Stabilization correction limit is invalid')
  }
  let previousFrame = -1
  for (const point of points) {
    if (
      !Number.isSafeInteger(point.frameIndex) ||
      point.frameIndex <= previousFrame ||
      !Number.isFinite(point.x) ||
      !Number.isFinite(point.y)
    ) {
      throw new Error('Stabilization trajectory contains invalid samples')
    }
    previousFrame = point.frameIndex
  }
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value))
}
