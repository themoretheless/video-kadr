import { expect, it } from 'vitest'
import { snapshotSelectedKeyframes, planKeyframePaste, type KeyframeCopySubject } from './keyframeClipboard'
it('copies an independent parameter snapshot on the composition clock across layers and authored bases', () => {
  const subjects: KeyframeCopySubject[] = [
    { parameter: 'x', timelineStartTicks: 2_000_000, selectedTicks: [500, 1500], value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: 500, value: 1, interpolation: 'ease_in' }, { tick: 1500, value: 2 }] } } },
    { parameter: 'y', timelineStartTicks: 0, selectedTicks: [1_000_000], value: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'hold', keyframes: [{ tick: 1_000_000, value: 3 }] } } },
  ]
  const before = JSON.stringify(subjects)
  const copied = snapshotSelectedKeyframes(subjects)
  expect(copied).toEqual([{ parameter: 'x', keys: [{ tick: 1_500_000, value: 1, interpolation: 'ease_in' }, { tick: 2_500_000, value: 2, interpolation: 'linear' }] }, { parameter: 'y', keys: [{ tick: 0, value: 3, interpolation: 'hold' }] }])
  expect(JSON.stringify(subjects)).toBe(before)
  expect(copied[0]!.keys[0]).not.toBe(subjects[0]!.value.mode === 'keyframes' && subjects[0]!.value.track.keyframes[0])
})
it('refuses a stale selected key before yielding a partial copy', () => {
  expect(() => snapshotSelectedKeyframes([{ parameter: 'x', timelineStartTicks: 0, selectedTicks: [1], value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [] } } }])).toThrow('Selected keyframe no longer exists')
  expect(snapshotSelectedKeyframes([])).toEqual([])
})

it('lifts constants, merges collisions pasted-last and keeps out-of-range times for each target start', () => {
  const groups = [{ parameter: 'x', keys: [{ tick: 0, value: 5, interpolation: 'ease_in' as const }, { tick: 2_000_000, value: 8, interpolation: 'hold' as const }] }, { parameter: 'gain', keys: [{ tick: 0, value: 1 }] }]
  const result = planKeyframePaste(groups, [
    { id: 'static', parameter: 'x', timelineStartTicks: 2_000_000, value: { mode: 'constant', value: 99 } },
    { id: 'keyed', parameter: 'x', timelineStartTicks: 0, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'hold', keyframes: [{ tick: 0, value: 0 }, { tick: 1000, value: 2 }] } } },
  ], 1_000_000)
  expect(result.skipped).toEqual(['gain'])
  expect(result.entries[0]).toMatchObject({ pastedTicks: [-1_000_000, 1_000_000], value: { track: { timeBase: 1_000_000, keyframes: [{ tick: -1_000_000, value: 5, interpolation: 'ease_in' }, { tick: 1_000_000, value: 8, interpolation: 'hold' }] } } })
  expect(result.entries[1]).toMatchObject({ pastedTicks: [1000, 3000], value: { track: { timeBase: 1000, interpolation: 'hold', keyframes: [{ tick: 0, value: 0 }, { tick: 1000, value: 5, interpolation: 'ease_in' }, { tick: 3000, value: 8 }] } } })
  expect(groups[0]!.keys[0]!.tick).toBe(0)
})
it('folds copied groups of the same parameter and refuses duplicate target addresses', () => {
  const groups = [{ parameter: 'x', keys: [{ tick: 0, value: 1 }] }, { parameter: 'x', keys: [{ tick: 0, value: 2 }] }]
  const target = { id: 'a', parameter: 'x', timelineStartTicks: 0, value: { mode: 'constant' as const, value: 0 } }
  expect(planKeyframePaste(groups, [target], 0).entries[0]).toMatchObject({ value: { track: { keyframes: [{ tick: 0, value: 2 }] } } })
  expect(() => planKeyframePaste(groups, [target, target], 0)).toThrow('Duplicate keyframe paste targets')
})

it('copies independent tangent sides and re-solves pasted Auto against destination neighbours', () => {
  const tangents = { incoming: { x: 2/3,y: 2/3,mode: 'auto' as const },outgoing: { x: 1/3,y: 1/3,mode: 'auto' as const },continuity: 'smooth' as const }
  const source: KeyframeCopySubject = { parameter: 'x',timelineStartTicks: 0,selectedTicks: [1000],value: { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 1000,value: 3,tangents }] } } }
  const copied = snapshotSelectedKeyframes([source])
  expect(copied[0]!.keys[0]!.tangents).not.toBe(tangents)
  expect(copied[0]!.keys[0]!.tangents!.incoming).not.toBe(tangents.incoming)
  const before = JSON.stringify(copied)
  const result = planKeyframePaste(copied,[{ id: 'target',parameter: 'x',timelineStartTicks: 0,value: { mode: 'keyframes',track: { timeBase: 1000,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] },keyframes: [{ tick: 0,value: 0 },{ tick: 2000,value: 2 }] } } }],1_000_000)
  const value = result.entries[0]!.value
  if (value.mode !== 'keyframes') throw new Error('track required')
  expect(value.track.keyframes[1]!.tangents).toMatchObject({ incoming: { y: 1 },outgoing: { y: 0 } })
  expect(JSON.stringify(copied)).toBe(before)
})

it('copies local spacing and pastes into a clocked destination without dropping its clock or extrapolation', () => {
  const value: KeyframeCopySubject['value'] = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 250000 }, extrapolation: { before: 'loop', after: 'ping_pong' }, keyframes: [{ tick: 250, value: 1 }, { tick: 1250, value: 2 }] } }
  const groups = snapshotSelectedKeyframes([{ parameter: 'x', timelineStartTicks: 1000000, selectedTicks: [250, 1250], value }])
  expect(groups[0]!.keys.map(key => key.tick)).toEqual([0, 500000])
  const pasted = planKeyframePaste(groups, [{ id: 'target', parameter: 'x', value, timelineStartTicks: 1000000 }], 1100000)
  expect(pasted.entries[0]!.pastedTicks).toEqual([450, 1450])
  expect(pasted.entries[0]!.value).toMatchObject({ track: { sampleClock: value.track.sampleClock, extrapolation: value.track.extrapolation } })
})
