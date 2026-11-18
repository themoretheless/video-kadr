import { bezierBounds, isBezierInterpolation, sampleBezier, sliceTimingBezier } from './bezier'
import { isElasticInterpolation, sampleElastic } from './elastic'
import { sampleAnimatableValue } from './keyframes'
import { keyframeAuthoredTick } from './keyframeClock'
import type {CompositionElasticInterpolation, CompositionExtrapolationMode, CompositionKeyframe, CompositionInterpolation, WireAnimatableValue, WireConcatParamKey, WireConcatPreparation, WireTransform} from './types'
export type ConcatPreparationEdit={kind:'edge';index:number;value:number}|{kind:'flip';axis:'horizontal'|'vertical';value:boolean}|{kind:'reset-crop'}|{kind:'enable-fit'}
/** Source Crop::tidy keeps at least one tenth of each image axis. */
export function editConcatPreparation(before:WireConcatPreparation,edit:ConcatPreparationEdit):WireConcatPreparation{
 const crop=[...before.crop] as [number,number,number,number]
 const result={...before,crop,canvas:[...before.canvas] as [number,number]}
 if(edit.kind==='edge'){
  if(!Number.isInteger(edit.index)||edit.index<0||edit.index>3||!Number.isFinite(edit.value))throw new Error('Некорректная граница обрезки Concat')
  crop[edit.index]=Math.max(0,Math.min(0.9,edit.value))
  if(crop[0]+crop[2]>0.9)crop[2]=Math.max(0,0.9-crop[0])
  if(crop[1]+crop[3]>0.9)crop[3]=Math.max(0,0.9-crop[1])
 }else if(edit.kind==='flip'){
  if(typeof edit.value!=='boolean'||!['horizontal','vertical'].includes(edit.axis))throw new Error('Некорректное отражение Concat')
  if(edit.axis==='horizontal')result.flipH=edit.value;else result.flipV=edit.value
 }else if(edit.kind==='reset-crop')result.crop=[0,0,0,0]
 else throw new Error('Неизвестное изменение обрезки')
 return result
}

export function applyConcatFit(
  transform: WireTransform,
  canvas: {width: number; height: number},
  source: {width: number; height: number},
  animation?: {durationTicks: number; opacity: WireAnimatableValue},
): WireTransform {
  const {width, height} = canvas
  if (width > 3840 || height > 2160) throw new Error('Подгонка Concat поддерживает холст до 3840 × 2160')
  const fit = Math.min(width / source.width, height / source.height)
  if ([transform.scaleX, transform.scaleY, transform.x, transform.y, transform.rotationDegrees, animation?.opacity].some(value => value?.mode === 'keyframes')) {
    if (!animation) throw new Error('Не задана длительность анимации клипа')
    return applyAnimatedConcatFit(transform, [width, height], fit, animation)
  }
  if (transform.scaleX.mode !== 'constant' || transform.scaleY.mode !== 'constant') {
    throw new Error('Не задана длительность анимации клипа')
  }
  const scaleX = transform.scaleX.value / fit
  const scaleY = transform.scaleY.value / fit
  if (![scaleX, scaleY].every(value => value >= 0.1 && value <= 10)) {
    throw new Error('Масштаб клипа выходит за пределы подгонки Concat')
  }
  return {
    ...transform,
    scaleX: { mode: 'constant', value: scaleX },
    scaleY: { mode: 'constant', value: scaleY },
    concatPreparation: { crop: [0, 0, 0, 0], canvas: [width, height] },
  }
}

const linearEase = [0, 0, 1, 1] as const
const cubicIn = [1 / 3, 0, 2 / 3, 0] as const
const cubicOut = [1 / 3, 1, 2 / 3, 1] as const

const quadraticIn = [1 / 3, 0, 2 / 3, 1 / 3] as const
const quadraticOut = [1 / 3, 2 / 3, 2 / 3, 1] as const
type ConcatSegment = {at: number; progress: number; ease: readonly [number, number, number, number]}

// Split each parabola at its minimum so adjacent keys have different values.
// Concat's interpolation cannot express a dip between equal endpoints.
const bounceOut: readonly ConcatSegment[] = [
  {at: 4 / 11, progress: 1, ease: quadraticIn},
  {at: 6 / 11, progress: 0.75, ease: quadraticOut},
  {at: 8 / 11, progress: 1, ease: quadraticIn},
  {at: 9 / 11, progress: 0.9375, ease: quadraticOut},
  {at: 10 / 11, progress: 1, ease: quadraticIn},
  {at: 10.5 / 11, progress: 0.984375, ease: quadraticOut},
  {at: 1, progress: 1, ease: quadraticIn},
]
const bounceIn: readonly ConcatSegment[] = bounceOut.map((_, index) => {
  const reverse = bounceOut.length - 1 - index
  const end = bounceOut[reverse - 1]
  return {
    at: 1 - (end?.at ?? 0),
    progress: 1 - (end?.progress ?? 0),
    ease: bounceOut[reverse]!.ease === quadraticIn ? quadraticOut : quadraticIn,
  }
})
const bounceInOut = [
  ...bounceIn.map(segment => ({...segment, at: segment.at / 2, progress: segment.progress / 2})),
  ...bounceOut.map(segment => ({...segment, at: 0.5 + segment.at / 2, progress: 0.5 + segment.progress / 2})),
]

// The linear interpolation error on an interval of width h is at most
// max(|f''|) * h² / 8. Elastic is a damped sine on each side of its midpoint.
const elasticProgressError = 0.0001
function elasticSegments(curve: CompositionElasticInterpolation, ticks: number): ConcatSegment[] {
  const segments: ConcatSegment[] = []
  if (ticks === 1) return [{at:1, progress:1, ease:linearEase}]
  const omega = 2 * Math.PI / curve.period
  const damping = 10 * Math.LN2
  const secondDerivative = curve.amplitude * (damping ** 2 + omega ** 2)
  if (!Number.isFinite(secondDerivative)) throw new Error('Период Elastic слишком мал для экспорта Concat')
  const interval = (a: number, b: number, direction: 'in' | 'out') => {
    const split = curve.dir === 'in_out'
    const exponent = direction === 'in'
      ? 10 * ((split ? 2 * b : b) - 1)
      : -10 * (split ? 2 * a - 1 : a)
    const bound = secondDerivative * 2 ** exponent * (split ? 2 : 1)
    if (bound * (b - a) ** 2 / 8 > elasticProgressError) {
      const mid = (a + b) / 2
      if (mid === a || mid === b) throw new Error('Период Elastic слишком мал для экспорта Concat')
      interval(a, mid, direction)
      interval(mid, b, direction)
    } else {
      if (segments.length >= 4095) throw new Error('Превышен предел ключей Elastic для Concat')
      segments.push({at: b, progress: sampleElastic(curve, b), ease: linearEase})
    }
  }
  const first = 1 / ticks, last = (ticks - 1) / ticks
  segments.push({at:first, progress:sampleElastic(curve,first), ease:linearEase})
  if (first < last) {
    if (curve.dir === 'in_out') {
      interval(first, 0.5, 'in')
      interval(0.5, last, 'out')
    } else {
      interval(first, last, curve.dir)
    }
  }
  segments.push({at:1, progress:1, ease:linearEase})
  return segments
}

function concatSegments(left: CompositionKeyframe, right: CompositionKeyframe, fallback: CompositionInterpolation): readonly ConcatSegment[] {
  const interpolation = left.interpolation ?? fallback
  const single = (ease: ConcatSegment['ease']) => [{at: 1, progress: 1, ease}]
  if (interpolation === 'linear') return single(linearEase)
  if (interpolation === 'ease_in') return single(cubicIn)
  if (interpolation === 'ease_out') return single(cubicOut)
  if (interpolation === 'ease_in_out' || interpolation === 'ease_in_out_cubic') {
    return [{at: 0.5, progress: 0.5, ease: cubicIn}, {at: 1, progress: 1, ease: cubicOut}]
  }
  if (interpolation === 'bounce_in') return bounceIn
  if (interpolation === 'bounce_out') return bounceOut
  if (interpolation === 'bounce_in_out') return bounceInOut
  if (isElasticInterpolation(interpolation)) {
    return elasticSegments(interpolation, right.tick - left.tick)
  }
  if (isBezierInterpolation(interpolation)) {
    const outgoing = left.tangents?.outgoing
    const incoming = right.tangents?.incoming
    const p1 = outgoing ? [outgoing.x, outgoing.y] as const : interpolation.p1
    const p2 = incoming ? [incoming.x, incoming.y] as const : interpolation.p2
    if (isBezierInterpolation({kind: 'bezier', p1, p2})) return single([...p1, ...p2])
  }
  throw new Error('Concat не может сохранить этот тип интерполяции')
}

function clipConcatRun(run: WireConcatParamKey[], from: number, to: number): WireConcatParamKey[] {
  if (run.length === 1) return run.filter(key => key.at >= from && key.at <= to)
  if (from === to) {
    const index = run.findIndex(key => key.at >= from)
    const right = run[index]!
    if (index === 0 || right.at === from) return [{...right, at:from, ease:linearEase}]
    const left = run[index-1]!
    const [x1,y1,x2,y2] = right.ease as ConcatSegment['ease']
    const progress = sampleBezier({kind:'bezier',p1:[x1,y1],p2:[x2,y2]}, (from-left.at)/(right.at-left.at))
    return [{at:from, value:left.value+(right.value-left.value)*progress, ease:linearEase}]
  }
  const clipped: WireConcatParamKey[] = []
  for (let i = 1; i < run.length; i++) {
    const a = run[i-1]!, b = run[i]!
    const start = Math.max(from, a.at), end = Math.min(to, b.at)
    if (start >= end) continue
    if (start === a.at && end === b.at) {
      if (!clipped.length) clipped.push(a)
      clipped.push(b)
      continue
    }
    const [x1,y1,x2,y2] = b.ease as ConcatSegment['ease']
    const sub = sliceTimingBezier({kind:'bezier',p1:[x1,y1],p2:[x2,y2]},
      (start-a.at)/(b.at-a.at), (end-a.at)/(b.at-a.at))
    const delta = b.value-a.value
    if (delta !== 0 && !sub.ease.every(Number.isFinite)) throw new Error('Граница клипа обрезает непереносимый разворот кривой')
    if (!clipped.length) clipped.push({at:start,value:a.value+delta*sub.from,ease:linearEase})
    clipped.push({at:end,value:a.value+delta*sub.to,ease:delta === 0 ? linearEase : sub.ease})
  }
  return clipped
}

function expandConcatCycles(
  keys: WireConcatParamKey[],
  duration: number,
  before: CompositionExtrapolationMode | undefined,
  after: CompositionExtrapolationMode | undefined,
): WireConcatParamKey[] {
  const repeat = (mode: CompositionExtrapolationMode | undefined) =>
    mode === 'ping_pong' || mode === 'loop' || mode === 'offset'
  if (keys.length < 2 || (!repeat(before) && !repeat(after))) {
    return keys[0]!.at >= 0 && keys.at(-1)!.at <= 1 ? keys : clipConcatRun(keys, 0, 1)
  }
  const first = Math.round(keys[0]!.at * duration)
  const last = Math.round(keys.at(-1)!.at * duration)
  const period = last - first
  const shift = keys.at(-1)!.value - keys[0]!.value
  const firstVisible = last === 0 ? 0 : Math.floor(-first / period)
  const lastVisible = Math.floor((duration - first) / period)
  const start = repeat(before) ? firstVisible : Math.max(0, firstVisible)
  const end = repeat(after) ? lastVisible : Math.min(0, lastVisible)
  if (start > end) return []
  if ((end - start + 1) * (keys.length - 1) + 1 > 4096) throw new Error('Превышен предел ключей Concat')
  const result: WireConcatParamKey[] = []
  for (let cycle = start; cycle <= end; cycle++) {
    const mode = cycle < 0 ? before : after
    const reverse = mode === 'ping_pong' && Math.abs(cycle % 2) === 1
    const offset = mode === 'offset' ? cycle * shift : 0
    const discontinuous = cycle !== 0 && mode === 'loop' && shift !== 0
    const cycleStart = first + cycle * period, cycleEnd = last + cycle * period
    // The last authored key owns its tick. Other loop boundaries sample the
    // start of the next cycle, so keep the old curve through the previous tick.
    const from = Math.max(0, cycleStart + (discontinuous && cycle === 1 ? 1 : 0))
    const to = Math.min(duration, cycleEnd - (discontinuous ? 1 : 0))
    if (from > to) continue
    const run = keys.map((_, i) => {
      const index = reverse ? keys.length - i - 1 : i
      const key = keys[index]!
      let ease = key.ease as ConcatSegment['ease']
      if (reverse && i > 0) {
        const [x1,y1,x2,y2] = keys[index+1]!.ease as ConcatSegment['ease']
        ease = [1-x2, 1-y2, 1-x1, 1-y1]
      }
      const at = i === 0 ? cycleStart / duration : i === keys.length - 1 ? cycleEnd / duration
        : cycle * period / duration + (reverse ? first / duration + (last / duration - key.at) : key.at)
      return {...key, at, value: key.value + offset, ease}
    })
    const clipped = clipConcatRun(run, from / duration, to / duration)
    if (result.at(-1)?.at === clipped[0]?.at) clipped.shift()
    result.push(...clipped)
    if (result.length > 4096) throw new Error('Превышен предел ключей Concat')
  }
  return result
}

function sampleConcatClock(
  channel: Extract<WireAnimatableValue, {mode:'keyframes'}>,
  duration: number,
  normalize: (value: number) => number,
) {
  const {track} = channel
  const base = normalize(sampleAnimatableValue(channel, 0))
  const keys: WireConcatParamKey[] = [{at:0, value:base, ease:linearEase}]
  if (track.keyframes.length === 1) return {base, keys}
  const first = track.keyframes[0]!.tick, last = track.keyframes.at(-1)!.tick
  const initialTick = keyframeAuthoredTick(track, 0)
  let finalTick = keyframeAuthoredTick(track, duration)
  const holdsBefore = (track.extrapolation?.before ?? 'hold') === 'hold'
  const holdsAfter = (track.extrapolation?.after ?? 'hold') === 'hold'
  if (holdsBefore && finalTick < first) return {base, keys}
  if (holdsAfter) finalTick = Math.min(finalTick, last)

  // Find the first local microsecond that samples beyond an authored tick.
  const firstAfter = (tick: number, start: number) => {
    let lo = start, hi = duration
    while (lo < hi) {
      const mid = lo + Math.floor((hi - lo) / 2)
      if (keyframeAuthoredTick(track, mid) <= tick) lo = mid + 1
      else hi = mid
    }
    return lo
  }

  let local = holdsBefore && initialTick < first ? firstAfter(first - 1, 1) - 1 : 0
  for (let tick = keyframeAuthoredTick(track, local), steps = 0; tick < finalTick; steps++) {
    if (steps >= 4095 || keys.length >= 4095) throw new Error('Сетка времени превышает предел ключей Concat')
    local = firstAfter(tick, local + 1)
    tick = keyframeAuthoredTick(track, local)
    const value = normalize(sampleAnimatableValue(channel, local))
    const previous = keys.at(-1)!
    if (value === previous.value) continue
    const before = (local - 1) / duration
    if (before > previous.at) keys.push({at:before, value:previous.value, ease:linearEase})
    keys.push({at:local/duration, value, ease:linearEase})
  }
  return {base, keys}
}

function readConcatChannel(channel: WireAnimatableValue, duration: number, divisor: number, min: number, max: number) {
  const normalize = (raw: number) => {
    const value = raw / divisor
    if (!(value >= min && value <= max)) throw new Error('Значение ключа выходит за пределы Concat')
    return value
  }
  if (channel.mode === 'constant') return {base: normalize(channel.value), keys: undefined}
  const {track} = channel
  if (track.keyframes.length === 0) throw new Error('В анимации нет ключей')
  if (track.sampleClock || track.timeBase !== 1_000_000) {
    return sampleConcatClock(channel, duration, normalize)
  }
  const keys: WireConcatParamKey[] = []
  for (const [index, key] of track.keyframes.entries()) {
    const left = track.keyframes[index - 1]
    if (!Number.isSafeInteger(key.tick) || (left && key.tick <= left.tick)) {
      throw new Error('Некорректные времена ключей Concat')
    }
    const at = key.tick / duration
    const value = (at >= 0 && at <= 1) ? normalize(key.value) : key.value / divisor
    const previous = keys.at(-1)
    if (!left || !previous) {
      keys.push({at, value, ease: linearEase})
      continue
    }
    if ((left.interpolation ?? track.interpolation) === 'hold') {
      // Concat interpolates between keys. Keep the value through the last
      // native microsecond before the jump, then place the new value on it.
      if (left.tick + 1 < key.tick && previous.value !== value) {
        keys.push({at: (key.tick - 1) / duration, value: previous.value, ease: linearEase})
      }
      keys.push({at, value, ease: linearEase})
      continue
    }
    const segments = concatSegments(left, key, track.interpolation)
    const delta = value - previous.value
    for (const segment of segments) {
      keys.push({
        at: segment.at === 1 ? at : previous.at + (at - previous.at) * segment.at,
        value: segment.progress === 1 ? value : previous.value + delta * segment.progress,
        ease: segment.ease,
      })
    }
  }
  const expanded = expandConcatCycles(keys, duration, track.extrapolation?.before, track.extrapolation?.after)
  const firstVisible = normalize(sampleAnimatableValue(channel, 0))
  if (!expanded.length) {
    const final = normalize(sampleAnimatableValue(channel, duration))
    const endpoints = [{at: 0, value: firstVisible, ease: linearEase}]
    if (final !== firstVisible) endpoints.push({at: 1, value: final, ease: linearEase})
    return {base: firstVisible, keys: endpoints}
  }
  const base = keys[0]!.at >= 0 && keys[0]!.at <= 1 ? keys[0]!.value : firstVisible
  for (const [index, key] of expanded.entries()) {
    const previous = expanded[index - 1]
    if (!(key.value >= min && key.value <= max)) throw new Error('Кривая выходит за пределы Concat')
    if (!previous || key.ease === linearEase) continue
    const [x1,y1,x2,y2] = key.ease as ConcatSegment['ease']
    const [lo, hi] = bezierBounds({kind:'bezier',p1:[x1,y1],p2:[x2,y2]})
    if (![lo, hi].every(progress => {
      const sample = previous.value + (key.value - previous.value) * progress
      return sample >= min && sample <= max
    })) throw new Error('Кривая выходит за пределы Concat')
  }
  if (track.extrapolation?.before === 'continue' && expanded[0]!.at > 0) {
    expanded.unshift({at: 0, value: normalize(sampleAnimatableValue(channel, 0)), ease: linearEase})
  }
  if (track.extrapolation?.after === 'continue' && expanded.at(-1)!.at < 1) {
    expanded.push({at: 1, value: normalize(sampleAnimatableValue(channel, duration)), ease: linearEase})
  }
  if (expanded.length > 4096) throw new Error('Превышен предел ключей Concat')
  return {base, keys: expanded}
}

function isLinearConcatKey(key: WireConcatParamKey) {
  return Array.isArray(key.ease) && key.ease[0] === key.ease[1] && key.ease[2] === key.ease[3]
}

export function convertConcatGain(channel: WireAnimatableValue, duration: number) {
  if (channel.mode !== 'keyframes' || !Number.isSafeInteger(duration) || duration <= 0) {
    throw new Error('Не удалось определить ключи или длительность звука для Concat')
  }
  const {base, keys} = readConcatChannel(channel, duration, 1, 0, 16)
  if (!keys?.length) throw new Error('Не удалось определить ключи громкости для Concat')
  return {
    gain: {mode: 'constant' as const, value: base},
    keys: keys.map(key => isLinearConcatKey(key) ? {at: key.at, value: key.value} : key),
  }
}

function withoutRedundantLinearKeys(keys: WireConcatParamKey[] | undefined) {
  if (!keys) return undefined
  const result: WireConcatParamKey[] = []
  for (const right of keys) {
    const middle = result.at(-1)
    const left = result.at(-2)
    if (left && middle && isLinearConcatKey(middle) && isLinearConcatKey(right)
      && middle.value === left.value + (right.value - left.value) * ((middle.at - left.at) / (right.at - left.at))) {
      result.pop()
    }
    result.push(right)
  }
  return result
}

function applyAnimatedConcatFit(
  transform: WireTransform,
  canvas: [number, number],
  fit: number,
  {durationTicks, opacity}: {durationTicks: number; opacity: WireAnimatableValue},
): WireTransform {
  if (transform.concatAnimation || !Number.isSafeInteger(durationTicks) || durationTicks <= 0) {
    throw new Error('Не удалось определить ключи или длительность клипа для Concat')
  }
  const a = readConcatChannel(transform.scaleX, durationTicks, fit, 0.05, 8)
  // Concat has a single scale curve and two constant stretch factors.
  const b = readConcatChannel(transform.scaleY, durationTicks, fit, 0.005, 80)
  const stretchY = b.base / a.base
  const horizontal = withoutRedundantLinearKeys(a.keys)
  const vertical = withoutRedundantLinearKeys(b.keys)
  if (!(stretchY >= 0.1 && stretchY <= 10) || Boolean(horizontal) !== Boolean(vertical)
    || (horizontal && (horizontal.length !== vertical?.length || horizontal.some((key, index) =>
      key.at !== vertical?.[index]?.at || Math.abs(vertical[index]!.value - key.value * stretchY) > 1e-12
      || JSON.stringify(key.ease) !== JSON.stringify(vertical[index]!.ease))))) {
    throw new Error('Анимация масштаба Concat должна сохранять пропорции')
  }
  const x = readConcatChannel(transform.x, durationTicks, canvas[0], -3, 3)
  const y = readConcatChannel(transform.y, durationTicks, canvas[1], -3, 3)
  const rotation = readConcatChannel(transform.rotationDegrees, durationTicks, 1, -Number.MAX_VALUE, Number.MAX_VALUE)
  const alpha = readConcatChannel(opacity, durationTicks, 1, 0, 1)
  const constant = (value: number) => ({mode: 'constant' as const, value})
  return {
    ...transform,
    x: constant(x.base * canvas[0]), y: constant(y.base * canvas[1]),
    rotationDegrees: constant(rotation.base),
    scaleX: constant(a.base), scaleY: constant(a.base * stretchY),
    concatPreparation: {crop: [0, 0, 0, 0], canvas},
    concatAnimation: {
      durationTicks, keys: {scale: a.keys, offsetX: x.keys, offsetY: y.keys, rotation: rotation.keys, opacity: alpha.keys},
      canvas, stretch: [1, stretchY],
      rest: {scaleX: a.base, scaleY: a.base, x: x.base, y: y.base, rotationDegrees: rotation.base, opacity: alpha.base},
    },
  }
}
