import {expect,it} from 'vitest'
import {evaluateNativeAgentBatch} from './nativeBatch'
import {readNativeKeyframes} from './nativeKeyframes'
import {createComposition,addTrack,addClip} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
import type {NativeNestedProject} from '../api'
function project():NativeNestedProject{
 let doc=createComposition({width:320,height:180,fps:10,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'titles',kind:'text',name:'Titles',locked:false,hidden:false,clips:[]})
 doc=addClip(doc,'titles',{id:'title',kind:'text',timelineStartTicks:1000000,durationTicks:3000000,text:'Keys',x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center'}})
 const content=buildCompositionWireDocument(doc),clip=content.tracks[0]!.clips[0]!
 if('transform' in clip)Object.assign(clip,{transform:{...clip.transform,x:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',extrapolation:{before:'hold',after:'ping_pong'},keyframes:[{tick:-1000,value:0},{tick:0,value:10},{tick:1000,value:20}]}}}})
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},child:{content:structuredClone(content),instances:[]}}}
}
const read=(p:NativeNestedProject,property='x',envelope='visual')=>readNativeKeyframes(p,'root',{clipId:'title',property,envelope})
it('reads exact signed native clocks and timeline offsets without changing the project',()=>{
 const input=project(),before=JSON.stringify(input),value=read(input)
 expect(value).toMatchObject({mode:'keyframes',timeBase:1000,extrapolation:{after:'ping_pong'},keyframes:[{tick:-1000,t_local_us:-1000000,t_us:0,value:0},{tick:0,t_local_us:0,t_us:1000000,value:10},{tick:1000,t_local_us:1000000,t_us:2000000,value:20}]})
 expect('keyframes' in value&&value.keyframes.at(-1)?.preset_id).toBeUndefined();expect(JSON.stringify(input)).toBe(before)
})
it('upserts/moves/edits/deletes on the retained timeBase and rejects occupied key times atomically',async()=>{
 const input=project(),before=JSON.stringify(input)
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'title',property:'x',tick:-500,value:5},{op:'move_keyframe',clipId:'title',property:'x',tick:1000,newTick:2000},{op:'edit_keyframe',clipId:'title',property:'x',tick:2000,newTick:1500,value:30},{op:'delete_keyframe',clipId:'title',property:'x',tick:-500}])
 expect(read(output)).toMatchObject({timeBase:1000,keyframes:[{tick:-1000,value:0},{tick:0,value:10},{tick:1500,value:30}]})
 expect(output.compositions.child).toBe(input.compositions.child);expect(JSON.stringify(input)).toBe(before)
 const signedEdit=await evaluateNativeAgentBatch(input,'root',[{op:'edit_keyframe',clipId:'title',property:'x',tick:0,newTick:-500,value:5}]);expect(read(signedEdit)).toMatchObject({timeBase:1000,keyframes:[{tick:-1000},{tick:-500,value:5},{tick:1000}]})
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'move_keyframe',clipId:'title',property:'x',tick:-1000,newTick:0}])).rejects.toThrow('occupies')
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'title',property:'x',tick:2000,value:99},{op:'delete_keyframe',clipId:'title',property:'x',tick:999}])).rejects.toThrow('no longer exists')
 expect(JSON.stringify(input)).toBe(before)
})
it('shares easing presets, Auto/Free tangent solving, explicit tangent edits and last-key constant collapse',async()=>{
 const input=project()
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'set_keyframe_preset',clipId:'title',property:'x',ticks:[-1000],preset:'ease_in_out'},{op:'set_keyframe_tangent_mode',clipId:'title',property:'x',ticks:[0],mode:'auto'},{op:'set_keyframe_tangent_mode',clipId:'title',property:'x',ticks:[0],mode:'free'},{op:'edit_keyframe_tangents',clipId:'title',property:'x',tick:0,incoming:{x:0.4,y:0.2},outgoing:{x:0.6,y:0.8},continuity:'broken'}])
 expect(read(output)).toMatchObject({timeBase:1000,keyframes:[{}, {tick:0,incoming:{x:0.4,y:expect.any(Number),mode:'free'},outgoing:{x:0.6,y:0.8,mode:'free'},continuity:'broken'},{}]})
 const inspected=read(output);if(inspected.mode==='keyframes')expect(inspected.keyframes[1]!.incoming.y).toBeCloseTo(0.2,12)
 const collapsed=await evaluateNativeAgentBatch(input,'root',[-1000,0,1000].map(tick=>({op:'delete_keyframe',clipId:'title',property:'x',tick})))
 expect(read(collapsed)).toEqual({mode:'constant',value:20})
})
it('edits active XY channels instead of shadow legacy fields and refuses XY edits on an active path',async()=>{
 const input=project(),clip=input.compositions.root!.content.tracks[0]!.clips[0]!
 if('transform' in clip)Object.assign(clip,{transform:{...clip.transform,position:{mode:'xy',x:{mode:'constant',value:40},y:{mode:'constant',value:50}}}})
 expect(read(input)).toEqual({mode:'constant',value:40})
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'title',property:'x',tick:0,value:60}])
 expect(read(output)).toMatchObject({mode:'keyframes',timeBase:1000000,keyframes:[{tick:0,value:60}]})
 const changed=output.compositions.root!.content.tracks[0]!.clips[0]!
 expect('transform' in changed&&changed.transform.x).toEqual('transform' in clip&&clip.transform.x)
 const staticPatch=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{x:70}}]);expect(read(staticPatch)).toEqual({mode:'constant',value:70})
 if('transform' in clip)Object.assign(clip,{transform:{...clip.transform,position:{mode:'path',path:{nodes:[{id:'node',point:{x:0,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]},progress:{mode:'constant',value:0}}}})
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'title',property:'x',tick:0,value:60}])).rejects.toThrow('inactive')
 const path=await evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'title',envelope:'path',property:'progress',tick:0,value:0},{op:'upsert_keyframe',clipId:'title',envelope:'path',property:'progress',tick:1000000,value:1}])
 expect(read(path,'progress','path')).toMatchObject({mode:'keyframes',keyframes:[{value:0},{value:1}]})
})
it('rejects locked targets, invalid modes/tangents, unsupported envelopes and non-finite values',async()=>{
 const input=project(),before=JSON.stringify(input)
 for(const operation of [{op:'upsert_keyframe',value:Infinity,tick:0},{op:'set_keyframe_tangent_mode',ticks:[0],mode:'wrong'},{op:'edit_keyframe_tangents',tick:0,outgoing:{x:1.5,y:0.5}},{op:'delete_keyframe',tick:10},{op:'upsert_keyframe',tick:0,value:1,envelope:'mask',maskId:'mask'}])await expect(evaluateNativeAgentBatch(input,'root',[{clipId:'title',property:'x',...operation}])).rejects.toThrow()
 const locked={...input,compositions:{...input.compositions,root:{...input.compositions.root!,content:{...input.compositions.root!.content,tracks:input.compositions.root!.content.tracks.map(track=>({...track,locked:true}))}}}}
 expect(read(locked)).toMatchObject({mode:'keyframes'})
 await expect(evaluateNativeAgentBatch(locked,'root',[{op:'upsert_keyframe',clipId:'title',property:'x',tick:0,value:1}])).rejects.toThrow('locked')
 expect(JSON.stringify(input)).toBe(before)
})
it('maps audio gain/pan to native audio and embedded-video envelopes while preserving unrelated channels',async()=>{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000FF'},{media:{id:'media',kind:'video',durationTicks:4000000,width:16,height:16,hasAudio:true}})
 doc=addTrack(doc,{id:'video',kind:'video',name:'V',locked:false,hidden:false,muted:false,clips:[]})
 doc=addClip(doc,'video',{id:'picture',kind:'video',sourceId:'media',timelineStartTicks:1000000,sourceInTicks:0,sourceOutTicks:2000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:true,audioGain:1})
 doc=addTrack(doc,{id:'audio',kind:'audio',name:'A',locked:false,muted:false,clips:[]})
 doc=addClip(doc,'audio',{id:'sound',kind:'audio',sourceId:'media',timelineStartTicks:1500000,sourceInTicks:0,sourceOutTicks:1000000,gain:1})
 const input:NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(doc),instances:[]}}}
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'picture',envelope:'audio',property:'gain',tick:0,value:0.25},{op:'upsert_keyframe',clipId:'sound',envelope:'audio',property:'pan',tick:0,value:-0.5}])
 expect(readNativeKeyframes(output,'root',{clipId:'picture',envelope:'audio',property:'gain'})).toMatchObject({mode:'keyframes',keyframes:[{value:0.25,t_us:1000000}]})
 expect(readNativeKeyframes(output,'root',{clipId:'sound',envelope:'audio',property:'pan'})).toMatchObject({mode:'keyframes',keyframes:[{value:-0.5,t_us:1500000}]})
 expect(readNativeKeyframes(output,'root',{clipId:'picture',property:'opacity'})).toEqual({mode:'constant',value:1})
 expect(readNativeKeyframes(output,'root',{clipId:'sound',envelope:'audio',property:'gain'})).toEqual({mode:'constant',value:1})
})

it('sets all scalar extrapolation modes and preserves signed keys and the authored clock',async()=>{
 const input=project(),before=read(input)
 if(before.mode!=='keyframes')throw new Error('Key track required')
 for(const extrapolation of ['hold','continue','loop','ping_pong','offset'])for(const side of ['before','after']){
  const output=await evaluateNativeAgentBatch(input,'root',[{op:'set_keyframe_extrapolation',clipId:'title',property:'x',side,extrapolation}])
  expect(read(output)).toMatchObject({...before,extrapolation:{before:'hold',after:'hold',...before.extrapolation,[side]:extrapolation}})
 }
 for(const patch of [{side:'bad',extrapolation:'hold'},{side:'after',extrapolation:'wrong'}])await expect(evaluateNativeAgentBatch(input,'root',[{op:'set_keyframe_extrapolation',clipId:'title',property:'x',...patch}])).rejects.toThrow('Invalid')
})

it('reports clocked local and timeline times while retaining authored ticks for editing', () => {
 const input=project(),clip=input.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip)||clip.transform.x.mode!=='keyframes')throw new Error('Expected keyed fixture')
 Object.assign(clip.transform.x.track,{sampleClock:{rateNum:2,rateDen:1,offsetNum:200000}})
 const before=JSON.stringify(input)
 expect(read(input)).toMatchObject({sampleClock:{rateNum:2,rateDen:1,offsetNum:200000},keyframes:[{tick:-1000,t_local_us:-600000,t_us:400000},{tick:0,t_local_us:-100000,t_us:900000},{tick:1000,t_local_us:400000,t_us:1400000}]})
 expect(JSON.stringify(input)).toBe(before)
})
