import { describe, expect, it } from 'vitest'
import { buildAnimationPreset } from './animationPresets'

const context = {
  clipDurationTicks: 4_000_000,
  presetDurationTicks: 750_000,
  canvasWidth: 1_000,
  values: { x: 40, y: -20, scaleX: 2, scaleY: 1.5, rotationDegrees: 15, opacity: 0.8 },
}

describe('composition animation presets', () => {
  it('materializes in/out presets as ordinary editable keyframes', () => {
    expect(buildAnimationPreset('slide_left_in', context)).toMatchObject({
      x: { mode: 'keyframes', track: { interpolation: 'ease_out', keyframes: [{ tick: 0, value: -310 }, { tick: 750_000, value: 40 }] } },
      opacity: { mode: 'keyframes', track: { keyframes: [{ tick: 0, value: 0 }, { tick: 750_000, value: 0.8 }] } },
    })
    expect(buildAnimationPreset('zoom_out', context)).toMatchObject({
      scaleX: { mode: 'keyframes', track: { keyframes: [{ tick: 3_250_000, value: 2 }, { tick: 4_000_000, value: 0.5 }] } },
      opacity: { mode: 'keyframes', track: { keyframes: [{ tick: 3_250_000, value: 0.8 }, { tick: 4_000_000, value: 0 }] } },
    })
  })

  it('builds bounded loop tracks across the whole clip', () => {
    const pulse = buildAnimationPreset('pulse_loop', context)
    expect(pulse.scaleX?.mode).toBe('keyframes')
    if (pulse.scaleX?.mode !== 'keyframes') throw new Error('expected keyframes')
    expect(pulse.scaleX.track.keyframes).toHaveLength(9)
    expect(pulse.scaleX.track.keyframes.at(-1)).toEqual({ tick: 4_000_000, value: 2 })
    expect(buildAnimationPreset('spin_loop', context)).toMatchObject({
      rotationDegrees: { mode: 'keyframes', track: { interpolation: 'linear', keyframes: [{ value: 15 }, { tick: 4_000_000, value: 375 }] } },
    })
  })
})
