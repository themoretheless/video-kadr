import { describe, expect, it } from 'vitest'
import {
  audioPropertyValue,
  cloneAudioAnimation,
  deleteKeyframe,
  sampleAudioProperty,
  sampleAnimatableValue,
  sliceAudioAnimation,
  sliceAnimatableValue,
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
