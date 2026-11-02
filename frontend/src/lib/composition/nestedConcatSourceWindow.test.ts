import {writeFileSync} from 'node:fs'
import {expect,it} from 'vitest'
import type {NativeNestedProject} from '$lib/api'
import type {Composition,WireVideoEffect} from './types'
import {createComposition} from './commands'
import {buildCompositionRenderRequest} from './payload'
import {splitNativeClips} from './nativeClipSplit'
import {trimNativeClipStarts,trimNativeClipEnds} from './nativeClipTrim'
const effect:WireVideoEffect={kind:'concat_exposure',parameters:{},concatKeys:[{at:0,value:0},{at:1,value:4,ease:[0.42,3,0.58,3]}]}
function project():NativeNestedProject{
 const c:Composition={...createComposition({width:16,height:16,fps:4,backgroundColor:'#000000'}),sources:{source:{id:'source',kind:'video',durationTicks:1000000,width:16,height:16,hasAudio:false}},tracks:[{id:'v',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[{id:'clip',kind:'video',sourceId:'source',timelineStartTicks:1000000,sourceInTicks:0,sourceOutTicks:1000000,speed:1,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:false,audioGain:1,effects:[effect]}],transitions:[]}]}
 return {schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionRenderRequest(c).composition,instances:[]}}}
}
it('splits source keys independently from the retained media phase without mutating input',()=>{
 const input=project(),before=JSON.stringify(input);const result=splitNativeClips(input,'root',['clip'],1500000,()=> 'right')
 const clips=result.compositions.root!.content.tracks[0]!.clips
 expect(clips[0]).toMatchObject({effects:[{concatKeys:[{at:0,value:0},{at:1,value:9.5,derived:true}]}]})
 expect(clips[1]).toMatchObject({placement:{timelineStartTick:1500000,sourceTimelineOffsetTick:500000,timelineDurationTick:500000},effects:[{concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]}]})
 expect(JSON.stringify(input)).toBe(before)
 if(process.env.CONCAT_NESTED_SPLIT_OUTPUT)writeFileSync(process.env.CONCAT_NESTED_SPLIT_OUTPUT,JSON.stringify(result))
})
it('trims either end with boundary samples and stable phase, and refuses an invalid edit atomically',()=>{
 const input=project(),before=JSON.stringify(input)
 expect(trimNativeClipEnds(input,'root',['clip'],-500000).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({placement:{timelineDurationTick:500000},effects:[{concatKeys:[{at:0,value:0},{at:1,value:9.5,derived:true}]}]})
 expect(trimNativeClipStarts(input,'root',['clip'],500000).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({placement:{timelineStartTick:1500000,sourceTimelineOffsetTick:500000,timelineDurationTick:500000},effects:[{concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]}]})
 expect(()=>trimNativeClipEnds(input,'root',['clip'],-1000000)).toThrow();expect(JSON.stringify(input)).toBe(before)
})
it('rewindows image clips in the non-placement split and trim branches',()=>{
 const base=project(),node=base.compositions.root!,track=node.content.tracks[0]!;if(track.kind!=='video')throw new Error('track');const video=track.clips[0]!
 const input:NativeNestedProject={...base,compositions:{root:{...node,content:{...node.content,sources:{source:{id:'source',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}},tracks:[{id:'image',kind:'image',name:'Image',hidden:false,locked:false,clips:[{id:'clip',sourceId:'source',timelineStartTick:1000000,durationTicks:1000000,transform:video.transform,opacity:video.opacity,blendMode:'normal',enabled:true,effects:[effect]}]}]}}}}
 expect(splitNativeClips(input,'root',['clip'],1500000,()=> 'right').compositions.root!.content.tracks[0]!.clips[1]).toMatchObject({timelineStartTick:1500000,durationTicks:500000,effects:[{concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]}]})
 expect(trimNativeClipStarts(input,'root',['clip'],500000).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({timelineStartTick:1500000,durationTicks:500000,effects:[{concatKeys:[{at:0,value:9.5,derived:true},{at:1,value:4}]}]})
 expect(trimNativeClipEnds(input,'root',['clip'],-500000).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({durationTicks:500000,effects:[{concatKeys:[{at:0,value:0},{at:1,value:9.5,derived:true}]}]})
})
