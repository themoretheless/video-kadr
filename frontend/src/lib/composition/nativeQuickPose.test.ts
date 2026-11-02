import {expect,it} from 'vitest'
import {quickPoseNativeClips} from './nativeQuickPose'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {initializeConcatVisualAnimation} from './concatTitleAnimation'
import type {NativeNestedProject} from '$lib/api'
import type {WireGraphicClip} from './types'
function project(){const p:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}};return insertNativeConcatShape(p,'root',{clipId:'shape',trackId:'',newTrackId:'shapes',kind:'arrow',startUs:1000000,durationUs:2000000})}
const clip=(p:NativeNestedProject)=>p.compositions.root.content.tracks[0].clips[0] as WireGraphicClip
it('rotates static poses with wrapping and toggles source preparation without changing input',()=>{
 const input=project(),before=JSON.stringify(input);let output=input
 for(const angle of [90,-180,-90,0]){output=quickPoseNativeClips(output,'root',['shape'],1500000,'rotate90');expect(clip(output).transform.rotationDegrees).toEqual({mode:'constant',value:angle})}
 const flipped=quickPoseNativeClips(input,'root',['shape'],1500000,'flipH');expect(clip(flipped).transform.concatPreparation).toMatchObject({flipH:true,crop:[0,0,0,0],canvas:[160,120]});expect(clip(quickPoseNativeClips(flipped,'root',['shape'],1500000,'flipH')).transform.concatPreparation?.flipH).toBe(false);expect(JSON.stringify(input)).toBe(before)
})
it('inserts a source rotation key at clamped playhead with inherited easing and keeps other channels',()=>{
 const input=project(),original=clip(input),animation=initializeConcatVisualAnimation(original,input.compositions.root.content.canvas)
 Object.assign(original,{transform:{...original.transform,concatAnimation:{...animation,keys:{rotation:[{at:0,value:0,ease:'inOut'},{at:1,value:120}],opacity:[{at:0,value:0.8}]}}}})
 const before=JSON.stringify(input),output=quickPoseNativeClips(input,'root',['shape'],2000000,'rotate90'),keys=clip(output).transform.concatAnimation!.keys
 expect(keys.rotation).toEqual([{at:0,value:0,ease:'inOut'},{at:0.5,value:150,ease:'inOut'},{at:1,value:120}]);expect(keys.opacity).toEqual(animation.keys.opacity??[{at:0,value:0.8}]);expect(JSON.stringify(input)).toBe(before)
 const end=quickPoseNativeClips(input,'root',['shape'],9000000,'rotate90');expect(clip(end).transform.concatAnimation!.keys.rotation?.at(-1)?.value).toBe(-150)
})
it('keeps native keyed rotation in its authored time base and refuses locked/missing targets atomically',()=>{
 const input=project(),original=clip(input);Object.assign(original,{transform:{...original.transform,rotationDegrees:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:0,value:0},{tick:2000,value:120}]}}}})
 const out=quickPoseNativeClips(input,'root',['shape'],2000000,'rotate90');expect(clip(out).transform.rotationDegrees).toMatchObject({track:{keyframes:[{tick:0,value:0},{tick:1000,value:150},{tick:2000,value:120}]}})
 expect(()=>quickPoseNativeClips(input,'root',['missing'],0,'rotate90')).toThrow('не найден');Object.assign(input.compositions.root.content.tracks[0],{locked:true});const before=JSON.stringify(input);expect(()=>quickPoseNativeClips(input,'root',['shape'],0,'rotate90')).toThrow('заблокирована');expect(JSON.stringify(input)).toBe(before)
})
it('reflects native keyed video scale without flattening it and leaves audio and treatments alone',async()=>{
 const fixture=await import('../../../../backend/tests/fixtures/concat-merge-preview.json')
 const input=structuredClone(fixture.default.nativeFirstFree[0].after) as unknown as NativeNestedProject,track=input.compositions['new-timeline'].content.tracks[1]
 if(track.kind!=='video')throw Error('video')
 Object.assign(track.clips[0],{transform:{...track.clips[0].transform,scaleX:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:0,value:1},{tick:2000,value:2}]}}}})
 const before=JSON.stringify(input),out=quickPoseNativeClips(input,'new-timeline',['auto-first','auto-first-audio'],2500000,'flipH')
 expect(out.compositions['new-timeline'].content.tracks[1].clips[0]).toMatchObject({transform:{scaleX:{track:{keyframes:[{tick:0,value:-1},{tick:2000,value:-2}]}}}});expect(out.compositions['new-timeline'].content.tracks[3]).toBe(input.compositions['new-timeline'].content.tracks[3]);expect(JSON.stringify(input)).toBe(before)
 const {insertNativeConcatTreatment}=await import('./nativeTreatmentInsert'),treatment=insertNativeConcatTreatment(project(),'root',{clipId:'look',trackId:'',newTrackId:'looks',startUs:0,durationUs:1000000,effects:[{kind:'concat_exposure',parameters:{stops:1}}]})
 expect(quickPoseNativeClips(treatment,'root',['look'],0,'rotate90')).toBe(treatment)
})
