import { describe, expect, it } from 'vitest'
import {
  audioPropertyValue,
  cloneAudioAnimation,
  deleteKeyframe,
  sampleAudioProperty,
  sampleAnimatableValue,
  sliceAudioAnimation,
  sliceAnimatableValue,
  shiftAnimatableValue,
  splitAnimatableValue,
  updateKeyframe,
  upsertKeyframe,
} from './keyframes'
import { COMPOSITION_TIME_BASE, MAX_KEYFRAMES_PER_VALUE, type AudioClip, type CompositionAnimatableValue } from './types'

const track = (interpolation: Extract<CompositionAnimatableValue, { mode: 'keyframes' }>['track']['interpolation']): CompositionAnimatableValue => ({
  mode: 'keyframes',
  track: {
    timeBase: COMPOSITION_TIME_BASE,
    interpolation,
    keyframes: [{ tick: 0, value: 0 }, { tick: COMPOSITION_TIME_BASE, value: 1 }],
  },
})

describe('composition keyframe math', () => {
  it('samples WeftCut Bounce directions, outgoing overrides and flat Continue tails', () => {
    for (const [mode, expected] of [['bounce_out', 0.765625], ['bounce_in', 0.234375], ['bounce_in_out', 0.5]] as const) {
      const value = track(mode)
      expect(sampleAnimatableValue(value, 500000)).toBe(expected)
      expect(sampleAnimatableValue(value, 0)).toBe(0)
      expect(sampleAnimatableValue(value, 1000000)).toBe(1)
      if (value.mode !== 'keyframes') throw new Error('Expected track')
      const continued = { ...value, track: { ...value.track, extrapolation: { before: 'continue', after: 'continue' } as const } }
      expect(sampleAnimatableValue(continued, -1000000)).toBe(0)
      expect(sampleAnimatableValue(continued, 2000000)).toBe(1)
    }
    expect(sampleAnimatableValue(track('bounce_out'), 250000)).toBe(0.47265625)
    expect(sampleAnimatableValue(track('bounce_out'), 750000)).toBe(0.97265625)
    expect(sampleAnimatableValue(track('bounce_in_out'), 250000)).toBe(0.1171875)
    expect(sampleAnimatableValue(track('bounce_in_out'), 750000)).toBe(0.8828125)
    const value = track('linear')
    if (value.mode !== 'keyframes') throw new Error('Expected track')
    const overridden = { ...value, track: { ...value.track, keyframes: [{ ...value.track.keyframes[0], interpolation: 'bounce_out' as const }, value.track.keyframes[1]] } }
    expect(sampleAnimatableValue(overridden, 500000)).toBe(0.765625)
  })

  it('samples every supported interpolation and clamps outside the authored interval', () => {
    expect(sampleAnimatableValue(track('hold'), COMPOSITION_TIME_BASE / 2)).toBe(0)
    expect(sampleAnimatableValue(track('linear'), COMPOSITION_TIME_BASE / 2)).toBe(0.5)
    expect(sampleAnimatableValue(track('ease_in'), COMPOSITION_TIME_BASE / 2)).toBe(0.125)
    expect(sampleAnimatableValue(track('ease_out'), COMPOSITION_TIME_BASE / 2)).toBe(0.875)
    expect(sampleAnimatableValue(track('ease_in_out'), COMPOSITION_TIME_BASE / 2)).toBe(0.5)
    expect(sampleAnimatableValue(track('ease_in_out_cubic'), COMPOSITION_TIME_BASE / 2)).toBe(0.5)
    expect(sampleAnimatableValue(track('linear'), -1)).toBe(0)
    expect(sampleAnimatableValue(track('linear'), 2 * COMPOSITION_TIME_BASE)).toBe(1)
  })

  it('adds, updates, deletes, orders, and bounds a track immutably', () => {
    const first = upsertKeyframe(undefined, COMPOSITION_TIME_BASE, 10, 0)
    const second = upsertKeyframe(first, 0, 2, 0)
    expect(second).toMatchObject({
      mode: 'keyframes',
      track: { keyframes: [{ tick: 0, value: 2 }, { tick: COMPOSITION_TIME_BASE, value: 10 }] },
    })
    const updated = updateKeyframe(second, COMPOSITION_TIME_BASE, COMPOSITION_TIME_BASE / 2, 8)
    expect(updated).toMatchObject({ track: { keyframes: [{ tick: 0 }, { tick: 500_000, value: 8 }] } })
    expect(deleteKeyframe(updated, 0)).toMatchObject({ track: { keyframes: [{ tick: 500_000, value: 8 }] } })
    expect(first).toMatchObject({ track: { keyframes: [{ tick: COMPOSITION_TIME_BASE, value: 10 }] } })

    let full: CompositionAnimatableValue | undefined
    for (let index = 0; index < MAX_KEYFRAMES_PER_VALUE; index += 1) {
      full = upsertKeyframe(full, index, index, 0)
    }
    expect(() => upsertKeyframe(full, MAX_KEYFRAMES_PER_VALUE, 99, 0)).toThrow('Не больше 32')
  })

  it('retimes trimmed/split curves with sampled boundary points', () => {
    const source: CompositionAnimatableValue = {
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'linear',
        keyframes: [
          { tick: 0, value: 0 },
          { tick: 2 * COMPOSITION_TIME_BASE, value: 20 },
          { tick: 4 * COMPOSITION_TIME_BASE, value: 40 },
        ],
      },
    }
    const sliced = sliceAnimatableValue(source, COMPOSITION_TIME_BASE, 3 * COMPOSITION_TIME_BASE, 4 * COMPOSITION_TIME_BASE)
    expect(sliced).toEqual({
      mode: 'keyframes',
      track: {
        timeBase: COMPOSITION_TIME_BASE,
        interpolation: 'linear',
        keyframes: [
          { tick: 0, value: 10 },
          { tick: COMPOSITION_TIME_BASE, value: 20 },
          { tick: 2 * COMPOSITION_TIME_BASE, value: 30 },
        ],
      },
    })
    expect(source.mode === 'keyframes' ? source.track.keyframes[0] : null).toEqual({ tick: 0, value: 0 })
  })

  it('falls back to static audio values and clones/slices authored automation independently', () => {
    const clip: AudioClip = {
      id: 'audio',
      kind: 'audio',
      sourceId: 'source',
      timelineStartTicks: 0,
      sourceInTicks: 0,
      sourceOutTicks: 4 * COMPOSITION_TIME_BASE,
      gain: 0.8,
      pan: -0.2,
      audioAnimation: { gain: track('linear') },
    }
    expect(audioPropertyValue(clip, 'pan')).toEqual({ mode: 'constant', value: -0.2 })
    expect(sampleAudioProperty(clip, 'gain', COMPOSITION_TIME_BASE / 2)).toBe(0.5)

    const cloned = cloneAudioAnimation(clip.audioAnimation)!
    const sliced = sliceAudioAnimation(cloned, COMPOSITION_TIME_BASE / 2, COMPOSITION_TIME_BASE, COMPOSITION_TIME_BASE)!
    expect(sliced.gain).toMatchObject({
      track: { keyframes: [{ tick: 0, value: 0.5 }, { tick: COMPOSITION_TIME_BASE / 2, value: 1 }] },
    })
    expect(cloned).not.toBe(clip.audioAnimation)
    expect(cloned.gain).not.toBe(clip.audioAnimation?.gain)
  })
})


it('retains the removed last key value and leaves missing/static keys untouched', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 250_000, value: 8 }] } }
  expect(deleteKeyframe(value, 250_000)).toEqual({ mode: 'constant', value: 8 })
  expect(deleteKeyframe(value, 0)).toBe(value)
  const constant = { mode: 'constant' as const, value: 9 }
  expect(deleteKeyframe(constant, 0)).toBe(constant)
})


it('inherits outgoing easing on insertion and starts a new leading segment as linear', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'ease_out', keyframes: [
    { tick: 250_000, value: 1, interpolation: 'ease_in' }, { tick: 1_000_000, value: 3, interpolation: 'hold' },
  ] } }
  const before = JSON.stringify(value)
  expect(upsertKeyframe(value, 500_000, 2, 1)).toMatchObject({ track: { keyframes: [{ tick: 250_000, interpolation: 'ease_in' }, { tick: 500_000, interpolation: 'ease_in' }, { tick: 1_000_000, interpolation: 'hold' }] } })
  expect(upsertKeyframe(value, 1_500_000, 4, 1)).toMatchObject({ track: { keyframes: [{ tick: 250_000 }, { tick: 1_000_000 }, { tick: 1_500_000, interpolation: 'hold' }] } })
  expect(upsertKeyframe(value, 0, 0, 1)).toMatchObject({ track: { keyframes: [{ tick: 0, interpolation: 'linear' }, { tick: 250_000 }, { tick: 1_000_000 }] } })
  expect(upsertKeyframe(value, 250_000, 2, 1)).toMatchObject({ track: { keyframes: [{ value: 2, interpolation: 'ease_in' }, { tick: 1_000_000 }] } })
  expect(upsertKeyframe(value, 250_000, 2, 1, 'ease_out')).toMatchObject({ track: { keyframes: [{ value: 2, interpolation: 'ease_out' }, { tick: 1_000_000 }] } })
  expect(JSON.stringify(value)).toBe(before)
})


it('samples signed authored keys across clip zero without clamping the sample clock', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: -1_000_000, value: 0 }, { tick: 1_000_000, value: 10 }] } }
  expect(sampleAnimatableValue(value, -500_000)).toBe(2.5)
  expect(sampleAnimatableValue(value, 0)).toBe(5)
  expect(upsertKeyframe(value, -500_000, 2.5, 0)).toMatchObject({ track: { keyframes: [{ tick: -1_000_000 }, { tick: -500_000 }, { tick: 1_000_000 }] } })
})


it('normalizes integer retime collisions with stable last-write wins and winning easing', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'linear', keyframes: [{ tick: 0, value: 1, interpolation: 'ease_in' }, { tick: 1_000_000, value: 2, interpolation: 'hold' }] } }
  const before = JSON.stringify(value)
  expect(updateKeyframe(value, 0, 1_000_000, 9)).toMatchObject({ track: { keyframes: [{ tick: 1_000_000, value: 2, interpolation: 'hold' }] } })
  expect(updateKeyframe(value, 1_000_000, 0, 9)).toMatchObject({ track: { keyframes: [{ tick: 0, value: 9, interpolation: 'hold' }] } })
  expect(JSON.stringify(value)).toBe(before)
})


it('continues endpoint slopes independently before and after the authored range', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: COMPOSITION_TIME_BASE, interpolation: 'ease_in', extrapolation: { before: 'continue', after: 'continue' }, keyframes: [{ tick: 1_000_000, value: 1 }, { tick: 2_000_000, value: 3 }] } }
  expect(sampleAnimatableValue(value, 0)).toBe(1)
  expect(sampleAnimatableValue(value, 3_000_000)).toBe(9)
  const linear = { ...value, track: { ...value.track, interpolation: 'linear' as const } }
  expect(sampleAnimatableValue(linear, 0)).toBe(-1)
  expect(sampleAnimatableValue(linear, 3_000_000)).toBe(5)
  expect(upsertKeyframe(value, 1_500_000, 2, 1)).toMatchObject({ track: { extrapolation: { before: 'continue', after: 'continue' } } })
})

it('folds repeated curves with Euclidean negative cycles and exact endpoint semantics', () => {
  for (const [mode, expected] of [
    ['loop', [15, 10, 15, 10, 20, 15, 10]],
    ['ping_pong', [15, 20, 15, 10, 20, 15, 10]],
    ['offset', [-5, 0, 5, 10, 20, 25, 30]],
  ] as const) {
    const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: mode, after: mode }, keyframes: [{ tick: 1000, value: 10 }, { tick: 2000, value: 20 }] } }
    expect([-500, 0, 500, 1000, 2000, 2500, 3000].map(tick => sampleAnimatableValue(value, tick, 1000))).toEqual(expected)
  }
})

it.each(['hold', 'continue', 'loop', 'ping_pong', 'offset'] as const)('shifts all authored keys on trim while preserving %s evaluation and easing', mode => {
  const source: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: mode, after: mode }, keyframes: [{ tick: 500, value: 1, interpolation: 'ease_in' }, { tick: 1500, value: 3 }] } }
  const before = JSON.stringify(source)
  const shifted = shiftAnimatableValue(source, -750_000)
  expect(shifted).toMatchObject({ track: { keyframes: [{ tick: -250, value: 1, interpolation: 'ease_in' }, { tick: 750, value: 3 }] } })
  for (const time of [0, 100_000, 750_000, 1_000_000, 2_000_000]) expect(sampleAnimatableValue(shifted, time)).toBeCloseTo(sampleAnimatableValue(source, time + 750_000))
  expect(JSON.stringify(source)).toBe(before)
})

it('retains curve context through split, including exact keys and extrapolated intervals', () => {
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: 'loop', after: 'offset' }, keyframes: [{ tick: -500, value: 1 }, { tick: 1000, value: 2, interpolation: 'hold' }, { tick: 2000, value: 3 }] } }
  for (const cut of [-1_000_000, 1_000_000, 3_000_000]) {
    for (const at of [-500_000, 0, 500_000, 1_000_000, 2_500_000]) {
      expect(sampleAnimatableValue(splitAnimatableValue(value, cut, false), at)).toBe(sampleAnimatableValue(value, at))
      expect(sampleAnimatableValue(splitAnimatableValue(value, cut, true), at)).toBe(sampleAnimatableValue(value, at + cut))
    }
  }
})

it.each(['linear', 'ease_in', 'ease_out', 'ease_in_out', 'bounce_in', 'bounce_out'] as const)('split preserves both halves of a crossing %s animation segment', interpolation => {
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation, keyframes: [{ tick: 0, value: 0 }, { tick: 4_000_000, value: 1 }] } }
  const left = splitAnimatableValue(original, 2_000_000, false)
  const right = splitAnimatableValue(original, 2_000_000, true)
  for (const at of [0, 250_000, 750_000, 1_500_000, 1_999_999]) {
    expect(sampleAnimatableValue(left, at)).toBe(sampleAnimatableValue(original, at))
    expect(sampleAnimatableValue(right, at)).toBe(sampleAnimatableValue(original, at + 2_000_000))
  }
})
