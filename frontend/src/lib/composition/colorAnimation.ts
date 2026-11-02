import type { CompositionAnimatableColor } from './types'
import { COMPOSITION_TIME_BASE } from './types'
import { cloneInterpolation,cloneKeyframe } from './tangents'
import type { NativeColor,NativeColorTrack } from '../api'
import { packConversionTrack } from './positionConversion'

export function colorToNative(value: string): NativeColor {
  if (!/^#[0-9a-f]{6}([0-9a-f]{2})?$/i.test(value)) throw new Error('Color must be hex RGB or RGBA')
  return { r: parseInt(value.slice(1,3),16),g: parseInt(value.slice(3,5),16),b: parseInt(value.slice(5,7),16),a: value.length === 9 ? parseInt(value.slice(7,9),16) : 255 }
}
export function nativeColorToHex(value: NativeColor): string {
  if (!value || typeof value !== 'object' || Object.keys(value).some(key => !['r','g','b','a'].includes(key))) throw new Error('Color must be an {r,g,b,a} object')
  const channels = [value.r,value.g,value.b,value.a]
  if (channels.some(channel => !Number.isInteger(channel) || channel < 0 || channel > 255)) throw new Error('Native color has an invalid channel')
  return `#${channels.map(channel => channel.toString(16).padStart(2,'0')).join('')}`
}

/** Transport only: scalar timing curves pack as before, RGBA is never fitted in JS. */
export function packColorAnimation(value: CompositionAnimatableColor): NativeColorTrack {
  if (value.mode === 'constant') return { keys: [{ t_us: 0,value: colorToNative(value.value),in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }],extrapolate: { before: 'Hold',after: 'Hold' } }
  const track = value.track
  if ('sampleClock' in track) throw new Error('Рациональные часы цвета пока не поддерживаются')
  const keys: NativeColorTrack['keys'][number][] = track.keyframes.map(key => ({ t_us: key.tick*1_000_000/track.timeBase,value: colorToNative(key.value),...(key.tangents?{tangent_modes:{incoming:key.tangents.incoming.mode,outgoing:key.tangents.outgoing.mode,continuity:key.tangents.continuity}}:{}),in_: [2/3,2/3],out: [1/3,1/3],segment: { kind: 'Linear' } }))
  let extrapolate: NativeColorTrack['extrapolate'] = { before: 'Hold',after: 'Hold' }
  for (let i = 0;i < track.keyframes.length;i++) {
    const left = track.keyframes[i]!,right = track.keyframes[i+1]
    const mode = left.interpolation ?? track.interpolation
    const legacy = mode === 'ease_in_out' || mode === 'ease_in_out_cubic'
    const packed = packConversionTrack({ mode: 'keyframes',track: { ...track,interpolation: legacy ? 'linear' : track.interpolation,keyframes: [{ ...left,value: 0,...(legacy ? { interpolation: 'linear' as const } : {}) },...(right ? [{ ...right,value: 1 }] : [])] } })
    extrapolate = packed.extrapolate
    if (!right) continue
    keys[i] = { ...keys[i]!,segment: legacy ? { kind: 'Spline' } : packed.keys[0]!.segment,out: legacy ? [1/3,0] : [...packed.keys[0]!.out],...(legacy ? { legacy_curve: 'ease_in_out' } : {}) }
    keys[i+1] = { ...keys[i+1]!,in_: legacy ? [2/3,1] : [...packed.keys[1]!.in_] }
  }
  return { keys,extrapolate }
}

/** Color has no scalar slope projection; preserve authored temporal controls. */
export function rebaseColorAnimation(value: CompositionAnimatableColor,deltaTicks = 0): CompositionAnimatableColor {
  if (value.mode === 'constant') return { ...value }
  const delta = Math.round(deltaTicks*value.track.timeBase/COMPOSITION_TIME_BASE)
  return { mode: 'keyframes',track: { ...value.track,
    interpolation: cloneInterpolation(value.track.interpolation),
    ...(value.track.extrapolation ? { extrapolation: { ...value.track.extrapolation } } : {}),
    keyframes: value.track.keyframes.map(key => {
      const tick = key.tick+delta
      if (!Number.isSafeInteger(tick)) throw new Error('Color key time is outside the safe tick grid')
      return { ...cloneKeyframe({ ...key,value: 0 }),tick,value: key.value }
    }),
  } }
}

export function resolvedColorTangents(value: Extract<CompositionAnimatableColor,{ mode: 'keyframes' }>,index: number): import('./types').CompositionKeyframeTangents {
  const key = value.track.keyframes[index]!
  if (key.tangents) return key.tangents
  const previous = value.track.keyframes[index-1]
  const incoming = previous ? previous.interpolation ?? value.track.interpolation : undefined
  const outgoing = index+1 < value.track.keyframes.length ? key.interpolation ?? value.track.interpolation : undefined
  const p2 = typeof incoming === 'object' && incoming.kind === 'bezier' ? incoming.p2 : [2/3,2/3]
  const p1 = typeof outgoing === 'object' && outgoing.kind === 'bezier' ? outgoing.p1 : [1/3,1/3]
  return { incoming: { x: p2[0]!,y: p2[1]!,mode: 'free' },outgoing: { x: p1[0]!,y: p1[1]!,mode: 'free' },continuity: 'broken' }
}

/** Pinned scalarless write solve: Auto on a Spline is identity, Smooth does not couple color sides. */
export function normalizeColorAnimation(value: CompositionAnimatableColor): CompositionAnimatableColor {
  const copy = rebaseColorAnimation(value)
  if (copy.mode !== 'keyframes') return copy
  const spline = (index: number) => {
    const key = copy.track.keyframes[index]
    const mode = key?.interpolation ?? copy.track.interpolation
    return typeof mode === 'object' && mode.kind === 'bezier'
  }
  return { ...copy,track: { ...copy.track,keyframes: copy.track.keyframes.map((key,index) => {
    if (!key.tangents) return key
    return { ...key,tangents: { ...key.tangents,
      incoming: key.tangents.incoming.mode === 'auto' && index > 0 && spline(index-1) ? { x: 2/3,y: 2/3,mode: 'auto' } : key.tangents.incoming,
      outgoing: key.tangents.outgoing.mode === 'auto' && index+1 < copy.track.keyframes.length && spline(index) ? { x: 1/3,y: 1/3,mode: 'auto' } : key.tangents.outgoing,
    } }
  }) } }
}
