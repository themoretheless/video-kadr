import {expect,it} from 'vitest'
import {moveNativeConcatKeys} from './nativeMoveConcatKeys'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {initializeConcatVisualAnimation} from './concatTitleAnimation'
import type {NativeNestedProject} from '$lib/api'
import type {WireGraphicClip} from './types'
const shape=(p:NativeNestedProject)=>p.compositions.root.content.tracks[0].clips[0] as WireGraphicClip
function project(){const p:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}};const input=insertNativeConcatShape(p,'root',{clipId:'shape',trackId:'',newTrackId:'shapes',kind:'arrow',startUs:1000000,durationUs:2000000}),clip=shape(input),animation=initializeConcatVisualAnimation(clip,input.compositions.root.content.canvas);Object.assign(clip,{transform:{...clip.transform,concatAnimation:{...animation,keys:{rotation:[{at:0.25,value:0},{at:0.5,value:90,ease:[0.2,0.1,0.7,0.9]},{at:0.75,value:180}],opacity:[{at:0.501,value:0.8,ease:'in'}]}}},effects:[{kind:'native_bypass',effect:{kind:'concat_adjust',parameters:{brightness:0.2},concatKeys:{brightness:[{at:0.5,value:0.3,ease:'out'}]}}},{kind:'concat_adjust',parameters:{},concatKeys:{brightness:[{at:0.5,value:0.4}]}}]});return input}
it('moves the cluster, replaces destination collisions and preserves values/easing and later Adjust',()=>{
 const input=project(),before=JSON.stringify(input),out=moveNativeConcatKeys(input,'root',['shape'],2000000,2500000),clip=shape(out)
 expect(clip.transform.concatAnimation!.keys.rotation).toEqual([{at:0.25,value:0},{at:0.75,value:90,ease:[0.2,0.1,0.7,0.9]}]);expect(clip.transform.concatAnimation!.keys.opacity).toEqual([{at:0.75,value:0.8,ease:'in'}]);expect(clip.effects![0]).toMatchObject({effect:{concatKeys:{brightness:[{at:0.75,value:0.3,ease:'out'}]}}});expect(clip.effects![1]).toBe(shape(input).effects![1]);expect(JSON.stringify(input)).toBe(before)
})
it('clamps to visible boundaries and converts retained graphic offsets independently',()=>{
 const input=project();Object.assign(shape(input),{animationOffsetUs:500000,timelineEndTick:2500000,effects:[]})
 const out=moveNativeConcatKeys(input,'root',['shape'],1500000,9000000);expect(shape(out).transform.concatAnimation!.keys.rotation).toContainEqual({at:1,value:90,ease:[0.2,0.1,0.7,0.9]});expect(shape(out).transform.concatAnimation!.keys.opacity).toEqual([{at:1,value:0.8,ease:'in'}])
 expect(moveNativeConcatKeys(input,'root',['shape'],900000,1500000)).toBe(input)
})
it('returns unchanged state for misses and refuses locked owners atomically',()=>{
 const input=project();expect(moveNativeConcatKeys(input,'root',['shape'],1100000,1200000)).toBe(input);expect(moveNativeConcatKeys(input,'root',['shape'],2000000,2000000)).toBe(input);Object.assign(input.compositions.root.content.tracks[0],{locked:true});const before=JSON.stringify(input);expect(()=>moveNativeConcatKeys(input,'root',['shape'],2000000,2500000)).toThrow('заблокирована');expect(JSON.stringify(input)).toBe(before);expect(()=>moveNativeConcatKeys(input,'root',['shape'],NaN,2500000)).toThrow('время')
})
it('moves source audio gain while preserving authored native gain automation',async()=>{
 const fixture=await import('../../../../backend/tests/fixtures/concat-merge-preview.json'),input=structuredClone(fixture.default.nativeFirstFree[0].after) as unknown as NativeNestedProject,track=input.compositions['new-timeline'].content.tracks[3];if(track.kind!=='audio')throw Error('audio');Object.assign(track.clips[0],{concatGainKeys:[{at:0.5,value:0.7,ease:'out'}]});const out=moveNativeConcatKeys(input,'new-timeline',['auto-first-audio'],3500000,4250000);expect(out.compositions['new-timeline'].content.tracks[3].clips[0]).toMatchObject({concatGainKeys:[{at:0.75,value:0.7,ease:'out'}],gain:track.clips[0].gain})
})
