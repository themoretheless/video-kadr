import type {WireVideoEffect,WireAnimatableValue} from './types'
import {shiftAnimatableValue} from './keyframes'
/** Retain off-clip keys and segment metadata when rebasing native effect time. */
export function rebaseNativeEffects(effects:readonly WireVideoEffect[]|undefined,deltaTicks=0):readonly WireVideoEffect[]|undefined {
 function copy(value:unknown):unknown {
  if(Array.isArray(value))return value.map(copy)
  if(value===null||typeof value!=='object')return value
  const object=value as Record<string,unknown>
  if(object.mode==='constant'||(object.mode==='keyframes'&&object.track))return shiftAnimatableValue(value as WireAnimatableValue,deltaTicks)
  return Object.fromEntries(Object.entries(object).map(([key,entry])=>[key,copy(entry)]))
 }
 return effects?.map(effect=>copy(effect) as WireVideoEffect)
}
