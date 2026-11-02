import type { NativeConversionTrack,NativeConversionSegment,NativePositionConversionRequest } from '../api'
import type { CompositionAnimatableValue,CompositionPositionAnimation,CompositionInterpolation } from './types'
import { segmentInterpolation } from './tangents'
import { canonicalSampleClock } from './keyframeClock'
const extrapolation = { hold: 'Hold',continue: 'Continue',loop: 'Loop',ping_pong: 'PingPong',offset: 'Offset' } as const
const direction = { in: 'In',out: 'Out',in_out: 'InOut' } as const
/** Packs authored scalar semantics; fitting and error measurement remain native. */
export function packConversionTrack(value: CompositionAnimatableValue): NativeConversionTrack {
  const keys: NativeConversionTrack['keys'][number][] = value.mode === 'constant'
    ? [{ t_us: 0,value: value.value,in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }]
    : value.track.keyframes.map(key => ({ t_us: key.tick*1_000_000/value.track.timeBase,value: key.value,in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }))
  const set = (index: number,mode: CompositionInterpolation) => {
    let segment: NativeConversionSegment
    let p1: readonly [number,number] = [1/3,1/3],p2: readonly [number,number] = [2/3,2/3]
    if (typeof mode === 'object') {
      if (mode.kind === 'bezier') { segment = { kind: 'Spline' };p1 = mode.p1;p2 = mode.p2 }
      else segment = { kind: 'Elastic',dir: direction[mode.dir],amplitude: mode.amplitude,period: mode.period }
    } else if (mode === 'hold') segment = { kind: 'Hold' }
    else if (mode === 'bounce_in' || mode === 'bounce_out' || mode === 'bounce_in_out') segment = { kind: 'Bounce',dir: mode === 'bounce_in' ? 'In' : mode === 'bounce_out' ? 'Out' : 'InOut' }
    else if (mode === 'ease_in' || mode === 'ease_out') { segment = { kind: 'Spline' };p1 = [1/3,mode === 'ease_in' ? 0 : 1];p2 = [2/3,mode === 'ease_in' ? 0 : 1] }
    else if (mode === 'linear') segment = { kind: 'Linear' }
    else throw new Error('Piecewise cubic easing requires split packing')
    keys[index] = { ...keys[index]!,segment,out: [...p1] }
    if (keys[index+1]) keys[index+1] = { ...keys[index+1]!,in_: [...p2] }
  }
  if (value.mode === 'keyframes') {
    let packed = 0
    for (let index = 0;index < value.track.keyframes.length-1;index++,packed++) {
      const mode = segmentInterpolation(value.track.keyframes[index]!,value.track.keyframes[index+1]!,value.track.interpolation)
      if (mode === 'ease_in_out' || mode === 'ease_in_out_cubic') {
        const left = keys[packed]!,right = keys[packed+1]!
        keys.splice(packed+1,0,{ t_us: (left.t_us+right.t_us)/2,value: (left.value+right.value)/2,in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } })
        set(packed,'ease_in');set(++packed,'ease_out')
      } else set(packed,mode)
    }
  }
  const sides = value.mode === 'keyframes' ? value.track.extrapolation : undefined
  const sampleGrid = value.mode === 'keyframes' ? { timeBase:value.track.timeBase,...canonicalSampleClock(value.track.sampleClock ?? {rateNum:1,rateDen:1,offsetNum:0}) } : undefined
  return { keys,...(sampleGrid ? {sampleGrid} : {}),extrapolate: { before: extrapolation[sides?.before ?? 'hold'],after: extrapolation[sides?.after ?? 'hold'] } }
}
export function packPositionConversion(position: CompositionPositionAnimation,options: NativePositionConversionRequest['options']): NativePositionConversionRequest {
  if (position.mode === 'xy') return { nodes: null,tracks: [packConversionTrack(position.x),packConversionTrack(position.y)],options: { ...options } }
  return { nodes: position.path.nodes.map(node => ({ point: { ...node.point },incoming: { ...node.inHandle },outgoing: { ...node.outHandle },cubic: node.segment === 'cubic' })),tracks: [packConversionTrack(position.progress)],options: { ...options } }
}

export function unpackConversionTrack(track: NativeConversionTrack): CompositionAnimatableValue {
  if (!track.keys.length) throw new Error('Native conversion returned no keys')
  if (track.keys.length === 1) return { mode: 'constant',value: track.keys[0]!.value }
  const grid = track.sampleGrid
  const clock = grid ? canonicalSampleClock({rateNum:grid.rateNum,rateDen:grid.rateDen,offsetNum:grid.offsetNum}) : undefined
  const timeBase = grid?.timeBase ?? 1_000_000
  if (!Number.isSafeInteger(timeBase) || timeBase < 1 || timeBase > 100_000_000) throw new Error('Invalid conversion grid')
  const sides = { Hold: 'hold',Continue: 'continue',Loop: 'loop',PingPong: 'ping_pong',Offset: 'offset' } as const
  return { mode: 'keyframes',track: { timeBase,...(clock ? {sampleClock:clock} : {}),interpolation: 'linear',extrapolation: { before: sides[track.extrapolate.before],after: sides[track.extrapolate.after] },keyframes: track.keys.map((key,index) => {
    const coordinate = grid ? key.t_us * timeBase / 1_000_000 : key.t_us
    const tick = Math.round(coordinate)
    if (!Number.isSafeInteger(tick) || Math.abs(coordinate-tick)>1e-7) throw new Error('Native conversion time is outside canonical tick grid')
    let interpolation: CompositionInterpolation
    if (key.segment.kind === 'Spline') interpolation = { kind: 'bezier',p1: [...key.out],p2: [...(track.keys[index+1]?.in_ ?? [2/3,2/3])] }
    else if (key.segment.kind === 'Elastic') interpolation = { kind: 'elastic',dir: key.segment.dir === 'In' ? 'in' : key.segment.dir === 'Out' ? 'out' : 'in_out',amplitude: key.segment.amplitude,period: key.segment.period }
    else if (key.segment.kind === 'Bounce') interpolation = key.segment.dir === 'In' ? 'bounce_in' : key.segment.dir === 'Out' ? 'bounce_out' : 'bounce_in_out'
    else interpolation = key.segment.kind === 'Hold' ? 'hold' : 'linear'
    return { tick,value: key.value,interpolation,tangents: { incoming: { x: key.in_[0],y: key.in_[1],mode: 'free' },outgoing: { x: key.out[0],y: key.out[1],mode: 'free' },continuity: 'broken' } }
  }) } }
}
export function unpackPositionConversion(result: import('../api').NativePositionConversionResult,id: (index: number) => string,allowOutsideTolerance = false): CompositionPositionAnimation {
  if ((!result.withinTolerance && !allowOutsideTolerance) || !Number.isFinite(result.maxErrorPx) || result.maxErrorPx < 0) throw new Error('Conversion target tolerance was not achieved')
  if (result.nodes !== null) {
    if (result.tracks.length !== 1 || !result.nodes.length) throw new Error('Invalid native path conversion result')
    return { mode: 'path',path: { nodes: result.nodes.map((node,index) => ({ id: id(index),point: { ...node.point },inHandle: { ...node.incoming },outHandle: { ...node.outgoing },segment: node.cubic ? 'cubic' : 'line',tangentMode: 'corner' })) },progress: unpackConversionTrack(result.tracks[0]!) }
  }
  if (result.tracks.length !== 2) throw new Error('Invalid native XY conversion result')
  return { mode: 'xy',x: unpackConversionTrack(result.tracks[0]!),y: unpackConversionTrack(result.tracks[1]!) }
}
