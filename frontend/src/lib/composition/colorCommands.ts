import type { CompositionAnimatableColor,Composition,TextClip,ColorClip } from './types'
import { COMPOSITION_TIME_BASE,clipDurationTicks } from './types'
import { rebaseColorAnimation,normalizeColorAnimation,resolvedColorTangents } from './colorAnimation'
import { findClipLocation } from './commands'
import { evaluateAgentBatch } from '../agent/operations'
import { EASING_PRESETS } from './easingPresets'
import { cloneInterpolation } from './tangents'

function textTarget(document: Composition,id: string): TextClip | ColorClip {
  const { clip,track } = findClipLocation(document,id)
  if (clip.kind !== 'text' && clip.kind !== 'color') throw new Error('Color animation requires a text or color layer')
  if (clip.locked || track.locked) throw new Error('Layer or track is locked')
  return clip
}

export function setTextColorTangentMode(document: Composition,id: string,tick: number,mode: 'auto' | 'free'): Composition {
  const clip = textTarget(document,id)
  if (mode !== 'auto' && mode !== 'free') throw new Error('Invalid color tangent mode')
  const value = clip.colorAnimation && normalizeColorAnimation(clip.colorAnimation)
  if (value?.mode !== 'keyframes') throw new Error('Color keys required')
  const index = value.track.keyframes.findIndex(key => key.tick === tick)
  if (index < 0) throw new Error('Color key does not exist')
  const keys = [...value.track.keyframes]
  const tangents = resolvedColorTangents(value,index)
  keys[index] = { ...keys[index]!,tangents: { ...tangents,incoming: { ...tangents.incoming,mode },outgoing: { ...tangents.outgoing,mode },...(mode === 'auto' ? { continuity: 'smooth' as const } : {}) } }
  if (mode === 'auto') for (const span of [index-1,index]) {
    if (span < 0 || span+1 >= keys.length) continue
    const segment = keys[span]!.interpolation ?? value.track.interpolation
    if (typeof segment !== 'object' || segment.kind !== 'bezier') keys[span] = { ...keys[span]!,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] } }
  }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: { ...value,track: { ...value.track,keyframes: keys } } } }])
}

export function editTextColorTangents(document: Composition,id: string,tick: number,patch: { continuity?: 'smooth' | 'broken';side?: 'incoming' | 'outgoing';point?: { x: number;y: number } }): Composition {
  const clip = textTarget(document,id)
  const value = clip.colorAnimation && normalizeColorAnimation(clip.colorAnimation)
  if (value?.mode !== 'keyframes') throw new Error('Color keys required')
  const index = value.track.keyframes.findIndex(key => key.tick === tick)
  if (index < 0) throw new Error('Color key does not exist')
  const keys = [...value.track.keyframes]
  let tangents = resolvedColorTangents(value,index)
  if (patch.continuity !== undefined) tangents = { ...tangents,continuity: patch.continuity }
  if (patch.point !== undefined) {
    if (patch.side !== 'incoming' && patch.side !== 'outgoing') throw new Error('Invalid color tangent side')
    tangents = { ...tangents,incoming: { ...tangents.incoming,mode: 'free' },outgoing: { ...tangents.outgoing,mode: 'free' },[patch.side]: { ...patch.point,mode: 'free' } }
    const span = patch.side === 'incoming' ? index-1 : index
    if (span >= 0 && span+1 < keys.length) {
      const mode = keys[span]!.interpolation ?? value.track.interpolation
      if (typeof mode !== 'object' || mode.kind !== 'bezier') {
        const other = span === index ? index+1 : span
        keys[other] = { ...keys[other]!,tangents: resolvedColorTangents(value,other) }
        keys[span] = { ...keys[span]!,interpolation: { kind: 'bezier',p1: [1/3,1/3],p2: [2/3,2/3] } }
      }
    }
  }
  keys[index] = { ...keys[index]!,tangents }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: { ...value,track: { ...value.track,keyframes: keys } } } }])
}

export function setTextColorKey(document: Composition,id: string,playheadTicks: number,color: string): Composition {
  const clip = textTarget(document,id)
  if (!Number.isSafeInteger(playheadTicks) || playheadTicks < clip.timelineStartTicks || playheadTicks > clip.timelineStartTicks+clipDurationTicks(clip)) throw new Error('Color key time is outside the layer')
  const current = rebaseColorAnimation(clip.colorAnimation ?? { mode: 'constant',value: clip.kind === 'text' ? clip.style.color : clip.color })
  const track = current.mode === 'keyframes' ? current.track : { timeBase: COMPOSITION_TIME_BASE,interpolation: 'linear' as const,keyframes: [] }
  const tick = Math.round((playheadTicks-clip.timelineStartTicks)*track.timeBase/document.timeBase)
  const keys = [...track.keyframes]
  if (current.mode === 'constant' && tick !== 0) keys.push({ tick: 0,value: current.value })
  const existing = keys.findIndex(key => key.tick === tick)
  if (existing < 0) keys.push({ tick,value: color })
  else keys[existing] = { ...keys[existing]!,value: color }
  keys.sort((left,right) => left.tick-right.tick)
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: { mode: 'keyframes',track: { ...track,keyframes: keys } } } }])
}

export function deleteTextColorKey(document: Composition,id: string,tick: number): Composition {
  const clip = textTarget(document,id)
  const current = clip.colorAnimation
  if (current?.mode !== 'keyframes') throw new Error('Color key does not exist')
  const removed = current.track.keyframes.find(key => key.tick === tick)
  if (!removed) throw new Error('Color key does not exist')
  const copied = rebaseColorAnimation(current)
  if (copied.mode !== 'keyframes') throw new Error('Color track required')
  const keys = copied.track.keyframes.filter(key => key.tick !== tick)
  const replacement: CompositionAnimatableColor = keys.length ? { ...copied,track: { ...copied.track,keyframes: keys } } : { mode: 'constant',value: removed.value }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: replacement } }])
}

export function updateTextColorKey(document: Composition,id: string,tick: number,patch: { tick?: number;value?: string }): Composition {
  const clip = textTarget(document,id)
  const copied = clip.colorAnimation && rebaseColorAnimation(clip.colorAnimation)
  if (copied?.mode !== 'keyframes' || !copied.track.keyframes.some(key => key.tick === tick)) throw new Error('Color key does not exist')
  const keys = copied.track.keyframes.map(key => key.tick === tick ? { ...key,...patch } : key).sort((a,b) => a.tick-b.tick)
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: { ...copied,track: { ...copied.track,keyframes: keys } } } }])
}

export function configureTextColor(document: Composition,id: string,options: { interpolation?: import('./types').CompositionInterpolation;preset?: string;tick?: number;side?: 'before' | 'after';extrapolation?: import('./types').CompositionExtrapolationMode }): Composition {
  const clip = textTarget(document,id)
  const copied = clip.colorAnimation && rebaseColorAnimation(clip.colorAnimation)
  if (copied?.mode !== 'keyframes') throw new Error('Color keys required')
  let track = copied.track
  if (options.preset !== undefined || options.interpolation !== undefined) {
    if (options.preset !== undefined && options.interpolation !== undefined) throw new Error('Choose a preset or explicit curve')
    const preset = EASING_PRESETS.find(item => item.id === options.preset)
    if (options.preset !== undefined && !preset) throw new Error('Unknown color easing preset')
    const interpolation = options.interpolation ?? preset!.interpolation
    if (options.tick !== undefined && !track.keyframes.some(key => key.tick === options.tick)) throw new Error('Color key does not exist')
    const keys = [...track.keyframes]
    const handles = (index: number) => resolvedColorTangents({ mode: 'keyframes',track: { ...track,keyframes: keys } },index)
    for (let index = 0;index < keys.length;index++) {
      const key = keys[index]!
      const selected = options.tick === undefined ? key.interpolation === undefined : key.tick === options.tick
      if (!selected) continue
      const mode = cloneInterpolation(interpolation)
      if (typeof mode === 'object' && mode.kind === 'bezier' && keys[index+1]) {
        const left = handles(index),right = handles(index+1)
        keys[index] = { ...key,tangents: { ...left,outgoing: { x: mode.p1[0],y: mode.p1[1],mode: 'free' },continuity: 'broken' } }
        keys[index+1] = { ...keys[index+1]!,tangents: { ...right,incoming: { x: mode.p2[0],y: mode.p2[1],mode: 'free' },continuity: 'broken' } }
      }
      if (options.tick !== undefined) keys[index] = { ...keys[index]!,interpolation: mode }
    }
    track = { ...track,keyframes: keys,...(options.tick === undefined ? { interpolation: cloneInterpolation(interpolation) } : {}) }
  }
  if (options.side !== undefined) {
    if (!['before','after'].includes(options.side) || !['hold','continue','loop','ping_pong','offset'].includes(options.extrapolation!)) throw new Error('Invalid color extrapolation')
    track = { ...track,extrapolation: { before: 'hold',after: 'hold',...track.extrapolation,[options.side]: options.extrapolation } }
  }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId: id,patch: { colorAnimation: { mode: 'keyframes',track } } }])
}

/** Apply all authored clock edits together so swaps never hit transient collisions. */
export function moveTextColorKeys(document: Composition,edits: readonly { clipId: string;tick: number;newTick: number }[]): Composition {
  if (!Array.isArray(edits) || !edits.length || edits.length > 4096) throw new Error('Color key move requires 1..4096 subjects')
  const subjects = new Set<string>()
  const groups = new Map<string,Extract<CompositionAnimatableColor,{ mode: 'keyframes' }>>()
  for (const edit of edits) {
    if (!Number.isSafeInteger(edit.tick) || !Number.isSafeInteger(edit.newTick)) throw new Error('Color key times must be safe integers')
    const subject = JSON.stringify([edit.clipId,edit.tick])
    if (subjects.has(subject)) throw new Error('Duplicate color key move subject')
    subjects.add(subject)
    const clip = textTarget(document,edit.clipId)
    if (clip.colorAnimation?.mode !== 'keyframes' || !clip.colorAnimation.track.keyframes.some(key => key.tick === edit.tick)) throw new Error('Color key does not exist')
    if (!groups.has(edit.clipId)) groups.set(edit.clipId,rebaseColorAnimation(clip.colorAnimation) as Extract<CompositionAnimatableColor,{ mode: 'keyframes' }>)
  }
  const operations = [...groups].map(([clipId,value]) => {
    const destinations = new Map(edits.filter(edit => edit.clipId === clipId).map(edit => [edit.tick,edit.newTick]))
    const keyframes = value.track.keyframes.map(key => ({ ...key,tick: destinations.get(key.tick) ?? key.tick })).sort((a,b) => a.tick-b.tick)
    return { op: 'update_layer' as const,clipId,patch: { colorAnimation: { ...value,track: { ...value.track,keyframes } } } }
  })
  return evaluateAgentBatch(document,operations)
}
