import { describe, expect, it } from 'vitest'
import type { CompositionSpeedRamp } from './types'
import {
  compositionSpeedRampSegments,
  sliceCompositionSpeedRamp,
  speedAtSourceProgress,
  speedRampSourceProgressAtTimelineTick,
  speedRampTimelineTickAtSourceProgress,
  speedRampTimelineDurationTicks,
} from './speedRamp'

const second = 1_000_000

describe('composition speed ramp timing', () => {
  it('matches backend hold and reciprocal-linear integral rounding exactly', () => {
    const hold: CompositionSpeedRamp = {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: second, speed: 2 },
        { sourceProgressTick: 2 * second, speed: 4 },
      ],
      audioPolicy: 'mute',
    }
    expect(compositionSpeedRampSegments(2 * second, 1, hold)).toEqual([
      {
        sourceStartTick: 0,
        sourceEndTick: second,
        timelineStartTick: 0,
        timelineEndTick: second,
        startSpeed: 1,
        endSpeed: 2,
        interpolation: 'hold',
      },
      {
        sourceStartTick: second,
        sourceEndTick: 2 * second,
        timelineStartTick: second,
        timelineEndTick: 1_500_000,
        startSpeed: 2,
        endSpeed: 4,
        interpolation: 'hold',
      },
    ])

    const linear: CompositionSpeedRamp = {
      interpolation: 'linear',
      points: [
        { sourceProgressTick: 0, speed: 0.5 },
        { sourceProgressTick: 2 * second, speed: 2 },
      ],
    }
    expect(speedRampTimelineDurationTicks(2 * second, 0.5, linear)).toBe(1_848_392)
  })

  it('inverts rounded backend timeline ticks into source progress at exact knots', () => {
    const ramp: CompositionSpeedRamp = {
      interpolation: 'linear',
      points: [
        { sourceProgressTick: 0, speed: 0.5 },
        { sourceProgressTick: second, speed: 1 },
        { sourceProgressTick: 2 * second, speed: 2 },
      ],
      audioPolicy: 'preserve_pitch',
    }
    expect(speedRampSourceProgressAtTimelineTick(2 * second, 0.5, ramp, 0)).toBe(0)
    expect(speedRampSourceProgressAtTimelineTick(2 * second, 0.5, ramp, 1_386_294)).toBe(second)
    expect(speedRampSourceProgressAtTimelineTick(2 * second, 0.5, ramp, 2_079_442)).toBe(2 * second)
    expect(speedRampTimelineTickAtSourceProgress(2 * second, 0.5, ramp, 0)).toBe(0)
    expect(speedRampTimelineTickAtSourceProgress(2 * second, 0.5, ramp, second)).toBe(1_386_294)
    expect(speedRampTimelineTickAtSourceProgress(2 * second, 0.5, ramp, 2 * second)).toBe(2_079_442)
  })

  it('interpolates trim boundaries and rebases a sliced curve', () => {
    const ramp: CompositionSpeedRamp = {
      interpolation: 'linear',
      points: [
        { sourceProgressTick: 0, speed: 0.5 },
        { sourceProgressTick: second, speed: 1.25 },
        { sourceProgressTick: 2 * second, speed: 2 },
      ],
    }
    expect(speedAtSourceProgress(2 * second, 0.5, ramp, 500_000)).toBe(0.875)
    const sliced = sliceCompositionSpeedRamp(2 * second, 0.5, ramp, 500_000, 1_500_000)
    expect(sliced).toEqual({
      speed: 0.875,
      speedRamp: {
        interpolation: 'linear',
        points: [
          { sourceProgressTick: 0, speed: 0.875 },
          { sourceProgressTick: second, speed: 1.625 },
        ],
        audioPolicy: 'preserve_pitch',
      },
    })
  })

  it('rejects malformed boundaries and segments below backend budgets', () => {
    expect(() => speedRampTimelineDurationTicks(second, 1, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 2 },
        { sourceProgressTick: second, speed: 2 },
      ],
    })).toThrow('clip.speed')
    expect(() => speedRampTimelineDurationTicks(second, 1, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 1 },
        { sourceProgressTick: 999, speed: 2 },
        { sourceProgressTick: second, speed: 2 },
      ],
    })).toThrow('не короче')
  })

  it('rounds cumulative segment ends instead of each segment independently', () => {
    const segments = compositionSpeedRampSegments(8_000, 3, {
      interpolation: 'hold',
      points: [
        { sourceProgressTick: 0, speed: 3 },
        { sourceProgressTick: 4_000, speed: 3 },
        { sourceProgressTick: 8_000, speed: 3 },
      ],
    })
    expect(segments.map((segment) => segment.timelineEndTick)).toEqual([1_333, 2_667])
    expect(segments.map((segment) => segment.timelineEndTick - segment.timelineStartTick)).toEqual([1_333, 1_334])
  })
})
