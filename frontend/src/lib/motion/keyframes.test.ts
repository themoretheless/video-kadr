import { describe, expect, it } from 'vitest'
import { trackedPointsToPositionKeyframes } from './keyframes'
import type { TrackedPoint } from './tracker'

function point(frameIndex: number, x: number, y: number): TrackedPoint {
  return { frameIndex, x, y, confidence: 1 }
}

describe('tracked position keyframe simplification', () => {
  it('collapses perfectly linear motion to matching X/Y endpoints', () => {
    const tracks = trackedPointsToPositionKeyframes([
      point(0, 10, 20),
      point(1, 11, 22),
      point(2, 12, 24),
      point(3, 13, 26),
    ], 30)

    expect(tracks).toEqual({
      x: {
        timeBase: 1_000_000,
        interpolation: 'linear',
        keyframes: [{ tick: 0, value: 10 }, { tick: 100_000, value: 13 }],
      },
      y: {
        timeBase: 1_000_000,
        interpolation: 'linear',
        keyframes: [{ tick: 0, value: 20 }, { tick: 100_000, value: 26 }],
      },
    })
  })

  it('retains a significant turn and bounds noisy tracks to the backend limit', () => {
    const turn = trackedPointsToPositionKeyframes([
      point(0, 0, 0),
      point(1, 1, 0),
      point(2, 2, 5),
      point(3, 3, 0),
      point(4, 4, 0),
    ], 10, 1_000, 0.5)
    expect(turn.x.keyframes.map((keyframe) => keyframe.tick)).toContain(200)
    expect(turn.y.keyframes.find((keyframe) => keyframe.tick === 200)?.value).toBe(5)

    const noisy = Array.from({ length: 200 }, (_, frameIndex) => (
      point(frameIndex, frameIndex + Math.sin(frameIndex) * 3, Math.cos(frameIndex * 0.7) * 4)
    ))
    const bounded = trackedPointsToPositionKeyframes(noisy, 30, 1_000_000, 0, 32)
    expect(bounded.x.keyframes).toHaveLength(32)
    expect(bounded.y.keyframes.map((keyframe) => keyframe.tick)).toEqual(
      bounded.x.keyframes.map((keyframe) => keyframe.tick),
    )
    expect(bounded.x.keyframes.at(-1)?.value).toBe(noisy.at(-1)?.x)
  })

  it('rejects duplicate frames and an unsafe output limit', () => {
    expect(() => trackedPointsToPositionKeyframes([
      point(0, 0, 0),
      point(0, 1, 1),
    ], 30)).toThrow('unordered')
    expect(() => trackedPointsToPositionKeyframes([
      point(0, 0, 0),
      point(1, 1, 1),
    ], 30, 1_000_000, 1, 33)).toThrow('2..=32')
  })
})
