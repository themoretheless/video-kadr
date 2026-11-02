import {nativeEffectScalarBindings,shiftEffectScalarValue} from './nativeEffectScalar'
import type { NativeNestedProject } from '$lib/api'
import type { CompositionAnimatableValue } from './types'
import { readNativeKeyframeValue, applyNativeKeyframeOperation } from '$lib/agent/nativeKeyframes'
import { nativeClipStart } from './nativeClipMove'
import { nativeClipEnd } from './nativeClipInsert'
import { moveSelectedKeyframeTracks, scaleSelectedKeyframeTracks, type SelectedKeyframeTrack } from './keyframeSelection'
export interface NativeScalarKeySelection { channelId:string;tick:number }
export interface NativeScalarChannel {
  id:string;clipId:string;property:string;envelope:'visual'|'audio'|'path'|'effect';effectIndex?:number;effectKind?:string;label:string;start:number;duration:number;locked:boolean
  value:Extract<CompositionAnimatableValue,{mode:'keyframes'}>
}
export function nativeScalarChannels(project:NativeNestedProject,scope:string):NativeScalarChannel[]{
  const node=project.compositions[scope];if(!node)return []
  return node.content.tracks.flatMap(track=>track.clips.flatMap(clip=>{
    const properties:[NativeScalarChannel['envelope'],string][]=[]
    if(track.kind!=='audio'){
      if('transform' in clip&&clip.transform.position?.mode==='path')properties.push(['path','progress'])
      else properties.push(['visual','x'],['visual','y'])
      properties.push(['visual','scaleX'],['visual','scaleY'],['visual','rotationDegrees'],['visual','opacity'])
    }
    if(track.kind==='video'||track.kind==='audio')properties.push(['audio','gain'],['audio','pan'])
    const ordinary=properties.flatMap(([envelope,property])=>{
      const value=readNativeKeyframeValue(project,scope,{clipId:clip.id,envelope,property})
      if(value.mode!=='keyframes'||!value.track.keyframes.length)return []
      return [{id:JSON.stringify([clip.id,envelope,property]),clipId:clip.id,envelope,property,label:`${track.name} · ${clip.id} · ${property}`,start:nativeClipStart(clip),duration:nativeClipEnd(clip)-nativeClipStart(clip),locked:track.locked,value}]
    })
    const effects=nativeEffectScalarBindings(clip).flatMap(binding=>{const value=shiftEffectScalarValue(binding.value,binding.shift);return value.mode==='keyframes'&&value.track.keyframes.length?[{id:JSON.stringify([clip.id,'effect',binding.effectIndex,binding.effectKind,binding.property]),clipId:clip.id,envelope:'effect' as const,effectIndex:binding.effectIndex,effectKind:binding.effectKind,property:binding.property,label:`${track.name} · ${clip.id} · ${binding.effectKind} #${binding.effectIndex+1} · ${binding.property}`,start:nativeClipStart(clip),duration:nativeClipEnd(clip)-nativeClipStart(clip),locked:track.locked,value}]:[]})
    return [...ordinary,...effects]
  }))
}
export function nativeScalarSelectedGroups(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[]){
  if(!selection.length||new Set(selection.map(item=>JSON.stringify(item))).size!==selection.length)throw new Error('Выберите разные существующие ключи')
  const channels=nativeScalarChannels(project,scope),groups=new Map<string,{channel:NativeScalarChannel;ticks:number[]}>()
  for(const item of selection){
    const channel=channels.find(channel=>channel.id===item.channelId)
    if(!channel||!Number.isSafeInteger(item.tick)||!channel.value.track.keyframes.some(key=>key.tick===item.tick))throw new Error('Выбранный ключ больше не существует')
    if(channel.locked)throw new Error('Дорожка заблокирована')
    const group=groups.get(channel.id)??{channel,ticks:[]};group.ticks.push(item.tick);groups.set(channel.id,group)
  }
  return [...groups.values()].map(group=>({...group,ticks:group.ticks.sort((a,b)=>a-b)}))
}
/** Plan against original tracks, then move from the leading edge so selected keys may share old destinations. */
export function planNativeScalarKeyMove(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[],requestedDeltaUs:number){
  const groups=nativeScalarSelectedGroups(project,scope,selection),canvas=project.compositions[scope]!.content.canvas
  const rate=canvas.fpsRate??{numerator:canvas.fpsMilli,denominator:1000}
  const moved=moveSelectedKeyframeTracks(groups.map(({channel,ticks})=>({id:channel.id,value:channel.value,selectedTicks:ticks,durationTicks:channel.duration})),requestedDeltaUs,rate)
  return {...nativeScalarRetimingResult(project,scope,groups,moved.groups),deltaUs:moved.delta}
}
/** Positive scaling uses composition time, then returns to each channel's authored clock. */
export function planNativeScalarKeyScale(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[],anchorUs:number,factor:number){
  if(!Number.isSafeInteger(anchorUs)||!Number.isFinite(factor)||factor<=0)throw new Error('Некорректный масштаб времени')
  const groups=nativeScalarSelectedGroups(project,scope,selection),canvas=project.compositions[scope]!.content.canvas
  const scaled=scaleSelectedKeyframeTracks(groups.map(({channel,ticks})=>({id:channel.id,value:channel.value,selectedTicks:ticks,durationTicks:channel.duration,timelineStartTicks:channel.start})),anchorUs,factor,canvas.fpsRate??{numerator:canvas.fpsMilli,denominator:1000},{constrainToClips:true})
  return {...nativeScalarRetimingResult(project,scope,groups,scaled.groups),factor:scaled.factor}
}
function nativeScalarRetimingResult(project:NativeNestedProject,scope:string,groups:ReturnType<typeof nativeScalarSelectedGroups>,targets:readonly SelectedKeyframeTrack[]){
  const operations:Record<string,unknown>[]=[],next:NativeScalarKeySelection[]=[]
  for(const [index,{channel,ticks}] of groups.entries()){
    const target=targets[index]!
    if(target.value.mode!=='keyframes'||target.value.track.keyframes.length!==channel.value.track.keyframes.length)throw new Error('На новом месте уже есть другой ключ')
    const edits=ticks.map((tick,index)=>({tick,newTick:target.selectedTicks[index]!}))
    for(const edit of edits)next.push({channelId:channel.id,tick:edit.newTick})
    const pending=edits.filter(edit=>edit.tick!==edit.newTick)
    while(pending.length){
      const free=pending.findIndex(edit=>!pending.some(other=>other.tick===edit.newTick))
      if(free<0)throw new Error('Циклическое перемещение ключей')
      const edit=pending.splice(free,1)[0]!
      operations.push({op:'move_keyframe',...nativeScalarAddress(channel),...edit})
    }
  }
  let candidate=project
  for(const operation of operations)candidate=applyNativeKeyframeOperation(candidate,scope,operation)
  return {project:candidate,operations,selection:next}
}
export function nativeScalarSelectionOperations(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[],action:'delete'|'preset',preset?:string):Record<string,unknown>[]{
  return nativeScalarSelectedGroups(project,scope,selection).flatMap<Record<string,unknown>>(({channel,ticks})=>action==='delete'?ticks.map(tick=>({op:'delete_keyframe',...nativeScalarAddress(channel),tick})):[{op:'set_keyframe_preset',...nativeScalarAddress(channel),ticks,preset}])
}


export function planNativeScalarKeyValues(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[],mode:'set'|'offset'|'multiply',operand:number){
 if(!['set','offset','multiply'].includes(mode)||!Number.isFinite(operand))throw new Error('Некорректное изменение значения')
 const operations:Record<string,unknown>[]=[]
 for(const {channel,ticks} of nativeScalarSelectedGroups(project,scope,selection))for(const tick of ticks){
  const key=channel.value.track.keyframes.find(key=>key.tick===tick)!
  const value=mode==='set'?operand:mode==='offset'?key.value+operand:key.value*operand
  if(!Number.isFinite(value))throw new Error('Значение ключа выходит за числовой диапазон')
  if(value!==key.value)operations.push({op:'edit_keyframe',...nativeScalarAddress(channel),tick,value})
 }
 let candidate=project;for(const op of operations)candidate=applyNativeKeyframeOperation(candidate,scope,op)
 return {project:candidate,operations,selection:[...selection]}
}
/** A graph point combines both axes into a single guarded authoring transaction. */
export function planNativeScalarPointEdit(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[],deltaUs:number,valueDelta:number){
 if(deltaUs===0)return {...planNativeScalarKeyValues(project,scope,selection,'offset',valueDelta),deltaUs:0}
 const moved=planNativeScalarKeyMove(project,scope,selection,deltaUs)
 const values=planNativeScalarKeyValues(moved.project,scope,moved.selection,'offset',valueDelta)
 return {...values,operations:[...moved.operations,...values.operations],deltaUs:moved.deltaUs}
}

export function nativeScalarAddress(channel:Pick<NativeScalarChannel,'clipId'|'envelope'|'property'|'effectIndex'|'effectKind'>){return {clipId:channel.clipId,envelope:channel.envelope,property:channel.property,...(channel.envelope==='effect'?{effectIndex:channel.effectIndex,effectKind:channel.effectKind}:{})}}

export function nativeScalarTargetLabel(channel:NativeScalarChannel){return channel.envelope==='effect'?`${channel.clipId} ${channel.effectKind} #${channel.effectIndex!+1} ${channel.property}`:`${channel.clipId} ${channel.property}`}
