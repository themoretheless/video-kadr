import type {NativeNestedProject} from '$lib/api'
import type {WireVideoEffect,WireConcatParamKey} from './types'
import {nativeClipStart} from './nativeClipMove'
import {nativeClipEnd} from './nativeClipInsert'
function firstAdjustRuns(effects:readonly WireVideoEffect[]):readonly (readonly WireConcatParamKey[])[]{
 for(const effect of effects){
  let inner=effect
  while(inner.kind==='native_bypass')inner=inner.effect
  if(inner.kind==='concat_adjust')return Object.values(inner.concatKeys??{})
  if(inner.kind==='concat_bypass'&&inner.link.id==='concat.adjust')return Object.values(inner.link.keys??{})
 }
 return []
}
export interface ConcatKeyMark {timeUs:number;exactTimeUs:number;epsilonUs:number;startUs:number;endUs:number}
/** Collect once per document/selection; the moving playhead is not part of this clock. */
export function concatSelectionKeyMarks(project:NativeNestedProject,scope:string,ids:readonly string[]):readonly ConcatKeyMark[]{
 const node=Object.hasOwn(project.compositions,scope)?project.compositions[scope]:undefined
 if(!node)return []
 const selected=new Set(ids)
 const marks:ConcatKeyMark[]=[]
 for(const track of node.content.tracks)for(const clip of track.clips){
  if(!selected.has(clip.id))continue
  const start=nativeClipStart(clip),end=nativeClipEnd(clip),duration=end-start
  if(duration<=0)continue
  const offset='animationOffsetUs' in clip?clip.animationOffsetUs??0:0
  const visit=(runs:readonly (readonly WireConcatParamKey[])[],clockDuration:number,clockOffset=0)=>{
   for(const run of runs)for(const key of run){
    if(!Number.isFinite(key.at)||key.at<0||key.at>1)continue
    const exact=start+key.at*clockDuration-clockOffset,at=Math.round(exact)
    if(!Number.isSafeInteger(at)||exact<start||exact>end)continue
    marks.push({timeUs:at,exactTimeUs:exact,epsilonUs:clockDuration*0.002,startUs:start,endUs:end})
   }
  }
  if('transform' in clip&&clip.transform.concatAnimation)visit(Object.values(clip.transform.concatAnimation.keys).filter((run):run is readonly WireConcatParamKey[]=>Boolean(run)),clip.transform.concatAnimation.durationTicks,offset)
  if('concatGainKeys' in clip&&clip.concatGainKeys)visit([clip.concatGainKeys],duration)
  if('effects' in clip&&clip.effects){const treatment='concatTreatment' in clip?clip.concatTreatment:undefined;visit(firstAdjustRuns(clip.effects),treatment?.durationTicks??duration,offset)}
 }
 return marks.sort((a,b)=>a.exactTimeUs-b.exactTimeUs)
}

export function concatKeyTargetFromMarks(marks:readonly ConcatKeyMark[],time:number,direction:-1|1):number|null{
 if(!Number.isSafeInteger(time)||time<0)return null
 for(let index=direction===1?0:marks.length-1;index>=0&&index<marks.length;index+=direction){const mark=marks[index]!;if(time>=mark.startUs&&time<=mark.endUs&&(mark.timeUs-time)*direction>0&&(mark.exactTimeUs-time)*direction>mark.epsilonUs)return mark.timeUs}
 return null
}
/** Source key_point / step_any_key, adapted to retained native clock windows. */
export function concatSelectionKeyTarget(project:NativeNestedProject,scope:string,ids:readonly string[],time:number,direction:-1|1):number|null{
 return concatKeyTargetFromMarks(concatSelectionKeyMarks(project,scope,ids),time,direction)
}
/** Dense runs share a visual marker; all original candidates remain navigable. */
export function concatKeyMarkerGroups(marks:readonly ConcatKeyMark[],limit=256):readonly {timeUs:number;count:number}[]{
 if(!Number.isSafeInteger(limit)||limit<1)throw Error('Некорректный предел отметок ключей')
 if(!marks.length)return []
 const first=marks[0]!.timeUs,last=marks.at(-1)!.timeUs,groups=new Map<number,{timeUs:number;count:number}>()
 for(const mark of marks){const bucket=last===first?0:Math.min(limit-1,Math.floor((mark.timeUs-first)/(last-first)*limit)),group=groups.get(bucket);if(group)group.count++;else groups.set(bucket,{timeUs:mark.timeUs,count:1})}
 return [...groups.values()]
}
