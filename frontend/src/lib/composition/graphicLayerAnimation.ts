import { editKeyframeTangent } from './tangents'
import type { GraphicContent, CompositionAnimatableValue, CompositionKeyframe } from './types'
import { graphicShapeChannels, graphicTextChannels } from './graphicLayerProperties'
import { cloneGraphicValue } from './graphicLayers'

export function graphicChannelAnimation(content: GraphicContent, channel: string): CompositionAnimatableValue | undefined {
  return (content.animation as Readonly<Record<string, CompositionAnimatableValue>> | undefined)?.[channel]
}
function channelAllowed(content: GraphicContent, channel: string) {
  if (!(content.kind === 'shape' ? graphicShapeChannels : graphicTextChannels).some(([key]) => key === channel)) throw new Error('Unknown graphic animation channel')
}
/** Convert the retained layer clock into the authored time base with exact integer rounding. */
export function graphicKeyTick(localUs: number, timeBase: number): number {
  if (!Number.isSafeInteger(localUs) || !Number.isSafeInteger(timeBase) || timeBase <= 0) throw new Error('Invalid graphic animation clock')
  const scaled = BigInt(localUs) * BigInt(timeBase)
  const tick = Number(scaled < 0n ? -((-scaled + 500000n) / 1000000n) : (scaled + 500000n) / 1000000n)
  if (!Number.isSafeInteger(tick)) throw new Error('Graphic key time exceeds safe integer range')
  return tick
}
export function clearGraphicChannel(content: GraphicContent, channel: string): GraphicContent {
  channelAllowed(content, channel)
  const animation = { ...cloneGraphicValue(content.animation ?? {}) } as Record<string, CompositionAnimatableValue>
  delete animation[channel]
  const result = { ...content } as GraphicContent
  if (Object.keys(animation).length) return { ...result, animation } as GraphicContent
  const { animation: _removed, ...rest } = result
  return rest as GraphicContent
}
export function setGraphicChannelKey(content: GraphicContent, channel: string, localUs: number, value: number): GraphicContent {
  channelAllowed(content, channel)
  if (!Number.isFinite(value)) throw new Error('Graphic key value must be finite')
  const old = graphicChannelAnimation(content, channel)
  const timeBase = old?.mode === 'keyframes' ? old.track.timeBase : 1000000
  const tick = graphicKeyTick(localUs, timeBase)
  const keys = old?.mode === 'keyframes' ? cloneGraphicValue(old.track.keyframes) : []
  const key = { ...keys.find(key => key.tick === tick), tick, value }
  const keyframes = [...keys.filter(key => key.tick !== tick), key].sort((a,b) => a.tick-b.tick)
  const track = old?.mode === 'keyframes' ? { ...old.track,keyframes } : { timeBase,interpolation:'linear' as const,keyframes }
  return { ...content, animation: { ...content.animation, [channel]: { mode:'keyframes',track } } } as GraphicContent
}
export function editGraphicChannelKey(content: GraphicContent, channel: string, index: number, patch: Partial<CompositionKeyframe> | null): GraphicContent {
  channelAllowed(content,channel)
  const old = graphicChannelAnimation(content,channel)
  if (old?.mode !== 'keyframes' || !old.track.keyframes[index]) throw new Error('Graphic key does not exist')
  const keys = cloneGraphicValue([...old.track.keyframes])
  if (patch === null) keys.splice(index,1)
  else keys[index] = { ...keys[index]!, ...patch }
  if (!keys.length) return clearGraphicChannel(content,channel)
  if (keys.some(key => !Number.isSafeInteger(key.tick) || !Number.isFinite(key.value)) || new Set(keys.map(key => key.tick)).size !== keys.length) throw new Error('Graphic key times must be unique safe integers and values finite')
  keys.sort((a,b)=>a.tick-b.tick)
  return { ...content, animation: { ...content.animation,[channel]: { ...old, track: { ...old.track,keyframes:keys } } } } as GraphicContent
}
export function setGraphicTextKey(content: GraphicContent, localUs: number, value: string): GraphicContent {
  if (content.kind !== 'text') throw new Error('Source text keys require a text layer')
  const timeBase = content.sourceTextAnimation?.timeBase ?? 1000000
  const tick = graphicKeyTick(localUs,timeBase)
  const keys = cloneGraphicValue(content.sourceTextAnimation?.keyframes ?? [])
  return { ...content,sourceTextAnimation:{timeBase,keyframes:[...keys.filter(key=>key.tick!==tick),{tick,value}].sort((a,b)=>a.tick-b.tick)} }
}
export function editGraphicTextKey(content: GraphicContent,index: number,patch: Partial<{tick:number;value:string}> | null): GraphicContent {
  if (content.kind !== 'text' || !content.sourceTextAnimation?.keyframes[index]) throw new Error('Source text key does not exist')
  const keys = cloneGraphicValue([...content.sourceTextAnimation.keyframes])
  if (patch === null) keys.splice(index,1)
  else keys[index] = {...keys[index]!,...patch}
  if (!keys.length) { const { sourceTextAnimation:_removed,...rest } = content; return rest }
  if (keys.some(key=>!Number.isSafeInteger(key.tick)) || new Set(keys.map(key=>key.tick)).size!==keys.length) throw new Error('Source text key times must be unique safe integers')
  return {...content,sourceTextAnimation:{...content.sourceTextAnimation,keyframes:keys.sort((a,b)=>a.tick-b.tick)}}
}

export function replaceGraphicChannel(content: GraphicContent, channel: string, value: CompositionAnimatableValue): GraphicContent {
  channelAllowed(content,channel)
  return {...content,animation:{...content.animation,[channel]:cloneGraphicValue(value)}} as GraphicContent
}
export function setGraphicExtrapolation(content: GraphicContent,channel: string,side:'before'|'after',mode:import('./types').CompositionExtrapolationMode): GraphicContent {
  const old=graphicChannelAnimation(content,channel)
  if(old?.mode!=='keyframes')throw new Error('Graphic channel has no keyframes')
  if(!['hold','continue','loop','ping_pong','offset'].includes(mode))throw new Error('Unknown extrapolation mode')
  return replaceGraphicChannel(content,channel,{...old,track:{...old.track,extrapolation:{before:old.track.extrapolation?.before??'hold',after:old.track.extrapolation?.after??'hold',[side]:mode}}})
}

/** A numeric Bezier edit must update explicit handles that override the segment definition. */
export function setGraphicSegmentCurve(content: GraphicContent,channel: string,index: number,curve:import('./types').CompositionInterpolation): GraphicContent {
  const original=graphicChannelAnimation(content,channel)
  if(original?.mode!=='keyframes' || !original.track.keyframes[index])throw new Error('Graphic key does not exist')
  let result=editGraphicChannelKey(content,channel,index,{interpolation:curve})
  let value=graphicChannelAnimation(result,channel)!
  if(typeof curve==='object' && curve.kind==='bezier'){
    const left=original.track.keyframes[index]!,right=original.track.keyframes[index+1]
    if(left.tangents)value=editKeyframeTangent(value,left.tick,'outgoing',curve.p1[0],curve.p1[1])
    if(right?.tangents)value=editKeyframeTangent(value,right.tick,'incoming',curve.p2[0],curve.p2[1])
    result=replaceGraphicChannel(result,channel,value)
  }
  return result
}
