import type { Composition,CompositionColorKeyframe,CompositionAnimatableColor } from './types'
import { COMPOSITION_TIME_BASE } from './types'
import { findClipLocation } from './commands'
import { cloneKeyframe,cloneInterpolation } from './tangents'
import { rebaseColorAnimation } from './colorAnimation'
import { evaluateAgentBatch } from '../agent/operations'
export interface ColorClipboardGroup { readonly keys: readonly CompositionColorKeyframe[] }
function clone(key: CompositionColorKeyframe): CompositionColorKeyframe { return { ...cloneKeyframe({ ...key,value: 0 }),value: key.value } }
export function copyColorKeys(document: Composition,subjects: readonly { clipId: string;tick: number }[]): ColorClipboardGroup[] {
  if (!subjects.length || subjects.length > 4096) throw new Error('Color copy requires 1..4096 keys')
  const seen = new Set<string>(),groups = new Map<string,CompositionColorKeyframe[]>()
  for (const subject of subjects) {
    const address = JSON.stringify([subject.clipId,subject.tick])
    if (seen.has(address)) throw new Error('Duplicate color copy subject')
    seen.add(address)
    const { clip } = findClipLocation(document,subject.clipId)
    if ((clip.kind !== 'text' && clip.kind !== 'color') || clip.colorAnimation?.mode !== 'keyframes') throw new Error('Color keys required')
    const track = clip.colorAnimation.track,key = track.keyframes.find(key => key.tick === subject.tick)
    if (!key) throw new Error('Color key does not exist')
    const tick = clip.timelineStartTicks+Math.round(key.tick*COMPOSITION_TIME_BASE/track.timeBase)
    if (!Number.isSafeInteger(tick)) throw new Error('Invalid color copy time')
    const keys = groups.get(clip.id) ?? []
    keys.push({ ...clone(key),tick,interpolation: cloneInterpolation(key.interpolation ?? track.interpolation) })
    groups.set(clip.id,keys)
  }
  const first = Math.min(...[...groups.values()].flatMap(keys => keys.map(key => key.tick)))
  return [...groups.values()].map(keys => ({ keys: keys.map(key => ({ ...key,tick: key.tick-first })).sort((a,b) => a.tick-b.tick) }))
}
export function pasteColorKeys(document: Composition,groups: readonly ColorClipboardGroup[],clipIds: readonly string[],atTicks: number): { document: Composition;subjects: { clipId: string;tick: number }[] } {
  if (!Number.isSafeInteger(atTicks) || atTicks < 0 || !groups.length || groups.length !== clipIds.length || new Set(clipIds).size !== clipIds.length) throw new Error('Invalid color paste targets or time')
  const subjects: { clipId: string;tick: number }[] = []
  const operations = groups.map((group,index) => {
    if (!group.keys.length) throw new Error('Empty color clipboard group')
    const clipId = clipIds[index]!,{ clip,track: owner } = findClipLocation(document,clipId)
    if ((clip.kind !== 'text' && clip.kind !== 'color') || owner.locked || clip.locked) throw new Error('Color paste requires unlocked text or color layers')
    const copied = clip.colorAnimation && rebaseColorAnimation(clip.colorAnimation)
    const track = copied?.mode === 'keyframes' ? copied.track : { timeBase: COMPOSITION_TIME_BASE,interpolation: 'linear' as const,keyframes: [] }
    const landed = new Map(track.keyframes.map(key => [key.tick,clone(key)])),pasted = new Set<number>()
    for (const key of group.keys) {
      if (!Number.isSafeInteger(key.tick) || key.tick < 0) throw new Error('Invalid clipboard key time')
      const tick = Math.round((atTicks-clip.timelineStartTicks+key.tick)*track.timeBase/COMPOSITION_TIME_BASE)
      if (!Number.isSafeInteger(tick) || pasted.has(tick)) throw new Error('Pasted color keys collide in target clock')
      pasted.add(tick);landed.set(tick,{ ...clone(key),tick });subjects.push({ clipId,tick })
    }
    const colorAnimation: CompositionAnimatableColor = { mode: 'keyframes',track: { ...track,keyframes: [...landed.values()].sort((a,b) => a.tick-b.tick) } }
    return { op: 'update_layer' as const,clipId,patch: { colorAnimation } }
  })
  return { document: evaluateAgentBatch(document,operations),subjects }
}
