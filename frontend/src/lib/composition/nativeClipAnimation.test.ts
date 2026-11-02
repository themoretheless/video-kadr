import { expect, it } from 'vitest'
import fixture from '../../../../backend/tests/fixtures/concat-merge-preview.json'
import type { WireVideoClip } from './types'
import { rebaseNativeClipAnimation } from './nativeClipAnimation'
import { sampleAnimatableValue } from './keyframes'
it('preserves the sampled phase of native opacity, motion and effect curves across a cut', () => {
  const curve = { mode: 'keyframes' as const, track: { timeBase: 1000000, interpolation: 'ease_in_out' as const, keyframes: [{ tick: 0, value: 0.2 }, { tick: 3000000, value: 0.9 }] } }
  const video = structuredClone(fixture.nativeFirstFree[0]!.after.compositions['new-timeline'].content.tracks[1]!.clips[0]) as unknown as WireVideoClip
  const input = { ...video, opacity: curve, transform: { ...video.transform, x: curve }, effects: [{ kind: 'brightness_contrast' as const, parameters: {}, animation: { brightness: curve, contrast: curve } }] }
  const before = JSON.stringify(input)
  const output = rebaseNativeClipAnimation(input, -1000000)
  for (const time of [0, 1, 250000, 1999999, 2000000]) {
    expect(sampleAnimatableValue(output.opacity, time)).toBeCloseTo(sampleAnimatableValue(input.opacity, time + 1000000), 14)
    expect(sampleAnimatableValue(output.effects[0]!.animation.brightness, time)).toBeCloseTo(sampleAnimatableValue(curve, time + 1000000), 14)
    expect(sampleAnimatableValue(output.transform.x, time)).toBeCloseTo(sampleAnimatableValue(input.transform.x, time + 1000000), 14)
  }
  expect(output.opacity).toMatchObject({ track: { keyframes: [{ tick: -1000000 }, { tick: 2000000 }] } })
  expect(output.placement).toEqual(input.placement)
  expect(JSON.stringify(input)).toBe(before)
  expect(() => rebaseNativeClipAnimation(input, NaN)).toThrow('времени')
})
it('rebases packed RGBA keys and graphic history clocks without shifting graphic layers twice', () => {
  const video = structuredClone(fixture.nativeFirstFree[0]!.after.compositions['new-timeline'].content.tracks[1]!.clips[0]) as unknown as WireVideoClip
  const common = { id: 'visual', timelineStartTick: 0, timelineEndTick: 3000000, width: 16, height: 16, transform: video.transform, opacity: video.opacity, enabled: true }
  const track = { keys: [{ t_us: 0, value: { r: 10, g: 20, b: 30, a: 255 }, in_: [0, 0] as const, out: [1, 1] as const, segment: { kind: 'Linear' as const } }], extrapolate: { before: 'Hold' as const, after: 'Hold' as const } }
  const color = { ...common, color: track.keys[0]!.value, colorAnimation: track }
  const shifted = rebaseNativeClipAnimation(color, -123456)
  expect(shifted.colorAnimation.keys[0]!.t_us).toBe(-123456)
  expect(shifted.colorAnimation.keys[0]!.value).toEqual(track.keys[0]!.value)
  expect(track.keys[0]!.t_us).toBe(0)
  const graphic = { ...common, graphic: { layers: [] }, effects: [], animationOffsetUs: 500000 }
  const shiftedGraphic = rebaseNativeClipAnimation(graphic, -123456)
  expect(shiftedGraphic.animationOffsetUs).toBe(623456)
  expect(shiftedGraphic.graphic).toBe(graphic.graphic)
})
