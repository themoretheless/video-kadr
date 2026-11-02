import { expect, it } from 'vitest'
import { BEZIER_PRESETS, bezierPresetId } from './easingPresets'
import { isBezierInterpolation, sampleBezier } from './bezier'
it('keeps the pinned named preset table and exact reverse lookup', () => {
  expect(BEZIER_PRESETS).toHaveLength(28)
  for (const preset of BEZIER_PRESETS) {
    expect(isBezierInterpolation(preset.interpolation)).toBe(true)
    expect(bezierPresetId(structuredClone(preset.interpolation))).toBe(preset.id)
  }
  const quad = BEZIER_PRESETS.find(p => p.id === 'ease_in_quad')!.interpolation
  expect(sampleBezier(quad,0.5)).toBeCloseTo(0.25,12)
  const back = BEZIER_PRESETS.find(p => p.id === 'ease_in_back')!.interpolation
  expect(back.p2[1]).toBe(-1.70158/3)
  expect(sampleBezier(back,0.5)).toBeCloseTo((1.70158+1)*0.125-1.70158*0.25,12)
  expect(bezierPresetId({ ...quad,p1: [quad.p1[0]+1e-12,quad.p1[1]] })).toBeUndefined()
})

it('writes both selected segment controls over existing Free/Broken handles', async () => {
  const { updateSelectedKeyframeInterpolation, sampleAnimatableValue } = await import('./keyframes')
  const preset = BEZIER_PRESETS.find(p => p.id === 'ease_in_quad')!.interpolation
  const value: import('./types').CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0,tangents: { incoming: { x: 0.2,y: -3,mode: 'free' },outgoing: { x: 0.2,y: 4,mode: 'free' },continuity: 'broken' } },{ tick: 1000,value: 1,tangents: { incoming: { x: 0.8,y: -2,mode: 'free' },outgoing: { x: 0.8,y: 3,mode: 'free' },continuity: 'broken' } }] } }
  const before = JSON.stringify(value)
  const changed = updateSelectedKeyframeInterpolation(value,[0],preset)
  expect(sampleAnimatableValue(changed,500,1000)).toBeCloseTo(0.25,12)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[0]!.tangents!.incoming).toEqual(value.track.keyframes[0]!.tangents!.incoming)
  expect(changed.track.keyframes[1]!.tangents!.outgoing).toEqual(value.track.keyframes[1]!.tangents!.outgoing)
  expect(JSON.stringify(value)).toBe(before)
})

it('bakes track presets into inherited segments while preserving explicit segment overrides', async () => {
  const { updateInterpolation, sampleAnimatableValue } = await import('./keyframes')
  const preset = BEZIER_PRESETS.find(p => p.id === 'ease_in_quad')!.interpolation
  const handles = { incoming: { x: 0.8,y: -2,mode: 'free' as const },outgoing: { x: 0.2,y: 4,mode: 'free' as const },continuity: 'broken' as const }
  const value: import('./types').CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0,tangents: structuredClone(handles) },{ tick: 1000,value: 1,tangents: structuredClone(handles) },{ tick: 2000,value: 2,interpolation: 'hold',tangents: structuredClone(handles) },{ tick: 3000,value: 3,tangents: structuredClone(handles) }] } }
  const before = JSON.stringify(value)
  const changed = updateInterpolation(value,preset)
  expect(sampleAnimatableValue(changed,500,1000)).toBeCloseTo(0.25,12)
  expect(sampleAnimatableValue(changed,1500,1000)).toBeCloseTo(1.25,12)
  expect(sampleAnimatableValue(changed,2500,1000)).toBe(2)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[0]!.interpolation).toBeUndefined()
  expect(changed.track.keyframes[1]!.interpolation).toBeUndefined()
  expect(changed.track.keyframes[2]!.interpolation).toBe('hold')
  expect(changed.track.keyframes[2]!.tangents!.outgoing).toEqual(handles.outgoing)
  expect(JSON.stringify(value)).toBe(before)
})

it('covers every canonical preset class and exactly identifies closed-form defaults', async () => {
  const { EASING_PRESETS,easingPresetId } = await import('./easingPresets')
  expect(EASING_PRESETS).toHaveLength(36)
  expect(new Set(EASING_PRESETS.map(p => p.id)).size).toBe(36)
  for (const preset of EASING_PRESETS) expect(easingPresetId(structuredClone(preset.interpolation))).toBe(preset.id)
  expect(easingPresetId({ kind: 'elastic',dir: 'in',amplitude: 1,period: 0.3+1e-12 })).toBeUndefined()
  expect(easingPresetId('ease_in')).toBeUndefined()
})

it('inherits both surrounding tangent controls when inserting a key into an authored spline', async () => {
  const { upsertKeyframe } = await import('./keyframes')
  const left = { incoming: { x: 0.8,y: -3,mode: 'free' as const },outgoing: { x: 0.2,y: 0.4,mode: 'free' as const },continuity: 'broken' as const }
  const right = { incoming: { x: 0.7,y: 0.9,mode: 'free' as const },outgoing: { x: 0.4,y: 3,mode: 'free' as const },continuity: 'broken' as const }
  const value: import('./types').CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 0,value: 0,tangents: left },{ tick: 1000,value: 1,tangents: right }] } }
  const before = JSON.stringify(value)
  const changed = upsertKeyframe(value,500,0.5,0)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[1]!.tangents).toEqual({ incoming: right.incoming,outgoing: left.outgoing,continuity: 'broken' })
  const appended = upsertKeyframe(value,1500,2,0)
  if (appended.mode !== 'keyframes') throw new Error('track required')
  expect(appended.track.keyframes[2]!.tangents).toEqual({ incoming: { x: 2/3,y: 2/3,mode: 'free' },outgoing: right.outgoing,continuity: 'broken' })
  expect(JSON.stringify(value)).toBe(before)
})

it('bakes an explicit insertion preset over inherited controls and next arriving side', async () => {
  const { upsertKeyframe,sampleAnimatableValue } = await import('./keyframes')
  const tangents = { incoming: { x: 0.8,y: -2,mode: 'free' as const },outgoing: { x: 0.2,y: 4,mode: 'free' as const },continuity: 'broken' as const }
  const value: import('./types').CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'linear',keyframes: [{ tick: 0,value: 0,tangents },{ tick: 1000,value: 1,tangents }] } }
  const preset = BEZIER_PRESETS.find(p => p.id === 'ease_in_quad')!.interpolation
  const before = JSON.stringify(value)
  for (const tick of [0,500]) {
    const changed = upsertKeyframe(value,tick,0,0,preset)
    expect(sampleAnimatableValue(changed,(tick+1000)/2,1000)).toBeCloseTo(0.25,12)
    if (changed.mode !== 'keyframes') throw new Error('track required')
    const key = changed.track.keyframes.find(key => key.tick === tick)!
    expect(key.tangents!.outgoing).toEqual({ x: 1/3,y: 0,mode: 'free' })
    expect(changed.track.keyframes.at(-1)!.tangents!.incoming).toEqual({ x: 2/3,y: 1/3,mode: 'free' })
  }
  expect(JSON.stringify(value)).toBe(before)
})
