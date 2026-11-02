import { keyframeLocalTime, keyframeAuthoredTime } from './keyframeClock'
import { segmentInterpolation } from './tangents'
import { sampleAnimatableValue } from './keyframes'
import { COMPOSITION_TIME_BASE } from './types'
import type { KeyframeChannel } from './keyframeChannels'
export type ScalarCurveChannel = Pick<KeyframeChannel, 'value' | 'duration' | 'start'>
export interface KeyframeValueRange { minimum: number; maximum: number }
export function keyframeValueRange(channel: ScalarCurveChannel): KeyframeValueRange {
  const keys = channel.value.track.keyframes
  const values = keys.map((key) => key.value)
  values.push(sampleAnimatableValue(channel.value, 0), sampleAnimatableValue(channel.value, channel.duration))
  for (let index = 1; index < keys.length; index++) {
    const left = keys[index - 1]!, right = keys[index]!
    for (let step = 1; step < 32; step++) values.push(sampleAnimatableValue(channel.value, keyframeLocalTime(channel.value.track, left.tick + (right.tick - left.tick) * step / 32)))
  }
  if (keys.length > 1) {
    const first = keys[0]!, last = keys.at(-1)!
    const duration = keyframeAuthoredTime(channel.value.track, channel.duration)
    const beginning = keyframeAuthoredTime(channel.value.track, 0)
    const period = last.tick - first.tick
    for (const [side, low, high] of [['before', beginning, Math.min(duration, first.tick)], ['after', Math.max(beginning, last.tick), duration]] as const) {
      if (channel.value.track.extrapolation?.[side] !== 'offset' || high <= low) continue
      for (let index = 0; index < keys.length; index++) {
        const key = keys[index]!
        const firstCycle = Math.ceil((low - key.tick) / period)
        const lastCycle = Math.floor((high - key.tick) / period)
        if (firstCycle > lastCycle) continue
        for (const cycle of [firstCycle, lastCycle]) {
          const offset = cycle * (last.value - first.value)
          values.push(key.value + offset)
          const previous = keys[index - 1]
          if (previous && (previous.interpolation ?? channel.value.track.interpolation) === 'hold') values.push(previous.value + offset)
        }
      }
    }
  }
  const minimum = Math.min(...values), maximum = Math.max(...values)
  if (!Number.isFinite(minimum) || !Number.isFinite(maximum)) return { minimum: 0, maximum: 1 }
  if (minimum === maximum) { const half = Math.max(1, Math.abs(maximum) * 0.1); return { minimum: minimum - half, maximum: maximum + half } }
  const pad = (maximum - minimum) * 0.1
  return { minimum: minimum - pad, maximum: maximum + pad }
}
export function keyframeValueY(value: number, range: KeyframeValueRange, height: number): number {
  return range.maximum > range.minimum ? (range.maximum - value) / (range.maximum - range.minimum) * height : height / 2
}
export function keyframeCurvePoints(channel: ScalarCurveChannel, zoom: number, height: number, range = keyframeValueRange(channel)): string {
  const keys = channel.value.track.keyframes
  const points: { local: number; value: number }[] = []
  for (let index = 0; index < keys.length; index++) {
    const key = keys[index]!
    const local = keyframeLocalTime(channel.value.track, key.tick)
    if (index) {
      const left = keys[index - 1]!
      const start = keyframeLocalTime(channel.value.track, left.tick)
      if ((left.interpolation ?? channel.value.track.interpolation) === 'hold') points.push({ local, value: left.value })
      else for (let step = 1; step < 32; step++) {
        const sample = start + (local - start) * step / 32
        points.push({ local: sample, value: sampleAnimatableValue(channel.value, sample) })
      }
    }
    points.push({ local, value: key.value })
  }
  return points.map((point) => `${((channel.start + point.local) / COMPOSITION_TIME_BASE * zoom).toFixed(3)},${keyframeValueY(point.value, range, height).toFixed(3)}`).join(' ')
}

export function keyframeTailPoints(channel: ScalarCurveChannel, side: 'before' | 'after', zoom: number, height: number, range = keyframeValueRange(channel)): string {
  const keys = channel.value.track.keyframes
  const endpoint = side === 'before' ? keys[0] : keys.at(-1)
  if (!endpoint) return ''
  const local = keyframeLocalTime(channel.value.track, endpoint.tick)
  const start = side === 'before' ? 0 : Math.max(0, local)
  const end = side === 'before' ? Math.min(channel.duration, local) : channel.duration
  if (end <= start) return ''
  const format = (tick: number, value: number) => `${((channel.start + tick) / COMPOSITION_TIME_BASE * zoom).toFixed(3)},${keyframeValueY(value, range, height).toFixed(3)}`
  const mode = channel.value.track.extrapolation?.[side] ?? 'hold'
  if (keys.length < 2 || mode === 'hold' || mode === 'continue') return [start, end].map(tick => format(tick, sampleAnimatableValue(channel.value, tick))).join(' ')
  const localTime = (tick: number) => keyframeLocalTime(channel.value.track, tick)
  const first = keys[0]!, last = keys.at(-1)!
  const origin = localTime(first.tick), period = localTime(last.tick) - localTime(first.tick)
  const firstCycle = Math.floor((start - origin) / period)
  const lastCycle = Math.floor((end - origin) / period)
  // Subpixel repetitions use screen-resolution samples to bound SVG work.
  if (period / COMPOSITION_TIME_BASE * zoom < 1 || (lastCycle - firstCycle + 1) * keys.length * 64 > 131_072) {
    const steps = Math.min(8192, Math.max(2, Math.ceil((end - start) / COMPOSITION_TIME_BASE * zoom)))
    return Array.from({ length: steps + 1 }, (_, index) => {
      const tick = start + (end - start) * index / steps
      return format(tick, sampleAnimatableValue(channel.value, tick))
    }).join(' ')
  }
  const base: { tick: number; value: number }[] = [{ tick: origin, value: first.value }]
  for (let index = 1; index < keys.length; index++) {
    const left = keys[index - 1]!, right = keys[index]!
    if ((left.interpolation ?? channel.value.track.interpolation) === 'hold') base.push({ tick: localTime(right.tick), value: left.value })
    else for (let step = 1; step < 32; step++) {
      const tick = localTime(left.tick + (right.tick - left.tick) * step / 32)
      base.push({ tick, value: sampleAnimatableValue(channel.value, tick) })
    }
    base.push({ tick: localTime(right.tick), value: right.value })
  }
  const points: string[] = [format(start, sampleAnimatableValue(channel.value, start))]
  for (let cycle = firstCycle; cycle <= lastCycle; cycle++) {
    const reversed = mode === 'ping_pong' && ((cycle % 2) + 2) % 2 === 1
    const offset = mode === 'offset' ? cycle * (last.value - first.value) : 0
    const transformed = base.map(point => ({
      tick: origin + cycle * period + (reversed ? period - (point.tick - origin) : point.tick - origin),
      value: point.value + offset,
    }))
    if (reversed) transformed.reverse()
    for (let index = 1; index < transformed.length; index++) {
      const left = transformed[index - 1]!, right = transformed[index]!
      if (right.tick < start || left.tick > end) continue
      const clippedStart = Math.max(start, left.tick), clippedEnd = Math.min(end, right.tick)
      const valueAt = (tick: number) => right.tick === left.tick ? right.value : left.value + (right.value - left.value) * (tick - left.tick) / (right.tick - left.tick)
      points.push(format(clippedStart, right.tick === left.tick ? left.value : valueAt(clippedStart)), format(clippedEnd, valueAt(clippedEnd)))
    }
  }
  points.push(format(end, sampleAnimatableValue(channel.value, end)))
  return points.join(' ')
}

/** World-space controls: incoming is stored unmirrored in its preceding segment. */
export function keyframeTangentPoints(channel: ScalarCurveChannel, zoom: number, height: number, range = keyframeValueRange(channel)): { tick: number; side: 'incoming' | 'outgoing'; x: number; y: number; keyX: number; keyY: number }[] {
  const keys = channel.value.track.keyframes
  const localTime = (tick: number) => keyframeLocalTime(channel.value.track, tick)
  const result: ReturnType<typeof keyframeTangentPoints> = []
  for (let i = 0; i < keys.length - 1; i++) {
    const left = keys[i]!, right = keys[i + 1]!
    const mode = segmentInterpolation(left, right, channel.value.track.interpolation)
    if (typeof mode !== 'object' || mode.kind !== 'bezier') continue
    for (const [side, control, owner] of [['outgoing',mode.p1,left], ['incoming',mode.p2,right]] as const) {
      const tick = left.tick + control[0]*(right.tick-left.tick)
      const value = left.value + control[1]*(right.value-left.value)
      result.push({ tick: owner.tick, side, x: (channel.start+localTime(tick))/COMPOSITION_TIME_BASE*zoom, y: keyframeValueY(value,range,height), keyX: (channel.start+localTime(owner.tick))/COMPOSITION_TIME_BASE*zoom, keyY: keyframeValueY(owner.value,range,height) })
    }
  }
  return result
}

/** Convert the pointer through a frozen graph axis into segment-unit controls. */
export function keyframeTangentAtPoint(channel: ScalarCurveChannel, tick: number, side: 'incoming' | 'outgoing', point: { x: number; y: number }, zoom: number, height: number, range: KeyframeValueRange): { x: number; y: number } | null {
  if (!(zoom > 0) || !Number.isFinite(zoom) || !(height > 0) || !Number.isFinite(height) || !Number.isFinite(point.x) || !Number.isFinite(point.y) || !Number.isFinite(range.minimum) || !Number.isFinite(range.maximum) || range.maximum <= range.minimum) return null
  const keys = channel.value.track.keyframes
  const owner = keys.findIndex(key => key.tick === tick)
  if (owner < 0) return null
  const left = keys[side === 'incoming' ? owner - 1 : owner]
  const right = keys[side === 'incoming' ? owner : owner + 1]
  if (!left || !right || right.tick <= left.tick) return null
  const mode = segmentInterpolation(left, right, channel.value.track.interpolation)
  if (typeof mode !== 'object' || mode.kind !== 'bezier') return null
  const pointerTick = keyframeAuthoredTime(channel.value.track, point.x / zoom * COMPOSITION_TIME_BASE - channel.start)
  const x = Math.max(0,Math.min(1,(pointerTick-left.tick)/(right.tick-left.tick)))
  const delta = right.value-left.value
  // A flat segment has no invertible value scale; preserve the stored Y.
  const current = side === 'incoming' ? mode.p2 : mode.p1
  const pointerValue = range.maximum - point.y/height*(range.maximum-range.minimum)
  const y = delta === 0 ? current[1] : (pointerValue-left.value)/delta
  return Number.isFinite(x) && Number.isFinite(y) ? { x,y } : null
}
