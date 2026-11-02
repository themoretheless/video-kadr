import {expect,it} from 'vitest'
import {nearestConcatFreeStart} from './concatPlacement'
import {nativeClipEnd} from './nativeClipInsert'
import {MAX_COMPOSITION_DURATION_TICKS} from './types'
import {insertNativeConcatTitle} from './nativeTitleInsert'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {insertNativeConcatTreatment} from './nativeTreatmentInsert'
import type {NativeNestedProject} from '$lib/api'
it('chooses the nearest fitting interval on either side, retaining source candidate tie order',()=>{
 const clips=[{timelineStartTick:3000000,timelineEndTick:5000000},{timelineStartTick:8000000,timelineEndTick:10000000}]
 expect(nearestConcatFreeStart(clips,4000000,2000000)).toBe(5000000)
 expect(nearestConcatFreeStart(clips,2800000,2000000)).toBe(1000000)
 expect(nearestConcatFreeStart(clips,6000000,2000000)).toBe(6000000)
 expect(nearestConcatFreeStart([{timelineStartTick:2000000,timelineEndTick:4000000}],2000000,2000000)).toBe(0)
})
it('matches exhaustive integer placement distances for unsorted and overlapping occupied spans',()=>{
 for(let mask=0;mask<64;mask++){
  const clips=Array.from({length:6},(_,i)=>({timelineStartTick:i*2,timelineEndTick:i*2+3})).filter((_,i)=>(mask&(1<<i))!==0).reverse()
  for(let start=0;start<15;start++)for(let duration=1;duration<=4;duration++){
   const actual=nearestConcatFreeStart(clips,start,duration)
   const legal=Array.from({length:22},(_,at)=>at).filter(at=>!clips.some(clip=>at<clip.timelineEndTick&&clip.timelineStartTick<at+duration))
   expect(Math.abs(actual-start)).toBe(Math.min(...legal.map(at=>Math.abs(at-start))))
  }
 }
})
it('refuses overflow when the entire supported timeline is occupied',()=>{
 expect(()=>nearestConcatFreeStart([{timelineStartTick:0,timelineEndTick:MAX_COMPOSITION_DURATION_TICKS}],10,1)).toThrow('Нет свободного места')
 for(const [start,duration] of [[NaN,1],[-1,1],[0,0],[0,Infinity]])expect(()=>nearestConcatFreeStart([],start,duration)).toThrow()
})
const project=():NativeNestedProject=>({schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}})
it('uses nearest placement for title, shape and treatment creation on existing lanes without changing originals',()=>{
 const request={clipId:'original',trackId:'',newTrackId:'lane',startUs:500000,durationUs:2000000}
 const insertions=[(p:NativeNestedProject,r:typeof request)=>insertNativeConcatTitle(p,'root',{...r,text:'Title'}),(p:NativeNestedProject,r:typeof request)=>insertNativeConcatShape(p,'root',{...r,kind:'circle'}),(p:NativeNestedProject,r:typeof request)=>insertNativeConcatTreatment(p,'root',{...r,effects:[{kind:'concat_exposure',parameters:{stops:1}}]})]
 for(const insert of insertions){
  const input=insert(project(),request),before=JSON.stringify(input),out=insert(input,{...request,clipId:'next',trackId:'lane'})
  expect(out.compositions.root.content.tracks).toHaveLength(1);expect(out.compositions.root.content.tracks[0].clips[1]).toMatchObject({id:'next',timelineStartTick:2500000,timelineEndTick:4500000});expect(out.compositions.root.durationUs).toBeUndefined();expect(Math.max(...out.compositions.root.content.tracks.flatMap(track=>track.clips.map(nativeClipEnd)))).toBe(4500000);expect(JSON.stringify(input)).toBe(before)
 }
})
it('selects compatible lanes from the bottom and finds lanes above the highest occupied clip',async()=>{
 const {firstConcatFreeTrack}=await import('./concatPlacement')
 const base=insertNativeConcatShape(project(),'root',{clipId:'shape',trackId:'',newTrackId:'base',kind:'circle',startUs:1000000,durationUs:1000000}).compositions.root.content.tracks[0]
 const tracks=[{...base,id:'top',clips:[]},{...base,id:'occupied'},{...base,id:'low',clips:[]}]
 expect(firstConcatFreeTrack(tracks,'graphic',1000000,1000000,'first_free')).toBe('low')
 expect(firstConcatFreeTrack(tracks,'graphic',1000000,1000000,'first_free_above')).toBe('top')
 expect(firstConcatFreeTrack(tracks.map(track=>({...track,locked:track.id==='top'})),'graphic',1000000,1000000,'first_free_above')).toBeUndefined()
 expect(firstConcatFreeTrack(tracks,'text',1000000,1000000,'first_free')).toBeUndefined()
 expect(firstConcatFreeTrack(tracks,'graphic',2000000,1000000,'first_free')).toBe('low')
})
it('reuses free lanes for every owner and creates a top lane only when no eligible lane exists',()=>{
 const request={clipId:'original',trackId:'',newTrackId:'lane',startUs:500000,durationUs:2000000}
 const insertions=[(p:NativeNestedProject,r:typeof request&{trackPlacement?:'first_free'|'first_free_above'})=>insertNativeConcatTitle(p,'root',{...r,text:'Title'}),(p:NativeNestedProject,r:typeof request&{trackPlacement?:'first_free'|'first_free_above'})=>insertNativeConcatShape(p,'root',{...r,kind:'circle'}),(p:NativeNestedProject,r:typeof request&{trackPlacement?:'first_free'|'first_free_above'})=>insertNativeConcatTreatment(p,'root',{...r,effects:[{kind:'concat_exposure',parameters:{stops:1}}]})]
 for(const insert of insertions){
  const input=insert(project(),request),base=input.compositions.root.content.tracks[0],before=JSON.stringify(input)
  input.compositions.root.content={...input.compositions.root.content,tracks:[{...base,id:'top',clips:[]},base,{...base,id:'bottom',clips:[]}]}
  const snapshot=JSON.stringify(input)
  for(const [trackPlacement,lane] of [['first_free','bottom'],['first_free_above','top']] as const){
   const out=insert(input,{...request,clipId:'next',newTrackId:'unused',trackPlacement}),tracks=out.compositions.root.content.tracks
   expect(tracks).toHaveLength(3);expect(tracks.find(track=>track.id===lane)!.clips[0]).toMatchObject({id:'next',timelineStartTick:500000,timelineEndTick:2500000});expect(JSON.stringify(input)).toBe(snapshot)
  }
  const full=insert(JSON.parse(before),{...request,clipId:'next',newTrackId:'fresh',trackPlacement:'first_free'})
  expect(full.compositions.root.content.tracks).toHaveLength(2);expect(full.compositions.root.content.tracks[0]).toMatchObject({id:'fresh',clips:[{id:'next',timelineStartTick:500000}]})
 }
})
it('automatic titles skip caption-owned and locked lanes even when they are empty',()=>{
 const input=insertNativeConcatTitle(project(),'root',{clipId:'old',trackId:'',newTrackId:'caption',text:'Old',startUs:0,durationUs:1}),base=input.compositions.root.content.tracks[0]
 input.compositions.root.content={...input.compositions.root.content,tracks:[{...base,id:'available',clips:[]},{...base,id:'locked',locked:true,clips:[]},{...base,clips:[]}],metadata:{captionTextTracks:['caption']}}
 const out=insertNativeConcatTitle(input,'root',{clipId:'next',trackId:'',newTrackId:'unused',text:'Next',startUs:100,durationUs:1,trackPlacement:'first_free'})
 expect(out.compositions.root.content.tracks).toHaveLength(3);expect(out.compositions.root.content.tracks[0].clips[0].id).toBe('next')
})
it('preserves implicit timeline duration when a shorter retained visual is inserted',()=>{
 const request={clipId:'long',trackId:'',newTrackId:'original',startUs:0,durationUs:4000000}
 const insertions=[(p:NativeNestedProject,r:typeof request)=>insertNativeConcatTitle(p,'root',{...r,text:'Title'}),(p:NativeNestedProject,r:typeof request)=>insertNativeConcatShape(p,'root',{...r,kind:'circle'}),(p:NativeNestedProject,r:typeof request)=>insertNativeConcatTreatment(p,'root',{...r,effects:[{kind:'concat_exposure',parameters:{stops:1}}]})]
 for(const insert of insertions){const input=insert(project(),request),before=JSON.stringify(input),out=insert(input,{...request,clipId:'short',newTrackId:'new',durationUs:1000000});expect(out.compositions.root.durationUs).toBeUndefined();expect(Math.max(...out.compositions.root.content.tracks.flatMap(track=>track.clips.map(nativeClipEnd)))).toBe(4000000);expect(JSON.stringify(input)).toBe(before)}
})
