import { expect, it } from 'vitest'
import type { KeyframeChannel } from './keyframeChannels'
import { keyframeCurvePoints, keyframeTailPoints, keyframeValueRange, keyframeValueY } from './keyframeCurve'
const channel: KeyframeChannel = { id: 'x', target: { kind: 'visual', clipId: 'clip' }, property: 'x', start: 500_000, duration: 2_000_000, locked: false, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: 0, value: 0 }, { tick: 1000, value: 10 }] } } }
it('maps actual curve values to a padded axis and composition time with authored timeBase', () => {
  const before = JSON.stringify(channel)
  expect(keyframeValueRange(channel)).toEqual({ minimum: -1, maximum: 11 })
  const points = keyframeCurvePoints(channel, 100, 120)
  expect(points.startsWith('50.000,110.000 ')).toBe(true)
  expect(points).toContain('100.000,60.000')
  expect(points.endsWith('150.000,10.000')).toBe(true)
  expect(keyframeValueY(10, keyframeValueRange(channel), 120)).toBe(10)
  expect(JSON.stringify(channel)).toBe(before)
})
it('draws eased sampling and exact hold corners instead of diagonal hold ramps', () => {
  const eased: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, interpolation: 'ease_in' } } }
  expect(keyframeCurvePoints(eased, 100, 120)).toContain('100.000,97.500')
  const held: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, interpolation: 'hold' } } }
  expect(keyframeCurvePoints(held, 100, 120)).toBe('50.000,110.000 150.000,110.000 150.000,10.000')
})
it('centres constant tracks on a nominal band scaled for large values', () => {
  const constant: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, keyframes: [{ tick: 0, value: 100 }, { tick: 1000, value: 100 }] } } }
  expect(keyframeValueRange(constant)).toEqual({ minimum: 90, maximum: 110 })
  expect(keyframeValueY(100, keyframeValueRange(constant), 120)).toBe(60)
})

it('includes continued clip endpoints in the range and draws both sloping tails', () => {
  const continued: KeyframeChannel = { ...channel, duration: 3_000_000, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: 'continue', after: 'continue' }, keyframes: [{ tick: 1000, value: 10 }, { tick: 2000, value: 20 }] } } }
  expect(keyframeValueRange(continued)).toEqual({ minimum: -3, maximum: 33 })
  expect(keyframeTailPoints(continued, 'before', 100, 120)).toBe('50.000,110.000 150.000,76.667')
  expect(keyframeTailPoints(continued, 'after', 100, 120)).toBe('250.000,43.333 350.000,10.000')
})
it('clips tails to the layer interval when authored keys are outside it', () => {
  const outside: KeyframeChannel = { ...channel, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', keyframes: [{ tick: -1000, value: 0 }, { tick: 3000, value: 10 }] } } }
  expect(keyframeTailPoints(outside, 'before', 100, 120)).toBe('')
  expect(keyframeTailPoints(outside, 'after', 100, 120)).toBe('')
  const before: KeyframeChannel = { ...outside, value: { mode: 'keyframes', track: { ...outside.value.track, keyframes: [{ tick: 3000, value: 10 }] } } }
  expect(keyframeTailPoints(before, 'before', 100, 120)).toBe('50.000,60.000 250.000,60.000')
  expect(keyframeTailPoints(before, 'after', 100, 120)).toBe('')
})

it('ranges Offset by repeated interior peaks rather than clip endpoints alone', () => {
  const offset: KeyframeChannel = { ...channel, duration: 4_000_000, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: 'offset', after: 'offset' }, keyframes: [{ tick: 1000, value: 1 }, { tick: 1500, value: 10 }, { tick: 2000, value: 2 }] } } }
  const range = keyframeValueRange(offset)
  expect(range.minimum).toBeCloseTo(-1.2)
  expect(range.maximum).toBeCloseTo(13.2)
})

it.each(['loop', 'ping_pong', 'offset'] as const)('draws repeated %s curves and clips the last cycle', mode => {
  const repeated: KeyframeChannel = { ...channel, start: 0, duration: 3_500_000, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'linear', extrapolation: { before: mode, after: mode }, keyframes: [{ tick: 1000, value: 10 }, { tick: 2000, value: 20 }] } } }
  const range = { minimum: 0, maximum: 40 }
  const points = keyframeTailPoints(repeated, 'after', 100, 120, range)
  expect(points.split(' ').length).toBeGreaterThan(32)
  expect(points.startsWith('200.000,60.000')).toBe(true)
  expect(points.endsWith(mode === 'offset' ? '350.000,15.000' : '350.000,75.000')).toBe(true)
  if (mode === 'loop') expect(points).toContain('300.000,60.000 300.000,90.000')
  if (mode === 'ping_pong') expect(points).toContain('300.000,90.000')
  expect(points.split(' ').every(point => Number(point.split(',')[0]) >= 200 && Number(point.split(',')[0]) <= 350)).toBe(true)
})
it('draws Hold jumps in mirrored negative cycles without diagonal ramps', () => {
  const repeated: KeyframeChannel = { ...channel, start: 0, value: { mode: 'keyframes', track: { timeBase: 1000, interpolation: 'hold', extrapolation: { before: 'ping_pong', after: 'loop' }, keyframes: [{ tick: 1000, value: 10 }, { tick: 2000, value: 20 }] } } }
  const points = keyframeTailPoints(repeated, 'before', 100, 120, { minimum: 0, maximum: 40 })
  expect(points).toContain('0.000,60.000 0.000,90.000')
  expect(points).toContain('0.000,90.000 100.000,90.000')
})
it('bounds rendering work for subpixel repetitions without changing authored data', () => {
  const dense: KeyframeChannel = { ...channel, duration: 1_000_000, value: { mode: 'keyframes', track: { timeBase: 1_000_000, interpolation: 'linear', extrapolation: { before: 'loop', after: 'loop' }, keyframes: [{ tick: 0, value: 10 }, { tick: 1, value: 20 }] } } }
  const before = JSON.stringify(dense)
  const points = keyframeTailPoints(dense, 'after', 100, 120)
  expect(points.split(' ').length).toBeLessThanOrEqual(8193)
  expect(points).not.toContain('NaN')
  expect(JSON.stringify(dense)).toBe(before)
})

it('projects outgoing and unmirrored incoming controls into segment time and value', async () => {
  const { keyframeTangentPoints } = await import('./keyframeCurve')
  const spline: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, interpolation: { kind: 'bezier', p1: [0.25,0.2], p2: [0.75,0.8] } } } }
  const points = keyframeTangentPoints(spline, 100, 120, { minimum: -1, maximum: 11 })
  expect(points).toEqual([{ tick: 0, side: 'outgoing', x: 75, y: 90, keyX: 50, keyY: 110 }, { tick: 1000, side: 'incoming', x: 125, y: 30, keyX: 150, keyY: 10 }])
  expect(keyframeTangentPoints(channel, 100, 120)).toEqual([])
})

it('inverts handle geometry, clamps time and preserves flat-segment Y', async () => {
  const { keyframeTangentPoints, keyframeTangentAtPoint } = await import('./keyframeCurve')
  const range = { minimum: -5, maximum: 15 }
  for (const values of [[0,10],[10,0],[4,4]]) {
    const spline: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, interpolation: { kind: 'bezier', p1: [0.25,-2], p2: [0.75,3] }, keyframes: [{ tick: 0, value: values[0]! }, { tick: 1000, value: values[1]! }] } } }
    for (const handle of keyframeTangentPoints(spline,100,120,range)) {
      const control = keyframeTangentAtPoint(spline,handle.tick,handle.side,handle,100,120,range)!
      expect(control.x).toBe(handle.side === 'incoming' ? 0.75 : 0.25)
      expect(control.y).toBe(handle.side === 'incoming' ? 3 : -2)
    }
    expect(keyframeTangentAtPoint(spline,0,'outgoing',{ x: -1000, y: 60 },100,120,range)!.x).toBe(0)
    expect(keyframeTangentAtPoint(spline,0,'incoming',{ x: 50, y: 60 },100,120,range)).toBeNull()
    expect(keyframeTangentAtPoint(spline,0,'outgoing',{ x: Infinity, y: 60 },100,120,range)).toBeNull()
  }
})

it('projects clocked curve keys into local time instead of their authored times', () => {
  const clocked: KeyframeChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, sampleClock: { rateNum: 2, rateDen: 1, offsetNum: 200000 } } } }
  const points = keyframeCurvePoints(clocked, 100, 120, { minimum: -1, maximum: 11 })
  expect(points.startsWith('40.000,110.000 ')).toBe(true)
  expect(points.endsWith('90.000,10.000')).toBe(true)
})
