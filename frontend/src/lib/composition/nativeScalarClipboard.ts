import {nativeEffectScalarBindings,shiftEffectScalarValue} from './nativeEffectScalar'
import type { NativeNestedProject } from '$lib/api'
import { readNativeKeyframeValue,replaceNativeKeyframeValue } from '$lib/agent/nativeKeyframes'
import { nativeScalarChannels,nativeScalarSelectedGroups,type NativeScalarKeySelection } from './nativeScalarTimeline'
import { snapshotSelectedKeyframes,planKeyframePaste,type KeyframeClipboardGroup } from './keyframeClipboard'
import { cloneKeyframe,resolvedKeyframeTangents,isKeyframeTangents } from './tangents'
import { isBezierInterpolation } from './bezier'
import { isElasticInterpolation } from './elastic'
import { nativeClipStart } from './nativeClipMove'
import { nativeClipEnd } from './nativeClipInsert'
import { keyframeAuthoredTick } from './keyframeClock'
import { MAX_KEYFRAMES_PER_VALUE,COMPOSITION_INTERPOLATIONS,type CompositionInterpolation } from './types'
export interface NativeScalarClipboard {format:'video-kadr/scalar-keys';version:1;groups:readonly KeyframeClipboardGroup[]}
export const MAX_SCALAR_CLIPBOARD_BYTES=4*1024*1024
const parameter=(envelope:string,property:string,effectIndex?:number,effectKind?:string)=>JSON.stringify(envelope==='effect'?[envelope,property,effectIndex,effectKind]:[envelope,property])
function validParameter(value:string){if(parameters.has(value))return true;try{const address:unknown=JSON.parse(value);return Array.isArray(address)&&address.length===4&&address[0]==='effect'&&typeof address[1]==='string'&&/^[a-zA-Z][a-zA-Z0-9_.]{0,127}$/.test(address[1])&&!['constructor','prototype','__proto__'].includes(address[1])&&Number.isSafeInteger(address[2])&&address[2]>=0&&address[2]<1024&&typeof address[3]==='string'&&/^[a-z][a-z0-9_]{0,127}$/.test(address[3])}catch{return false}}
const properties={visual:['x','y','scaleX','scaleY','rotationDegrees','opacity'],audio:['gain','pan'],path:['progress']} as const
const parameters=new Set(Object.entries(properties).flatMap(([envelope,keys])=>keys.map(key=>parameter(envelope,key))))
function object(value:unknown):value is Record<string,unknown>{return !!value&&typeof value==='object'&&!Array.isArray(value)}
function fields(value:Record<string,unknown>,allowed:string[]){return Object.keys(value).every(key=>allowed.includes(key))}
function interpolation(value:unknown):value is CompositionInterpolation{return typeof value==='string'?[...COMPOSITION_INTERPOLATIONS,'ease_in_out_cubic'].includes(value):isBezierInterpolation(value)||isElasticInterpolation(value)}
function check(value:unknown):asserts value is NativeScalarClipboard{
 if(!object(value)||!fields(value,['format','version','groups'])||value.format!=='video-kadr/scalar-keys'||value.version!==1||!Array.isArray(value.groups)||!value.groups.length||value.groups.length>256)throw new Error('Некорректный буфер скалярных ключей')
 const used=new Set<string>();let total=0
 for(const group of value.groups){
  if(!object(group)||!fields(group,['parameter','keys'])||typeof group.parameter!=='string'||!validParameter(group.parameter)||used.has(group.parameter)||!Array.isArray(group.keys)||!group.keys.length)throw new Error('Некорректный канал в буфере ключей')
  used.add(group.parameter);let previous=-1
  for(const key of group.keys){
   if(!object(key)||!fields(key,['tick','value','interpolation','tangents'])||!Number.isSafeInteger(key.tick)||Number(key.tick)<0||Number(key.tick)<=previous||typeof key.value!=='number'||!Number.isFinite(key.value)||key.interpolation!==undefined&&!interpolation(key.interpolation)||key.tangents!==undefined&&!isKeyframeTangents(key.tangents))throw new Error('Некорректный ключ в буфере')
   previous=Number(key.tick);if(++total>8192)throw new Error('Слишком много ключей в буфере')
  }
 }
}
export function serializeNativeScalarClipboard(clipboard:NativeScalarClipboard):string{
 check(clipboard);const text=JSON.stringify(clipboard)
 if(text.length>MAX_SCALAR_CLIPBOARD_BYTES||new TextEncoder().encode(text).length>MAX_SCALAR_CLIPBOARD_BYTES)throw new Error('Буфер ключей слишком большой')
 return text
}
export function parseNativeScalarClipboard(text:string):NativeScalarClipboard{
 if(typeof text!=='string'||text.length>MAX_SCALAR_CLIPBOARD_BYTES||new TextEncoder().encode(text).length>MAX_SCALAR_CLIPBOARD_BYTES)throw new Error('Буфер ключей слишком большой')
 let value:unknown;try{value=JSON.parse(text)}catch{throw new Error('В буфере нет скалярных ключей Video Kadr')}
 check(value);return value
}
export function captureNativeScalarClipboard(project:NativeNestedProject,scope:string,selection:readonly NativeScalarKeySelection[]):NativeScalarClipboard{
 const subjects=nativeScalarSelectedGroups(project,scope,selection).map(({channel,ticks})=>{
  const keys=channel.value.track.keyframes.map((key,index)=>{
   const own=key.interpolation??channel.value.track.interpolation,previous=channel.value.track.keyframes[index-1],before=previous?.interpolation??channel.value.track.interpolation
   return isBezierInterpolation(own)||previous&&isBezierInterpolation(before)?{...cloneKeyframe(key),tangents:resolvedKeyframeTangents(channel.value.track.keyframes,index,channel.value.track.interpolation)}:cloneKeyframe(key)
  })
  return {parameter:parameter(channel.envelope,channel.property,channel.effectIndex,channel.effectKind),timelineStartTicks:channel.start,selectedTicks:ticks,value:{...channel.value,track:{...channel.value.track,keyframes:keys}}}
 })
 return parseNativeScalarClipboard(serializeNativeScalarClipboard({format:'video-kadr/scalar-keys',version:1,groups:snapshotSelectedKeyframes(subjects)}))
}
export function nativeScalarPasteClips(project:NativeNestedProject,scope:string){return (project.compositions[scope]?.content.tracks??[]).flatMap(track=>track.clips.map(clip=>({id:clip.id,label:`${track.name} · ${clip.id}`,locked:track.locked})))}
export function planNativeScalarPaste(project:NativeNestedProject,scope:string,clipboard:NativeScalarClipboard,clipId:string,timeUs:number){
 check(clipboard)
 const node=project.compositions[scope],track=node?.content.tracks.find(track=>track.clips.some(clip=>clip.id===clipId)),clip=track?.clips.find(clip=>clip.id===clipId)
 if(!node||!track||!clip)throw new Error('Клип для вставки не найден')
 if(track.locked)throw new Error('Дорожка заблокирована')
 if(!Number.isSafeInteger(timeUs)||timeUs<nativeClipStart(clip)||timeUs>nativeClipEnd(clip))throw new Error('Курсор должен находиться внутри выбранного клипа')
 const targets=clipboard.groups.flatMap(group=>{
  const [envelope,property,effectIndex,effectKind]=JSON.parse(group.parameter) as [string,string,number|undefined,string|undefined]
  if(envelope==='audio'&&track.kind!=='audio'&&track.kind!=='video'||envelope!=='audio'&&track.kind==='audio')return []
  if(envelope==='path'&&(!('transform' in clip)||clip.transform.position?.mode!=='path'))return []
  if(envelope==='visual'&&(property==='x'||property==='y')&&'transform' in clip&&clip.transform.position?.mode==='path')return []
  const effect=envelope==='effect'?nativeEffectScalarBindings(clip).find(item=>item.effectIndex===effectIndex&&item.effectKind===effectKind&&item.property===property):undefined
  if(envelope==='effect'&&!effect)throw new Error('В выбранном клипе нет соответствующего канала эффекта')
  const raw=readNativeKeyframeValue(project,scope,{clipId,envelope,property,effectIndex,effectKind}),shift=effect?.shift??0,value=shiftEffectScalarValue(raw,shift),landings=group.keys.map(key=>{
   const local=timeUs-nativeClipStart(clip)+key.tick
   if(timeUs+key.tick>nativeClipEnd(clip))throw new Error('Ключи выходят за конец выбранного клипа')
   return value.mode==='keyframes'?keyframeAuthoredTick(value.track,local):local
  })
  if(new Set(landings).size!==landings.length)throw new Error('Сетка канала объединяет скопированные ключи')
  return [{id:group.parameter,parameter:group.parameter,value,timelineStartTicks:nativeClipStart(clip),envelope,property,effectIndex,effectKind,shift,raw}]
 })
 const plan=planKeyframePaste(clipboard.groups,targets,timeUs)
 if(!plan.entries.length)throw new Error('В выбранном клипе нет совместимых каналов')
 let candidate=project;const selection:NativeScalarKeySelection[]=[]
 for(const entry of plan.entries){
  if(entry.value.mode!=='keyframes'||entry.value.track.keyframes.length>MAX_KEYFRAMES_PER_VALUE)throw new Error('Слишком много ключей в канале')
  const target=targets.find(target=>target.id===entry.id)!
  let restored=shiftEffectScalarValue(entry.value,-target.shift)
  if(restored.mode==='keyframes'&&target.raw.mode==='keyframes'&&!target.raw.track.sampleClock&&restored.track.sampleClock?.rateNum===1&&restored.track.sampleClock.rateDen===1&&restored.track.sampleClock.offsetNum===0){const {sampleClock: _clock,...track}=restored.track;void _clock;restored={...restored,track}}
  candidate=replaceNativeKeyframeValue(candidate,scope,{clipId,envelope:target.envelope,property:target.property,effectIndex:target.effectIndex,effectKind:target.effectKind},restored)
  const channel=nativeScalarChannels(candidate,scope).find(channel=>channel.clipId===clipId&&channel.envelope===target.envelope&&channel.property===target.property&&channel.effectIndex===target.effectIndex&&channel.effectKind===target.effectKind)
  if(!channel)throw new Error('Вставленный канал не найден')
  for(const tick of entry.pastedTicks)selection.push({channelId:channel.id,tick})
 }
 return {project:candidate,selection,skipped:plan.skipped}
}
