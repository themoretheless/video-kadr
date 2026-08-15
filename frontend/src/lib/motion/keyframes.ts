import type { TrackedPoint } from './tracker'

export const MAX_TRACKED_KEYFRAMES = 32

export interface NumericKeyframe {
  readonly tick: number
  readonly value: number
}

export interface LinearKeyframeTrack {
  readonly timeBase: number
  readonly interpolation: 'linear'
  readonly keyframes: readonly NumericKeyframe[]
}

export interface PositionKeyframeTracks {
  readonly x: LinearKeyframeTrack
  readonly y: LinearKeyframeTrack
}

/**
 * Simplify tracking samples by maximum 2D linear-interpolation error. The same
 * retained timestamps are used for X and Y, preventing axis drift in export.
 */
export function trackedPointsToPositionKeyframes(
  points: readonly TrackedPoint[],
  fps: number,
  timeBase = 1_000_000,
  tolerancePixels = 1.5,
  maxKeyframes = MAX_TRACKED_KEYFRAMES,
): PositionKeyframeTracks {
  validateOptions(points, fps, timeBase, tolerancePixels, maxKeyframes)
  const selected = selectSamples(points, tolerancePixels, maxKeyframes)
  const toTrack = (axis: 'x' | 'y'): LinearKeyframeTrack => ({
    timeBase,
    interpolation: 'linear',
    keyframes: selected.map((point) => ({
      tick: Math.round((point.frameIndex / fps) * timeBase),
      value: point[axis],
    })),
  })
  return { x: toTrack('x'), y: toTrack('y') }
}

function selectSamples(
  points: readonly TrackedPoint[],
  tolerancePixels: number,
  maxKeyframes: number,
): TrackedPoint[] {
  if (points.length <= 2) return points.map((point) => ({ ...point }))
  const selected = new Set([0, points.length - 1])
  while (selected.size < maxKeyframes) {
    const ordered = [...selected].sort((left, right) => left - right)
    let bestIndex = -1
    let bestError = tolerancePixels
    for (let segment = 1; segment < ordered.length; segment += 1) {
      const startIndex = ordered[segment - 1]!
      const endIndex = ordered[segment]!
      const start = points[startIndex]!
      const end = points[endIndex]!
      const frameSpan = end.frameIndex - start.frameIndex
      if (frameSpan <= 1) continue
      for (let index = startIndex + 1; index < endIndex; index += 1) {
        const point = points[index]!
        const progress = (point.frameIndex - start.frameIndex) / frameSpan
        const predictedX = start.x + (end.x - start.x) * progress
        const predictedY = start.y + (end.y - start.y) * progress
        const error = Math.hypot(point.x - predictedX, point.y - predictedY)
        if (error > bestError || (error === bestError && (bestIndex < 0 || index < bestIndex))) {
          bestError = error
          bestIndex = index
        }
      }
    }
    if (bestIndex < 0) break
    selected.add(bestIndex)
  }
  return [...selected].sort((left, right) => left - right).map((index) => ({ ...points[index]! }))
}

function validateOptions(
  points: readonly TrackedPoint[],
  fps: number,
  timeBase: number,
  tolerancePixels: number,
  maxKeyframes: number,
): void {
  if (!points.length) throw new Error('Point track is empty')
  if (!Number.isFinite(fps) || fps <= 0 || fps > 1_000) throw new Error('Invalid tracking frame rate')
  if (!Number.isSafeInteger(timeBase) || timeBase <= 0) throw new Error('Invalid keyframe time base')
  if (!Number.isFinite(tolerancePixels) || tolerancePixels < 0) throw new Error('Invalid tracking tolerance')
  if (!Number.isSafeInteger(maxKeyframes) || maxKeyframes < 2 || maxKeyframes > MAX_TRACKED_KEYFRAMES) {
    throw new Error(`Tracked position supports 2..=${MAX_TRACKED_KEYFRAMES} keyframes`)
  }
  let previousFrame = -1
  for (const point of points) {
    if (
      !Number.isSafeInteger(point.frameIndex) ||
      point.frameIndex <= previousFrame ||
      !Number.isFinite(point.x) ||
      !Number.isFinite(point.y)
    ) {
      throw new Error('Point track contains invalid or unordered samples')
    }
    previousFrame = point.frameIndex
  }
}
