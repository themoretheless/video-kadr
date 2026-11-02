import { describe, expect, it } from 'vitest'
import { cloneDspEffect, setDspInterpolation, upsertDspKey, editDspKey, removeDspKey } from './dsp'
import type { CompositionDspEffect } from './types'
describe('DSP authored curve editing', () => {
  it('copies proxied curves without serialization and retains endpoint metadata', () => {
    const curve = { mode: 'keyframes' as const, track: { timeBase: 90000, interpolation: 'linear' as const, keyframes: [{ tick: 90000, value: -6, interpolation: 'ease_in' as const }] } }
    const effect: CompositionDspEffect = new Proxy({ id: 'amplify', animation: { gain: new Proxy(curve, {}) } }, {})
    const copy = cloneDspEffect(effect)
    curve.track.keyframes[0]!.value = 3
    expect(copy.animation?.gain).toMatchObject({ track: { keyframes: [{ value: -6, interpolation: 'ease_in' }] } })
    const changed = upsertDspKey(copy, 'gain', 1000000, 0)
    expect(changed.animation?.gain).toMatchObject({ track: { timeBase: 90000, keyframes: [{ tick: 90000, value: 0, interpolation: 'ease_in' }] } })
    expect(setDspInterpolation(changed, 'gain', 'hold').animation?.gain).toMatchObject({ track: { interpolation: 'hold' } })
  })
  it('retains a constant baseline when inserting a later key', () => {
    const changed = upsertDspKey({ id: 'amplify', animation: { gain: { mode: 'constant', value: -6 } } }, 'gain', 500000, 3)
    expect(changed.animation?.gain).toMatchObject({ track: { keyframes: [{ tick: 0, value: -6 }, { tick: 500000, value: 3 }] } })
  })
})

it('moves and removes authored keys without losing unaffected curve data', () => {
  const effect: CompositionDspEffect = { id: 'amplify', params: { gain: 3 }, animation: { gain: { mode: 'keyframes', track: { timeBase: 90000, interpolation: 'linear', keyframes: [{ tick: 0, value: -6, interpolation: 'ease_in' }, { tick: 90000, value: 3 }] } } } }
  const moved = editDspKey(effect, 'gain', 0, { localTick: 500000, value: -3 })
  expect(moved.animation?.gain).toMatchObject({ track: { keyframes: [{ tick: 45000, value: -3, interpolation: 'ease_in' }, { tick: 90000, value: 3 }] } })
  const collided = editDspKey(moved, 'gain', 0, { localTick: 1000000 })
  expect(collided.animation?.gain).toMatchObject({ track: { keyframes: [{ tick: 90000, value: -3, interpolation: 'ease_in' }] } })
  expect(removeDspKey(collided, 'gain', 0).animation).toEqual({})
  expect(effect.animation?.gain).toMatchObject({ track: { keyframes: [{ tick: 0, value: -6 }, { tick: 90000, value: 3 }] } })
})

it('deep-copies inactive source keys without converting them into audible automation',()=>{
 const effect:CompositionDspEffect={id:'concat.pitch',params:{semitones:0},concatSourceKeys:{semitones:[{at:0,value:-12,ease:[0.42,-0.5,0.58,1]},{at:1,value:12}]}}
 const copy=cloneDspEffect(effect);expect(copy).toEqual(effect);expect(copy.concatSourceKeys).not.toBe(effect.concatSourceKeys);expect(copy.concatSourceKeys!.semitones).not.toBe(effect.concatSourceKeys!.semitones);expect(copy.concatSourceKeys!.semitones![0]!.ease).not.toBe(effect.concatSourceKeys!.semitones![0]!.ease);expect(copy.animation).toBeUndefined()
})
