import {expect,it} from 'vitest'
import {clearNativeConcatKeys} from './nativeClearConcatKeys'
import {insertNativeConcatShape} from './nativeShapeInsert'
import {initializeConcatVisualAnimation} from './concatTitleAnimation'
import type {NativeNestedProject} from '$lib/api'
import type {WireGraphicClip,WireVideoEffect} from './types'
const run=[{at:0,value:0},{at:1,value:1}] as const
const clip=(p:NativeNestedProject)=>p.compositions.root.content.tracks[0].clips[0] as WireGraphicClip
function project(){
 const p:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{instances:[],content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}}
 const input=insertNativeConcatShape(p,'root',{clipId:'shape',trackId:'',newTrackId:'shapes',kind:'arrow',startUs:1000000,durationUs:2000000}),shape=clip(input),animation=initializeConcatVisualAnimation(shape,input.compositions.root.content.canvas)
 Object.assign(shape,{transform:{...shape.transform,concatAnimation:{...animation,keys:{rotation:run}}},effects:[{kind:'concat_adjust',parameters:{brightness:0.2},concatKeys:{brightness:run}},{kind:'concat_exposure',parameters:{stops:1},concatKeys:run},{kind:'concat_adjust',parameters:{brightness:0.3},concatKeys:{brightness:run}}]})
 return input
}
it('returns motion to stored constants and clears only the first Adjust source runs immutably',()=>{
 const input=project(),before=JSON.stringify(input),out=clearNativeConcatKeys(input,'root',['shape']),shape=clip(out)
 expect(shape.transform.concatAnimation!.keys).toEqual({});expect(shape.transform.concatAnimation!.rest).toEqual(clip(input).transform.concatAnimation!.rest)
 expect(shape.effects![0]).toEqual({kind:'concat_adjust',parameters:{brightness:0.2},concatKeys:undefined});expect(shape.effects![1]).toBe(clip(input).effects![1]);expect(shape.effects![2]).toBe(clip(input).effects![2]);expect(JSON.stringify(input)).toBe(before);expect(clearNativeConcatKeys(out,'root',['shape'])).toBe(out)
})
it('recognizes first disabled Adjust owners but does not skip an unkeyed first entry',()=>{
 for(const first of [{kind:'native_bypass',effect:{kind:'concat_adjust',parameters:{brightness:0.2},concatKeys:{brightness:run}}},{kind:'concat_bypass',link:{id:'concat.adjust',enabled:false,params:{brightness:0.2},keys:{brightness:run}}}] as const){
  const input=project();Object.assign(clip(input),{effects:[first,clip(input).effects![2]]});const out=clearNativeConcatKeys(input,'root',['shape']);expect(JSON.stringify(clip(out).effects![0])).not.toContain('"at"');expect(clip(out).effects![1]).toBe(clip(input).effects![1])
 }
 const input=project();Object.assign(clip(input),{effects:[{kind:'concat_adjust',parameters:{}},clip(input).effects![2]] as WireVideoEffect[]});expect(clip(clearNativeConcatKeys(input,'root',['shape'])).effects).toEqual(clip(input).effects)
})
it('preserves native animation and refuses locked/missing selected owners without mutation',()=>{
 const input=project(),shape=clip(input),native={mode:'keyframes' as const,track:{timeBase:1000,interpolation:'linear' as const,keyframes:[{tick:0,value:0},{tick:1000,value:1}]}}
 Object.assign(shape,{transform:{...shape.transform,rotationDegrees:native}});expect(clip(clearNativeConcatKeys(input,'root',['shape'])).transform.rotationDegrees).toBe(native)
 Object.assign(input.compositions.root.content.tracks[0],{locked:true});const before=JSON.stringify(input);expect(()=>clearNativeConcatKeys(input,'root',['shape'])).toThrow('заблокирована');expect(()=>clearNativeConcatKeys(input,'root',['missing'])).toThrow('не найден');expect(JSON.stringify(input)).toBe(before)
})
it('clears retained source gain runs on audio while keeping its authored constant gain',async()=>{
 const fixture=await import('../../../../backend/tests/fixtures/concat-merge-preview.json'),input=structuredClone(fixture.default.nativeFirstFree[0].after) as unknown as NativeNestedProject,track=input.compositions['new-timeline'].content.tracks[3]
 if(track.kind!=='audio')throw Error('audio');Object.assign(track.clips[0],{concatGainKeys:run});const before=JSON.stringify(input),output=clearNativeConcatKeys(input,'new-timeline',['auto-first-audio']),after=output.compositions['new-timeline'].content.tracks[3]
 expect(after.clips[0]).toMatchObject({concatGainKeys:undefined,gain:track.clips[0].gain});expect(JSON.stringify(input)).toBe(before)
})
