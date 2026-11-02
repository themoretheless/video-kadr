import type {NativeNestedProject} from '$lib/api'
import type {WireCompositionTrack,WireConcatParamKey,WireVideoEffect} from './types'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
function nearest(run:readonly WireConcatParamKey[],at:number):number{
 let index=-1,distance=Infinity
 run.forEach((key,i)=>{const delta=Math.abs(key.at-at);if(delta<=0.002&&delta<distance){index=i;distance=delta}})
 return index
}
/** Clear source key then set at destination, preserving value/easing and replacing a near collision. */
function moveRun(run:readonly WireConcatParamKey[],from:number,to:number):readonly WireConcatParamKey[]{
 const index=nearest(run,from)
 if(index<0||run[index]!.at===to)return run
 const key=run[index]!,remaining=run.filter((_,i)=>i!==index),collision=nearest(remaining,to)
 const moved={...key,at:to,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})}
 if(collision<0)remaining.push(moved);else remaining[collision]=moved
 return remaining.sort((a,b)=>a.at-b.at)
}
export function moveNativeConcatKeys(project:NativeNestedProject,scope:string,ids:readonly string[],fromTime:number,toTime:number):NativeNestedProject{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw Error('Таймлайн не найден')
 if(!Number.isSafeInteger(fromTime)||!Number.isSafeInteger(toTime)||fromTime<0||toTime<0)throw Error('Некорректное время ключей')
 if(fromTime===toTime)return project
 const selected=new Set(ids),known=new Set(node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)))
 if(ids.some(id=>!known.has(id)))throw Error('Выбранный клип не найден')
 let changed=false
 const tracks=node.content.tracks.map(track=>{
  const clips=track.clips.map(clip=>{
   if(!selected.has(clip.id))return clip
   const start=nativeClipStart(clip),end=nativeClipEnd(clip),duration=end-start
   if(duration<=0||fromTime<start||fromTime>end)return clip
   const offset='animationOffsetUs' in clip?clip.animationOffsetUs??0:0
   const move=(run:readonly WireConcatParamKey[],clock:number,age=0)=>moveRun(run,(fromTime-start+age)/clock,Math.max(0,Math.min(1,(Math.max(start,Math.min(end,toTime))-start+age)/clock)))
   let next=clip
   const update=(patch:object)=>{if(track.locked)throw Error('Дорожка заблокирована');changed=true;next={...next,...patch}}
   if('transform' in clip&&clip.transform.concatAnimation){
    const animation=clip.transform.concatAnimation;let altered=false
    const keys=Object.fromEntries(Object.entries(animation.keys).map(([name,run])=>{const result=run?move(run,animation.durationTicks,offset):run;altered ||= result!==run;return [name,result]}))
    if(altered)update({transform:{...clip.transform,concatAnimation:{...animation,keys}}})
   }
   if('concatGainKeys' in clip&&clip.concatGainKeys){const result=move(clip.concatGainKeys,duration);if(result!==clip.concatGainKeys)update({concatGainKeys:result})}
   if('effects' in clip&&clip.effects){
    let found=false,altered=false
    const treatment='concatTreatment' in clip?clip.concatTreatment:undefined
    const moveKeys=(keys:Readonly<Record<string,readonly WireConcatParamKey[]>>)=>Object.fromEntries(Object.entries(keys).map(([name,run])=>{const result=move(run,treatment?.durationTicks??duration,offset);altered ||= result!==run;return [name,result]}))
    const edit=(effect:WireVideoEffect):WireVideoEffect=>{
     if(found)return effect
     if(effect.kind==='native_bypass'){const inner=edit(effect.effect);return inner===effect.effect?effect:{...effect,effect:inner}}
     if(effect.kind==='concat_adjust'){found=true;const keys=moveKeys(effect.concatKeys??{});return altered?{...effect,concatKeys:keys}:effect}
     if(effect.kind==='concat_bypass'&&effect.link.id==='concat.adjust'){found=true;const keys=moveKeys(effect.link.keys??{});return altered?{...effect,link:{...effect.link,keys}}:effect}
     return effect
    }
    const effects=clip.effects.map(edit);if(altered)update({effects})
   }
   return next
  })
  return clips.every((clip,index)=>clip===track.clips[index])?track:{...track,clips} as WireCompositionTrack
 })
 return changed?{...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks}}}}:project
}
