import {expect,it} from 'vitest'
import type {NativeNestedProject} from '$lib/api'
import type {WireGraphicClip} from './types'
import {insertNativeConcatTreatment} from './nativeTreatmentInsert'
import {splitNativeClips} from './nativeClipSplit'
import {trimNativeClipStarts,trimNativeClipEnds} from './nativeClipTrim'
import {sampleConcatSourceRun} from './concatSourceWindow'
function project(){
 const empty:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],durationUs:5000000,content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}}
 const result=insertNativeConcatTreatment(empty,'root',{clipId:'look',trackId:'',newTrackId:'layer',startUs:1000000,durationUs:4000000,effects:[{kind:'concat_exposure',parameters:{stops:0},concatKeys:[{at:0,value:0},{at:1,value:4}],animation:{mode:'keyframes',track:{timeBase:1000000,interpolation:'linear',keyframes:[{tick:2000000,value:1}]}}}]})
 const clip=clips(result)[0];clips(result)[0]={...clip,concatTreatment:{...clip.concatTreatment!,fadeIn:0.5,fadeOut:0.75}};return result
}
const clips=(p:NativeNestedProject)=>p.compositions.root.content.tracks[0].clips as WireGraphicClip[]
function value(clip:WireGraphicClip,progress:number){const effect=clip.effects![0];if(effect.kind!=='concat_exposure')throw Error('effect');return sampleConcatSourceRun(effect.concatKeys!,progress)}
it('splits treatment fades and source windows without moving absolute native keys',()=>{
 const input=project(),before=JSON.stringify(input),out=splitNativeClips(input,'root',['look'],3000000,()=> 'tail'),[head,tail]=clips(out)
 expect(head.concatTreatment).toEqual({strength:1,fadeIn:0.5,fadeOut:0,durationTicks:2000000})
 expect(tail.concatTreatment).toEqual({strength:1,fadeIn:0,fadeOut:0.75,durationTicks:2000000})
 expect(tail).toMatchObject({timelineStartTick:3000000,timelineEndTick:5000000,animationOffsetUs:0})
 expect(value(head,0.5)).toBeCloseTo(1);expect(value(tail,0.5)).toBeCloseTo(3)
 for(const clip of [head,tail])expect(clip.effects![0]).toMatchObject({animation:{track:{keyframes:[{tick:2000000}]}}})
 expect(JSON.stringify(input)).toBe(before)
})
it('rewindows successive head and tail trims, keeps fades, and permits source-free extension',()=>{
 const input=project(),head=trimNativeClipStarts(input,'root',['look'],1000000),tail=trimNativeClipEnds(head,'root',['look'],-1000000),clip=clips(tail)[0]
 expect(clip).toMatchObject({timelineStartTick:2000000,timelineEndTick:4000000,animationOffsetUs:0,concatTreatment:{durationTicks:2000000,fadeIn:0.5,fadeOut:0.75}})
 expect(value(clip,0)).toBeCloseTo(1);expect(value(clip,1)).toBeCloseTo(3)
 expect(clip.effects![0]).toMatchObject({animation:{track:{keyframes:[{tick:2000000}]}}})
 const extended=clips(trimNativeClipEnds(tail,'root',['look'],1000000))[0]
 expect(extended.concatTreatment!.durationTicks).toBe(3000000);expect(value(extended,1)).toBeCloseTo(3)
})
it('normalizes the original retained offset before a new split',()=>{
 const input=project();clips(input)[0]={...clips(input)[0],timelineStartTick:2000000,animationOffsetUs:1000000}
 const [head,tail]=clips(splitNativeClips(input,'root',['look'],3000000,()=> 'tail'))
 expect(value(head,0)).toBeCloseTo(1);expect(value(head,1)).toBeCloseTo(2);expect(value(tail,0)).toBeCloseTo(2);expect(value(tail,1)).toBeCloseTo(4)
})
it('duplicates absolute effect keys with the clip while retaining source keys, fades and local offset',async()=>{
 const {duplicateNativeClips}=await import('./nativeClipDuplicate')
 const input=project(),before=JSON.stringify(input),output=duplicateNativeClips(input,'root',['look'],()=> 'copy',5000000),copy=clips(output)[1]
 expect(copy).toMatchObject({timelineStartTick:5000000,timelineEndTick:9000000,concatTreatment:clips(input)[0].concatTreatment,effects:[{animation:{track:{keyframes:[{tick:6000000}]}}}]})
 expect(copy.animationOffsetUs).toBe(clips(input)[0].animationOffsetUs);expect(value(copy,0.5)).toBeCloseTo(2);expect(JSON.stringify(input)).toBe(before)
})
it('places a treatment duplicate beyond a neighbouring obstacle and shifts its absolute keys by the final delta',async()=>{
 const {duplicateNativeClips}=await import('./nativeClipDuplicate')
 const input=project(),original=clips(input)[0];clips(input).push({...original,id:'obstacle',timelineStartTick:5000000,timelineEndTick:6000000,concatTreatment:{...original.concatTreatment!,durationTicks:1000000}})
 const before=JSON.stringify(input),output=duplicateNativeClips(input,'root',['look'],()=> 'copy'),copy=clips(output).find(clip=>clip.id==='copy')!
 expect(copy).toMatchObject({timelineStartTick:6000000,timelineEndTick:10000000,effects:[{animation:{track:{keyframes:[{tick:7000000}]}}}]});expect(value(copy,0.5)).toBeCloseTo(2);expect(JSON.stringify(input)).toBe(before)
})
