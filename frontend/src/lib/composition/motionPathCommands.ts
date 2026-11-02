import { unpackPositionConversion } from './positionConversion'
import { evaluateMotionPath,type NativeMotionPathRequest } from '../api'
import { evaluateAgentBatch } from '../agent/operations'
import { findClipLocation } from './commands'
import { clipDurationTicks,type Composition,type CompositionPositionAnimation } from './types'
import { EASING_PRESETS } from './easingPresets'
import { updateInterpolation,upsertKeyframe } from './keyframes'

export type MotionPathEdit =
  | { operation: 'translate';delta: { x: number;y: number } }
  | { operation: 'remove' | 'toggle_segment';index: number }
  | { operation: 'solve' }
  | { operation: 'edit';index: number;part: 'point' | 'in_handle' | 'out_handle';value: { x: number;y: number } }
  | { operation: 'mode';index: number;mode: 'corner' | 'smooth' | 'auto' }
  | { operation: 'insert';segment: number;t: number;id: string }

/** Computes an immutable authored edit; callers commit only against the captured document. */
export async function editCompositionMotionPath(document: Composition,clipId: string,edit: MotionPathEdit,signal?: AbortSignal): Promise<Composition> {
  const { clip,track } = findClipLocation(document,clipId)
  if (clip.kind === 'audio' || clip.position?.mode !== 'path') throw new Error('Motion path required')
  if (track.locked || clip.locked) throw new Error('Layer is locked')
  const position = JSON.parse(JSON.stringify(clip.position)) as Extract<CompositionPositionAnimation,{ mode: 'path' }>
  const request = { ...edit,path: position.path } as NativeMotionPathRequest
  const response = await evaluateMotionPath(request,signal)
  if (signal?.aborted) throw new Error('Motion path edit cancelled')
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId,patch: { position: { ...position,path: response.path } } }])
}

/** Exact representation switches only; animated axes need the native conversion fitter. */
export function switchStaticPositionMode(document: Composition,clipId: string,mode: 'xy' | 'path',nodeId: string): Composition {
  const { clip } = findClipLocation(document,clipId)
  if (clip.kind === 'audio') throw new Error('Visual layer required')
  if (clip.position?.mode === mode) return document
  let position: CompositionPositionAnimation
  if (mode === 'xy') {
    if (clip.position?.mode !== 'path' || clip.position.path.nodes.length !== 1) throw new Error('Multi-node path requires motion conversion')
    const point = clip.position.path.nodes[0]!.point
    position = { mode: 'xy',x: { mode: 'constant',value: point.x },y: { mode: 'constant',value: point.y } }
  } else {
    const x = clip.position?.mode === 'xy' ? clip.position.x : clip.animation?.x ?? { mode: 'constant' as const,value: clip.kind === 'text' ? clip.x : clip.transform.x }
    const y = clip.position?.mode === 'xy' ? clip.position.y : clip.animation?.y ?? { mode: 'constant' as const,value: clip.kind === 'text' ? clip.y : clip.transform.y }
    if (x.mode !== 'constant' || y.mode !== 'constant') throw new Error('Animated XY requires motion conversion')
    position = { mode: 'path',path: { nodes: [{ id: nodeId,point: { x: x.value,y: y.value },inHandle: { x: 0,y: 0 },outHandle: { x: 0,y: 0 },segment: 'line',tangentMode: 'corner' }] },progress: { mode: 'constant',value: 0 } }
  }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId,patch: { position } }])
}


export function setMotionPathProgressKey(document: Composition,clipId: string,playheadTicks: number,value: number): Composition {
  const { clip } = findClipLocation(document,clipId)
  if (clip.kind === 'audio' || clip.position?.mode !== 'path') throw new Error('Motion path required')
  const local = playheadTicks-clip.timelineStartTicks
  if (!Number.isSafeInteger(playheadTicks) || local < 0 || local > clipDurationTicks(clip)) throw new Error('Playhead outside motion path clip')
  if (!Number.isFinite(value)) throw new Error('Finite progress required')
  let progress = clip.position.progress
  const timeBase = progress.mode === 'keyframes' ? progress.track.timeBase : document.timeBase
  const tick = Math.round(local*timeBase/document.timeBase)
  if (progress.mode === 'constant') {
    const initial = progress.value
    progress = { mode: 'keyframes',track: { timeBase,interpolation: 'linear',keyframes: [{ tick: 0,value: initial }] } }
  }
  progress = upsertKeyframe(progress,tick,value,0)
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId,patch: { position: { ...clip.position,progress } } }])
}

export async function arriveAtMotionPathNode(document: Composition,clipId: string,index: number,playheadTicks: number,signal?: AbortSignal): Promise<Composition> {
  const { clip,track } = findClipLocation(document,clipId)
  if (clip.kind === 'audio' || clip.position?.mode !== 'path') throw new Error('Motion path required')
  if (track.locked || clip.locked) throw new Error('Layer is locked')
  if (!Number.isSafeInteger(index) || !clip.position.path.nodes[index]) throw new Error('Path node not found')
  const path = JSON.parse(JSON.stringify(clip.position.path))
  const measurement = await evaluateMotionPath({ operation: 'measure',path },signal)
  if (signal?.aborted) throw new Error('Motion path edit cancelled')
  const progress = measurement.nodeProgresses?.[index]
  if (progress === undefined || !Number.isFinite(progress)) throw new Error('Native node distance missing')
  return setMotionPathProgressKey(document,clipId,playheadTicks,progress)
}


export function configureMotionPathProgress(document: Composition,clipId: string,options: { interpolation?: import('./types').CompositionInterpolation;preset?: string;side?: 'before' | 'after';extrapolation?: import('./types').CompositionExtrapolationMode }): Composition {
  const { clip } = findClipLocation(document,clipId)
  if (clip.kind === 'audio' || clip.position?.mode !== 'path' || clip.position.progress.mode !== 'keyframes') throw new Error('Path progress keys required')
  let progress = clip.position.progress
  if (options.interpolation !== undefined) progress = updateInterpolation(progress,options.interpolation) as typeof progress
  if (options.preset !== undefined) {
    const preset = EASING_PRESETS.find(entry => entry.id === options.preset)
    if (!preset) throw new Error('Unknown easing preset')
    progress = updateInterpolation(progress,preset.interpolation) as typeof progress
  }
  if (options.side !== undefined) {
    if ((options.side !== 'before' && options.side !== 'after') || !['hold','continue','loop','ping_pong','offset'].includes(options.extrapolation!)) throw new Error('Invalid extrapolation')
    progress = { ...progress,track: { ...progress.track,extrapolation: { before: 'hold',after: 'hold',...progress.track.extrapolation,[options.side]: options.extrapolation } } }
  }
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId,patch: { position: { ...clip.position,progress } } }])
}

export async function insertMotionPathNodeNear(document: Composition,clipId: string,point: { x: number;y: number },id: string,maxDistance: number,signal?: AbortSignal): Promise<Composition> {
  const { clip,track } = findClipLocation(document,clipId)
  if (clip.kind === 'audio' || clip.position?.mode !== 'path') throw new Error('Motion path required')
  if (track.locked || clip.locked) throw new Error('Layer is locked')
  if (!Number.isFinite(maxDistance) || maxDistance < 0) throw new Error('Invalid hit-test tolerance')
  const path = JSON.parse(JSON.stringify(clip.position.path))
  const response = await evaluateMotionPath({ operation: 'nearest',path,point },signal)
  if (signal?.aborted) throw new Error('Motion path edit cancelled')
  const hit = response.nearest
  if (!hit || hit.distance > maxDistance) throw new Error('No nearby path segment')
  return editCompositionMotionPath(document,clipId,{ operation: 'insert',segment: hit.segment,t: hit.t,id },signal)
}

export function applyPositionConversion(document: Composition,clipId: string,result: import('../api').NativePositionConversionResult,nodeId: (index: number) => string): Composition {
  const position = unpackPositionConversion(result,nodeId)
  const { clip } = findClipLocation(document,clipId)
  if (clip.kind === 'audio') throw new Error('Visual layer required')
  return evaluateAgentBatch(document,[{ op: 'update_layer',clipId,patch: { position } }])
}
