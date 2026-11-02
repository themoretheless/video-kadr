import type { CompositionAnimatableValue, CompositionDspEffect, CompositionInterpolation } from './types'
import { cloneInterpolation, cloneKeyframe } from './tangents'
export function cloneDspCurve(curve: CompositionAnimatableValue): CompositionAnimatableValue {
  if (curve.mode === 'constant') return { ...curve }
  return { mode: 'keyframes', track: {
    ...curve.track,
    ...(curve.track.extrapolation ? { extrapolation: { ...curve.track.extrapolation } } : {}),
    interpolation: cloneInterpolation(curve.track.interpolation),
    keyframes: curve.track.keyframes.map(cloneKeyframe),
  } }
}
/** Plain authored copies also support Svelte proxies, which structuredClone rejects. */
export function cloneDspEffect(effect: CompositionDspEffect): CompositionDspEffect {
  return { ...effect,
    ...(effect.concatSourceKeys?{concatSourceKeys:Object.fromEntries(Object.entries(effect.concatSourceKeys).map(([id,keys])=>[id,keys.map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}))]))}:{}),
    ...(effect.params ? { params: { ...effect.params } } : {}),
    ...(effect.animation ? { animation: Object.fromEntries(Object.entries(effect.animation).map(([id, curve]) => [id, cloneDspCurve(curve)])) } : {}),
  }
}
export function upsertDspKey(effect: CompositionDspEffect, id: string, localTick: number, value: number): CompositionDspEffect {
  const curve = effect.animation?.[id]
  const previous = curve?.mode === 'keyframes' ? curve.track : undefined
  const timeBase = previous?.timeBase ?? 1000000
  const tick = Math.round(Math.max(0, localTick) * timeBase / 1000000)
  const keys = previous?.keyframes.map(cloneKeyframe) ?? (curve?.mode === 'constant' && tick > 0 ? [{ tick: 0, value: curve.value }] : [])
  const existing = keys.find(key => key.tick === tick)
  const keyframes = [...keys.filter(key => key.tick !== tick), { ...existing, tick, value }].sort((a,b) => a.tick - b.tick)
  return { ...effect, params: { ...effect.params, [id]: value }, animation: { ...effect.animation, [id]: { mode: 'keyframes', track: { ...previous, timeBase, interpolation: previous?.interpolation ?? 'linear', keyframes } } } }
}
export function setDspInterpolation(effect: CompositionDspEffect, id: string, interpolation: CompositionInterpolation): CompositionDspEffect {
  const curve = effect.animation?.[id]
  if (curve?.mode !== 'keyframes') return effect
  return { ...effect, animation: { ...effect.animation, [id]: { ...curve, track: { ...curve.track, interpolation: cloneInterpolation(interpolation) } } } }
}

/** Retain all keys, including off-clip handles, when rebasing local clip time. */
export function rebaseDspEffects(effects: readonly CompositionDspEffect[] | undefined, deltaTicks = 0): readonly CompositionDspEffect[] | undefined {
  return effects?.map(effect => {
    const copy = cloneDspEffect(effect)
    if (!copy.animation) return copy
    return { ...copy, animation: Object.fromEntries(Object.entries(copy.animation).map(([id, curve]) => {
      if (curve.mode === 'constant') return [id, curve]
      const delta = Math.round(deltaTicks * curve.track.timeBase / 1000000)
      return [id, { ...curve, track: { ...curve.track, keyframes: curve.track.keyframes.map(key => ({ ...key, tick: key.tick + delta })) } }]
    })) }
  })
}

export function editDspKey(effect: CompositionDspEffect, id: string, index: number, change: { localTick?: number; value?: number }): CompositionDspEffect {
  const curve = effect.animation?.[id]
  if (curve?.mode !== 'keyframes' || !curve.track.keyframes[index]) return effect
  const current = curve.track.keyframes[index]!
  const tick = change.localTick === undefined ? current.tick : Math.round(change.localTick * curve.track.timeBase / 1000000)
  const value = change.value ?? current.value
  if (!Number.isSafeInteger(tick) || !Number.isFinite(value)) return effect
  // Moving onto an existing key replaces that key with the moved key's metadata/value.
  const keyframes = curve.track.keyframes.filter((key,i) => i !== index && key.tick !== tick).map(cloneKeyframe)
  keyframes.push({ ...cloneKeyframe(current), tick, value }); keyframes.sort((a,b) => a.tick - b.tick)
  return { ...effect, animation: { ...effect.animation, [id]: { ...curve, track: { ...curve.track, keyframes } } } }
}
export function removeDspKey(effect: CompositionDspEffect, id: string, index: number): CompositionDspEffect {
  const curve = effect.animation?.[id]
  if (curve?.mode !== 'keyframes') return effect
  const keyframes = curve.track.keyframes.filter((_,i) => i !== index).map(cloneKeyframe)
  const animation = { ...effect.animation }
  if (keyframes.length) animation[id] = { ...curve, track: { ...curve.track, keyframes } }
  else delete animation[id]
  return { ...effect, animation }
}
