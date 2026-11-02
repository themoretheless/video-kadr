import type {WireCompositionTrack} from './types'
import {evaluateAgentBatch} from '../agent/operations'
import {expect,it} from 'vitest'
import {createComposition,addTrack,addClip,findClipLocation} from './commands'
import {buildCompositionWireDocument} from './payload'
import {slipNativeLayer} from './nativeSlip'
import {nativeClipEnd} from './nativeClipInsert'
import type {NativeNestedProject} from '../api'
function authoring(){
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'},{media:{id:'media',kind:'video',durationTicks:5000000,width:16,height:16,hasAudio:true}})
 doc=addTrack(doc,{id:'v',kind:'video',name:'Video',hidden:false,muted:false,locked:false,clips:[]})
 doc=addClip(doc,'v',{id:'picture',kind:'video',sourceId:'media',timelineStartTicks:2000000,sourceInTicks:1000000,sourceOutTicks:3000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:true,audioGain:1})
 doc=addTrack(doc,{id:'a',kind:'audio',name:'Audio',muted:false,locked:false,clips:[]})
 doc=addClip(doc,'a',{id:'sound',kind:'audio',sourceId:'media',timelineStartTicks:0,sourceInTicks:1000000,sourceOutTicks:3000000,gain:1})
 return doc
}
function project():NativeNestedProject{
 const content=buildCompositionWireDocument(authoring())
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},other:{content:structuredClone(content),instances:[]}}}
}
it('slips video/audio windows while retaining timeline, animation, effects and unrelated nodes',()=>{
 const input=project(),before=JSON.stringify(input)
 for(const id of ['picture','sound']){
  const old=input.compositions.root!.content.tracks.flatMap<WireCompositionTrack['clips'][number]>(track=>track.clips).find(clip=>clip.id===id)!
  const output=slipNativeLayer(input,'root',id,500000),clip=output.compositions.root!.content.tracks.flatMap<WireCompositionTrack['clips'][number]>(track=>track.clips).find(clip=>clip.id===id)!
  expect(clip).toEqual({...old,placement:{...('placement' in old?old.placement:{}),sourceInTick:1500000,sourceOutTick:3500000}})
  expect(nativeClipEnd(clip)).toBe(nativeClipEnd(old));expect(output.compositions.other).toBe(input.compositions.other)
 }
 expect(JSON.stringify(input)).toBe(before);expect(slipNativeLayer(input,'root','picture',0)).toBe(input)
})
it('clamps forward/reverse retained source heads and refuses unsupported clocks or locks',()=>{
 const input=project(),clip=input.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('placement' in clip))throw new Error('Media fixture required')
 Object.assign(clip.placement,{sourceTimelineOffsetTick:-250000,sourceHeadTimelineTick:250000,timelineDurationTick:2250000})
 const left=slipNativeLayer(input,'root','picture',-5000000).compositions.root!.content.tracks[0]!.clips[0]!
 expect(left).toMatchObject({placement:{sourceInTick:250000,sourceOutTick:2250000,timelineStartTick:2000000,sourceTimelineOffsetTick:-250000}})
 Object.assign(clip,{playbackMode:{mode:'reverse'}})
 expect(slipNativeLayer(input,'root','picture',5000000).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({placement:{sourceInTick:2750000,sourceOutTick:4750000}})
 Object.assign(clip,{playbackMode:{mode:'freeze',sourceTick:1000000}})
 expect(()=>slipNativeLayer(input,'root','picture',1)).toThrow('Freeze')
 expect(()=>slipNativeLayer(input,'root','sound',Infinity)).toThrow('safe')
 Object.assign(input.compositions.root!.content.tracks[1]!,{locked:true});expect(()=>slipNativeLayer(input,'root','sound',1)).toThrow('locked')
})

it('exposes the same source-only slip through ordinary agent batches',()=>{
 const input=authoring(),before=JSON.stringify(input)
 const output=evaluateAgentBatch(input,[{op:'slip_layer',clipId:'picture',sourceDeltaTicks:500000}])
 expect(findClipLocation(output,'picture').clip).toMatchObject({timelineStartTicks:2000000,sourceInTicks:1500000,sourceOutTicks:3500000})
 expect(JSON.stringify(input)).toBe(before)
})
