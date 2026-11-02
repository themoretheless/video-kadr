import {expect,it} from 'vitest'
import {insertNativeConcatShape} from './nativeShapeInsert'
import type {NativeNestedProject} from '$lib/api'
const project=():NativeNestedProject=>({schemaVersion:1,root:'root',compositions:{root:{instances:[],durationUs:1000000,content:{schemaVersion:1,timeBase:1000000,sources:{},tracks:[],canvas:{width:160,height:120,fpsMilli:30000,backgroundMode:'color',backgroundBlur:24,background:{red:0,green:0,blue:0,alpha:1}}}}}})
const request={clipId:'shape',trackId:'',newTrackId:'shapes',kind:'arrow' as const,startUs:500000,durationUs:2000000}
it('creates all seven retained source-free kinds and extends only the target composition',()=>{
 for(const kind of ['square','circle','triangle','parallelogram','trapezoid','line','arrow'] as const){const input=project(),before=JSON.stringify(input),output=insertNativeConcatShape(input,'root',{...request,kind});expect(JSON.stringify(input)).toBe(before);expect(output.compositions.root.durationUs).toBe(2500000);expect(output.compositions.root.content.sources).toEqual({});expect(output.compositions.root.content.tracks[0]).toMatchObject({kind:'graphic',clips:[{concatShape:{kind},width:160,height:120,timelineStartTick:500000,timelineEndTick:2500000}]})}
})
it('rejects locked tracks, duplicate IDs and invalid clocks atomically',()=>{
 const input=insertNativeConcatShape(project(),'root',request),before=JSON.stringify(input)
 for(const patch of [{clipId:'shape'},{clipId:'other',startUs:-1},{clipId:'other',durationUs:0}])expect(()=>insertNativeConcatShape(input,'root',{...request,...patch})).toThrow()
 const track=input.compositions.root.content.tracks[0];const locked={...input,compositions:{root:{...input.compositions.root,content:{...input.compositions.root.content,tracks:[{...track,locked:true}]}}}};expect(()=>insertNativeConcatShape(locked,'root',{...request,clipId:'other',trackId:'shapes',startUs:3000000})).toThrow('заблокирована');expect(JSON.stringify(input)).toBe(before)
 const added=insertNativeConcatShape(input,'root',{...request,clipId:'other',trackId:'shapes',startUs:2500000});expect(added.compositions.root.content.tracks).toHaveLength(1)
})
it('rebinds layer references on a new top graphic lane and refuses changes to locked consumers',()=>{
 const input=insertNativeConcatShape(project(),'root',request),track=input.compositions.root.content.tracks[0];if(track.kind!=='graphic')throw new Error('graphic required')
 const consumer={...track,clips:track.clips.map(clip=>({...clip,effects:[{kind:'track_matte' as const,parameters:{matte:1}}]}))};input.compositions.root.content={...input.compositions.root.content,tracks:[consumer]}
 const output=insertNativeConcatShape(input,'root',{...request,clipId:'other',newTrackId:'top'}),shifted=output.compositions.root.content.tracks[1];expect(shifted.clips[0]).toMatchObject({effects:[{kind:'track_matte',parameters:{matte:2}}]});expect(output.compositions.root.content.tracks[0].id).toBe('top')
 input.compositions.root.content={...input.compositions.root.content,tracks:[{...consumer,locked:true}]};expect(()=>insertNativeConcatShape(input,'root',{...request,clipId:'other',newTrackId:'top'})).toThrow('заблокированную')
})
