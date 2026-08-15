import { describe, expect, it } from 'vitest'
import {
  stabilizationPositionKeyframes,
  stabilizeTrajectory,
  trajectoryJitter,
} from './stabilization'
import type { TrackedPoint } from './tracker'

function trajectory(): TrackedPoint[] {
  return Array.from({ length: 30 }, (_, frameIndex) => ({
    frameIndex,
    x: 100 + frameIndex * 0.8 + (frameIndex % 2 ? 3 : -3),
    y: 50 + frameIndex * 0.3 + Math.sin(frameIndex * 1.7) * 2,
    confidence: 0.95,
  }))
}

describe('classical trajectory stabilization', () => {
  it('reduces declared second-difference jitter while retaining deliberate pan', () => {
    const points = trajectory()
    const corrections = stabilizeTrajectory(points, {
      smoothingRadiusFrames: 4,
      strength: 1,
      maxCorrectionPixels: 20,
    })
    const stabilized = points.map((point, index) => ({
      x: point.x + corrections[index]!.correctionX,
      y: point.y + corrections[index]!.correctionY,
    }))

    expect(trajectoryJitter(stabilized)).toBeLessThan(trajectoryJitter(points) * 0.25)
    expect(stabilized.at(-1)!.x - stabilized[0]!.x).toBeGreaterThan(15)
  })

  it('clamps corrections and produces bounded backend-compatible position tracks', () => {
    const points = trajectory()
    const corrections = stabilizeTrajectory(points, {
      smoothingRadiusFrames: 5,
      maxCorrectionPixels: 1,
    })
    expect(corrections.every((sample) => Math.abs(sample.correctionX) <= 1 && Math.abs(sample.correctionY) <= 1)).toBe(true)

    const tracks = stabilizationPositionKeyframes(points, 30, 10, -5, {
      smoothingRadiusFrames: 5,
      maxCorrectionPixels: 10,
      keyframeTolerancePixels: 0.2,
    })
    expect(tracks.x.keyframes.length).toBeLessThanOrEqual(32)
    expect(tracks.y.keyframes.map((keyframe) => keyframe.tick)).toEqual(
      tracks.x.keyframes.map((keyframe) => keyframe.tick),
    )
    expect(tracks.x.keyframes.every((keyframe) => Number.isFinite(keyframe.value))).toBe(true)
  })

  it('rejects unsafe settings and malformed trajectories', () => {
    expect(() => stabilizeTrajectory(trajectory(), { smoothingRadiusFrames: 0 })).toThrow('radius')
    expect(() => stabilizeTrajectory(trajectory(), {
      smoothingRadiusFrames: 2,
      strength: 2,
    })).toThrow('strength')
    const duplicate = trajectory().slice(0, 2).map((point) => ({ ...point, frameIndex: 0 }))
    expect(() => stabilizeTrajectory(duplicate, { smoothingRadiusFrames: 2 })).toThrow('invalid samples')
  })
})
