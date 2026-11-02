import {expect,it} from 'vitest'
import {concatSelectionKeyTarget} from './concatKeyNavigation'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {initializeConcatVisualAnimation} from './concatTitleAnimation'
import type {NativeNestedProject} from '$lib/api'
import type {WireGraphicClip} from './types'
function project(){const p:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}};const input=insertNativeConcatShape(p,'root',{clipId:'shape',trackId:'',newTrackId:'shapes',kind:'arrow',startUs:1000000,durationUs:2000000}),clip=input.compositions.root.content.tracks[0].clips[0] as WireGraphicClip,animation=initializeConcatVisualAnimation(clip,input.compositions.root.content.canvas);Object.assign(clip,{transform:{...clip.transform,concatAnimation:{...animation,keys:{rotation:[{at:0.25,value:0},{at:0.75,value:90},{at:1,value:180}]}}},effects:[{kind:'concat_adjust',parameters:{},concatKeys:{brightness:[{at:0.5,value:0.2}]}},{kind:'concat_exposure',parameters:{},concatKeys:[{at:0.6,value:1}]},{kind:'concat_adjust',parameters:{},concatKeys:{brightness:[{at:0.65,value:0.3}]}}]});return input}
it('steps across clip and first Adjust keys using source epsilon without modifying the project',()=>{
 const input=project(),before=JSON.stringify(input),target=(time:number,direction:-1|1)=>concatSelectionKeyTarget(input,'root',['shape'],time,direction)
 expect(target(2250000,-1)).toBe(2000000);expect(target(2250000,1)).toBe(2500000);expect(target(2497000,1)).toBe(3000000);expect(target(2503000,-1)).toBe(2000000);expect(target(900000,1)).toBeNull();expect(target(3000001,-1)).toBeNull();expect(target(3000000,1)).toBeNull();expect(JSON.stringify(input)).toBe(before)
})
it('uses the retained graphic clock and excludes keys outside its visible window',()=>{
 const input=project(),clip=input.compositions.root.content.tracks[0].clips[0];Object.assign(clip,{timelineEndTick:2500000,animationOffsetUs:500000,effects:[]})
 expect(concatSelectionKeyTarget(input,'root',['shape'],1100000,1)).toBe(2000000);expect(concatSelectionKeyTarget(input,'root',['shape'],2100000,1)).toBe(2500000);expect(concatSelectionKeyTarget(input,'root',['shape'],2100000,-1)).toBe(2000000)
})
it('collects disabled Adjust and source audio gain keys and stops at an unkeyed first Adjust',async()=>{
 const input=project(),clip=input.compositions.root.content.tracks[0].clips[0] as WireGraphicClip;Object.assign(clip,{effects:[{kind:'concat_bypass',link:{id:'concat.adjust',enabled:false,keys:{brightness:[{at:0.5,value:0.2}]}}}]});expect(concatSelectionKeyTarget(input,'root',['shape'],1600000,1)).toBe(2000000)
 Object.assign(clip,{effects:[{kind:'concat_adjust',parameters:{}},{kind:'concat_adjust',parameters:{},concatKeys:{brightness:[{at:0.5,value:0.2}]}}]});expect(concatSelectionKeyTarget(input,'root',['shape'],1600000,1)).toBe(2500000)
 const fixture=await import('../../../../backend/tests/fixtures/concat-merge-preview.json'),audio=structuredClone(fixture.default.nativeFirstFree[0].after) as unknown as NativeNestedProject;Object.assign(audio.compositions['new-timeline'].content.tracks[3].clips[0],{concatGainKeys:[{at:0.5,value:0.7}]});expect(concatSelectionKeyTarget(audio,'new-timeline',['auto-first-audio'],3000000,1)).toBe(3500000)
})
it('collects reusable marks and bounds dense display groups without removing navigation candidates',async()=>{
 const {concatSelectionKeyMarks,concatKeyTargetFromMarks,concatKeyMarkerGroups}=await import('./concatKeyNavigation'),input=project(),marks=concatSelectionKeyMarks(input,'root',['shape'])
 expect(marks.map(mark=>mark.timeUs)).toEqual([1500000,2000000,2500000,3000000]);expect(concatKeyTargetFromMarks(marks,2250000,1)).toBe(2500000)
 const dense=Array.from({length:4096},(_,i)=>({timeUs:i*1000,exactTimeUs:i*1000,epsilonUs:0,startUs:0,endUs:4096000})),groups=concatKeyMarkerGroups(dense,16)
 expect(groups).toHaveLength(16);expect(groups.reduce((count,group)=>count+group.count,0)).toBe(4096);expect(concatKeyTargetFromMarks(dense,123000,1)).toBe(124000);expect(concatKeyMarkerGroups([dense[0],dense[0]])).toEqual([{timeUs:0,count:2}]);expect(()=>concatKeyMarkerGroups(marks,0)).toThrow('предел')
})
