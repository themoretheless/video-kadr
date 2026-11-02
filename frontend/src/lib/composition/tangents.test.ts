import { expect, it } from 'vitest'
import { cloneAnimatableValue, sampleAnimatableValue, animatableExtents } from './keyframes'
import { isKeyframeTangents, segmentInterpolation } from './tangents'
import { scaleTracksTwins } from './scaleLink'
import type { CompositionAnimatableValue, CompositionKeyframeTangents } from './types'

it('uses per-key controls in preview, bounds and Continue with independent clones', () => {
  const tangents: CompositionKeyframeTangents = { incoming: { x: 0.75, y: 1, mode: 'free' }, outgoing: { x: 0.25, y: 0, mode: 'free' }, continuity: 'smooth' }
  const value: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: { kind: 'bezier', p1: [0.1,-2], p2: [0.9,3] }, extrapolation: { before: 'continue', after: 'continue' }, keyframes: [{ tick: 0, value: 0, tangents }, { tick: 1000, value: 1, tangents }] } }
  expect(sampleAnimatableValue(value, 500, 1000)).toBeCloseTo(0.5, 12)
  expect(sampleAnimatableValue(value, -1000, 1000)).toBe(0)
  expect(sampleAnimatableValue(value, 2000, 1000)).toBe(1)
  expect(animatableExtents(value)).toEqual({ minimum: 0, maximum: 1 })
  const cloned = cloneAnimatableValue(value)
  expect(scaleTracksTwins(value, cloned)).toBe(true)
  if (cloned.mode !== 'keyframes') throw new Error('track required')
  expect(cloned.track.keyframes[0]!.tangents).not.toBe(tangents)
  expect(cloned.track.keyframes[0]!.tangents!.outgoing).not.toBe(tangents.outgoing)
  const changed: CompositionAnimatableValue = { ...cloned, track: { ...cloned.track, keyframes: [{ ...cloned.track.keyframes[0]!, tangents: { ...tangents, continuity: 'broken' } }, cloned.track.keyframes[1]!] } }
  expect(scaleTracksTwins(value, changed)).toBe(false)
  expect(segmentInterpolation(value.track.keyframes[0]!, value.track.keyframes[1]!, 'hold')).toBe('hold')
})

it('validates exact tangent fields, finite unrestricted Y and X limits', () => {
  const value = { incoming: { x: 0, y: -2, mode: 'auto' }, outgoing: { x: 1, y: 3, mode: 'free' }, continuity: 'broken' }
  expect(isKeyframeTangents(value)).toBe(true)
  for (const invalid of [{ ...value, extra: 1 }, { ...value, continuity: 'other' }, { ...value, outgoing: { ...value.outgoing, x: -0.1 } }, { ...value, incoming: { ...value.incoming, y: Infinity } }, { ...value, incoming: { ...value.incoming, mode: 'other' } }]) expect(isKeyframeTangents(invalid)).toBe(false)
})

it('matches all upstream Auto/Smooth solver goldens and remains idempotent', async () => {
  const { normalizeKeyframeTangents } = await import('./tangents')
  const fixture = await import('../../../../backend/src/domain/tangents_golden.json')
  const convert = (key: typeof fixture.cases[number]['input']['value'][number]) => ({
    tick: key.t_us, value: key.value,
    interpolation: key.segment.kind === 'Spline' ? { kind: 'bezier' as const, p1: [1/3,1/3] as const, p2: [2/3,2/3] as const } : 'linear' as const,
    tangents: { incoming: { ...key.in, mode: key.in.mode.toLowerCase() as 'auto' | 'free' }, outgoing: { ...key.out, mode: key.out.mode.toLowerCase() as 'auto' | 'free' }, continuity: key.continuity.toLowerCase() as 'smooth' | 'broken' },
  })
  for (const example of fixture.cases) {
    const actual = normalizeKeyframeTangents(example.input.value.map(convert), 'linear')
    const expected = example.expect.value.map(convert)
    expect(actual).toHaveLength(expected.length)
    for (let i = 0; i < actual.length; i++) {
      for (const side of ['incoming','outgoing'] as const) {
        expect(actual[i]!.tangents![side].x).toBe(expected[i]!.tangents[side].x)
        expect(actual[i]!.tangents![side].y).toBeCloseTo(expected[i]!.tangents[side].y, 9)
        expect(actual[i]!.tangents![side].mode).toBe(expected[i]!.tangents[side].mode)
      }
    }
    expect(normalizeKeyframeTangents(actual, 'linear')).toEqual(actual)
  }
})

it('re-solves Auto on authored value edits without changing the input', async () => {
  const { upsertKeyframe } = await import('./keyframes')
  const tangents: CompositionKeyframeTangents = { incoming: { x: 2/3, y: 2/3, mode: 'auto' }, outgoing: { x: 1/3, y: 1/3, mode: 'auto' }, continuity: 'smooth' }
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: { kind: 'bezier', p1: [1/3,1/3], p2: [2/3,2/3] }, keyframes: [{ tick: 0, value: 0, tangents }, { tick: 1000, value: 1, tangents }, { tick: 2000, value: 2, tangents }] } }
  const before = JSON.stringify(original)
  const changed = upsertKeyframe(original, 1000, 3, 0)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[1]!.tangents!.incoming).toEqual({ x: 2/3,y: 1,mode: 'auto' })
  expect(changed.track.keyframes[1]!.tangents!.outgoing.y).toBe(0)
  expect(JSON.stringify(original)).toBe(before)
})

it('bakes Free sides when an authored segment becomes Bézier', async () => {
  const { updateInterpolation, updateSelectedKeyframeInterpolation } = await import('./keyframes')
  const tangents: CompositionKeyframeTangents = { incoming: { x: 0.8, y: -3, mode: 'auto' }, outgoing: { x: 0.2, y: 4, mode: 'auto' }, continuity: 'smooth' }
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0, tangents }, { tick: 1000, value: 1, tangents }] } }
  const before = JSON.stringify(original)
  const mode = { kind: 'bezier' as const, p1: [1/3,1/3] as const, p2: [2/3,2/3] as const }
  for (const changed of [updateInterpolation(original, mode), updateSelectedKeyframeInterpolation(original, [0], mode)]) {
    if (changed.mode !== 'keyframes') throw new Error('track required')
    expect(changed.track.keyframes[0]!.tangents!.outgoing).toEqual({ x: 1/3,y: 1/3,mode: 'free' })
    expect(changed.track.keyframes[1]!.tangents!.incoming).toEqual({ x: 2/3,y: 2/3,mode: 'free' })
    expect(changed.track.keyframes[0]!.tangents!.incoming).toEqual(tangents.incoming)
    expect(changed.track.keyframes[1]!.tangents!.outgoing).toEqual(tangents.outgoing)
  }
  expect(JSON.stringify(original)).toBe(before)
})

it('materializes incoming from the preceding segment and rejects stale edits atomically', async () => {
  const { updateKeyframeTangents } = await import('./tangents')
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: { kind: 'bezier', p1: [0.2,0.3], p2: [0.7,0.8] }, keyframes: [{ tick: 0, value: 0, interpolation: { kind: 'bezier', p1: [0.1,-2], p2: [0.9,3] } }, { tick: 1000, value: 1 }, { tick: 2000, value: 2 }] } }
  const before = JSON.stringify(original)
  const changed = updateKeyframeTangents(original, [1000], { continuity: 'broken' })
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[1]!.tangents).toEqual({ incoming: { x: 0.9, y: 3, mode: 'free' }, outgoing: { x: 0.2, y: 0.3, mode: 'free' }, continuity: 'broken' })
  for (let tick = 0; tick <= 2000; tick += 10) expect(sampleAnimatableValue(changed, tick, 1000)).toBe(sampleAnimatableValue(original, tick, 1000))
  expect(() => updateKeyframeTangents(original, [1000,999], { continuity: 'smooth' })).toThrow('no longer')
  expect(() => updateKeyframeTangents(original, [1000], { outgoing: { x: -1, y: 0, mode: 'free' } })).toThrow('Invalid')
  expect(JSON.stringify(original)).toBe(before)
})

it('incoming Smooth edits preserve the written world slope across unequal spans', async () => {
  const { editKeyframeTangent } = await import('./tangents')
  const tangents: CompositionKeyframeTangents = { incoming: { x: 0.8, y: 0.8, mode: 'free' }, outgoing: { x: 0.25, y: 0.25, mode: 'free' }, continuity: 'smooth' }
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: { kind: 'bezier', p1: [1/3,1/3], p2: [2/3,2/3] }, keyframes: [{ tick: 0, value: 0 }, { tick: 1000, value: 2, tangents }, { tick: 3000, value: 5 }] } }
  const changed = editKeyframeTangent(original, 1000, 'incoming', 0.8, 0.4)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  const actual = changed.track.keyframes[1]!.tangents!
  expect(actual.incoming.y).toBeCloseTo(0.4, 12)
  const incomingSlope = (1-actual.incoming.y)/(1-actual.incoming.x)*2/1000
  const outgoingSlope = actual.outgoing.y/actual.outgoing.x*3/2000
  expect(incomingSlope).toBeCloseTo(outgoingSlope, 14)
  expect(actual.outgoing.y).toBeCloseTo(1, 12)
  expect(() => editKeyframeTangent(original, 1000, 'incoming', -1, 0)).toThrow('Invalid')
})

it('Auto converts both adjacent segments and Free retains the resolved shape', async () => {
  const { setKeyframeTangentMode } = await import('./tangents')
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'hold', keyframes: [{ tick: 0,value: 0 }, { tick: 1000,value: 1 }, { tick: 2000,value: 2 }] } }
  const auto = setKeyframeTangentMode(original,[1000],'auto')
  if (auto.mode !== 'keyframes') throw new Error('track required')
  expect(auto.track.keyframes[0]!.interpolation).toMatchObject({ kind: 'bezier' })
  expect(auto.track.keyframes[1]!.interpolation).toMatchObject({ kind: 'bezier' })
  expect(auto.track.keyframes[1]!.tangents).toMatchObject({ continuity: 'smooth', incoming: { mode: 'auto' },outgoing: { mode: 'auto' } })
  const free = setKeyframeTangentMode(auto,[1000],'free')
  for (let tick = 0;tick<=2000;tick+=10) expect(sampleAnimatableValue(free,tick,1000)).toBeCloseTo(sampleAnimatableValue(auto,tick,1000),12)
})

it('grabbing an Auto side converts both sides to Free and keeps Smooth continuity', async () => {
  const { setKeyframeTangentMode, editKeyframeTangent } = await import('./tangents')
  const original: CompositionAnimatableValue = { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: 0,value: 0 }, { tick: 1000,value: 1 }, { tick: 2000,value: 2 }] } }
  const auto = setKeyframeTangentMode(original,[1000],'auto')
  const changed = editKeyframeTangent(auto,1000,'incoming',0.8,0.4)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  const record = changed.track.keyframes[1]!.tangents!
  expect(record.incoming.mode).toBe('free')
  expect(record.outgoing.mode).toBe('free')
  expect(record.continuity).toBe('smooth')
  expect(record.incoming.y).toBeCloseTo(0.4,12)
  expect((1-record.incoming.y)/(1-record.incoming.x)).toBeCloseTo(record.outgoing.y/record.outgoing.x,12)
})

it('a tangent edit converts its owning segment while retaining the opposite segment', async () => {
  const { editKeyframeTangent } = await import('./tangents')
  const original: CompositionAnimatableValue = { mode: 'keyframes',track: { timeBase: 1000,interpolation: 'hold',keyframes: [{ tick: 0,value: 0 },{ tick: 1000,value: 1,interpolation: { kind: 'bezier',p1: [0.2,0.3],p2: [0.7,0.8] } },{ tick: 2000,value: 2 }] } }
  const before = JSON.stringify(original)
  const changed = editKeyframeTangent(original,1000,'incoming',0.8,0.4)
  if (changed.mode !== 'keyframes') throw new Error('track required')
  expect(changed.track.keyframes[0]!.interpolation).toMatchObject({ kind: 'bezier' })
  expect(changed.track.keyframes[1]!.interpolation).toEqual(original.track.keyframes[1]!.interpolation)
  expect(changed.track.keyframes[1]!.tangents!.incoming).toEqual({ x: 0.8,y: 0.4,mode: 'free' })
  expect(changed.track.keyframes[1]!.tangents!.outgoing).toEqual({ x: 0.2,y: 0.3,mode: 'free' })
  for (let tick=1000;tick<=2000;tick+=10) expect(sampleAnimatableValue(changed,tick,1000)).toBe(sampleAnimatableValue(original,tick,1000))
  expect(JSON.stringify(original)).toBe(before)
})
