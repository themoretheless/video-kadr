import type {NativeNestedProject} from '$lib/api'
import type {WireCompositionTrack,WireAnimatableValue} from './types'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
import {sampleConcatSourceRun} from './concatSourceWindow'
import {sampleAnimatableValue,upsertKeyframe,keyframeTickAtLocalTime} from './keyframes'
export type NativeQuickPose='rotate90'|'flipH'|'flipV'
const wrap=(value:number)=>((value+180)%360+360)%360-180
function negate(value:WireAnimatableValue):WireAnimatableValue{
 return value.mode==='constant'?{...value,value:-value.value}:{...value,track:{...value.track,keyframes:value.track.keyframes.map(key=>({...key,value:-key.value}))}}
}
export function quickPoseNativeClips(project:NativeNestedProject,scope:string,ids:readonly string[],time:number,action:NativeQuickPose):NativeNestedProject{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)throw Error('Таймлайн не найден')
 if(!Number.isSafeInteger(time)||time<0||!['rotate90','flipH','flipV'].includes(action))throw Error('Некорректная команда геометрии')
 const selected=new Set(ids),known=new Set(node.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id)))
 if(ids.some(id=>!known.has(id)))throw Error('Выбранный клип не найден')
 let changed=false
 const tracks=node.content.tracks.map(track=>{
  if(!['video','image','text','graphic'].includes(track.kind))return track
  const clips=track.clips.map(clip=>{
   if(!selected.has(clip.id)||!('transform' in clip)||'concatTreatment' in clip&&clip.concatTreatment)return clip
   if(track.locked)throw Error('Дорожка заблокирована')
   const transform=clip.transform,animation=transform.concatAnimation
   changed=true
   if(action!=='rotate90'){
    const retained='concatTitle' in clip&&clip.concatTitle||'concatShape' in clip&&clip.concatShape
    const preparation=transform.concatPreparation??(retained||animation?{crop:[0,0,0,0] as const,canvas:animation?.canvas??[node.content.canvas.width,node.content.canvas.height] as const}:undefined)
    if(preparation)return {...clip,transform:{...transform,concatPreparation:{...preparation,[action]:!preparation[action]}}}
    return {...clip,transform:{...transform,[action==='flipH'?'scaleX':'scaleY']:negate(transform[action==='flipH'?'scaleX':'scaleY'])}}
   }
   const start=nativeClipStart(clip),duration=nativeClipEnd(clip)-start,local=Math.max(0,Math.min(duration,time-start))+('animationOffsetUs' in clip?clip.animationOffsetUs??0:0)
   if(animation){
    const at=Math.max(0,Math.min(1,local/animation.durationTicks)),run=animation.keys.rotation??[],value=wrap((run.length?sampleConcatSourceRun(run,at):animation.rest.rotationDegrees)+90)
    if(!run.length)return {...clip,transform:{...transform,concatAnimation:{...animation,rest:{...animation.rest,rotationDegrees:value}}}}
    const keys=run.map(key=>({...key}));let nearest=-1,distance=Infinity
    keys.forEach((key,index)=>{const d=Math.abs(key.at-at);if(d<=0.002&&d<distance){nearest=index;distance=d}})
    if(nearest>=0)keys[nearest]={...keys[nearest],at,value,derived:false}
    else keys.push({at,value,ease:[...run].reverse().find(key=>key.at<at)?.ease??'linear'})
    keys.sort((a,b)=>a.at-b.at)
    if(keys.length>4096||Object.entries(animation.keys).reduce((n,[property,run])=>n+(property==='rotation'?0:run?.length??0),keys.length)>8192)throw Error('Слишком много ключей клипа Concat')
    return {...clip,transform:{...transform,concatAnimation:{...animation,keys:{...animation.keys,rotation:keys}}}}
   }
   const rotation=transform.rotationDegrees,value=wrap(sampleAnimatableValue(rotation,local)+90)
   return {...clip,transform:{...transform,rotationDegrees:rotation.mode==='constant'?{...rotation,value}:upsertKeyframe(rotation,keyframeTickAtLocalTime(local,rotation.track.timeBase,1000000,rotation.track),value,value)}}
  })
  return {...track,clips} as WireCompositionTrack
 })
 return changed?{...project,compositions:{...project.compositions,[scope]:{...node,content:{...node.content,tracks}}}}:project
}
