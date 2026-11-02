import {afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {evaluateNativeAgentBatch} from './nativeBatch'
import {createComposition,addTrack,addClip} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
const track={id:'titles',kind:'text' as const,name:'Titles',hidden:false,locked:false,clips:[]}
const clip={id:'title',kind:'text' as const,text:'Original',timelineStartTicks:1000000,durationTicks:2000000,x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center' as const}}
function project():api.NativeNestedProject{
 let doc=createComposition({width:320,height:180,fps:30000/1001,fpsRate:{numerator:30000,denominator:1001},backgroundColor:'#000000FF'})
 doc=addClip(addTrack(doc,track),'titles',clip)
 const content=buildCompositionWireDocument(doc)
 return {schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},untouched:{content:structuredClone(content),instances:[]}}}
}
afterEach(()=>vi.restoreAllMocks())
it('adds authored layers/tracks without round-tripping retained native content or fractional canvas clocks',async()=>{
 const input=project(),before=JSON.stringify(input)
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'add_track',track:{...track,id:'new'}},{op:'add_layer',trackId:'new',clip:{...clip,id:'new-title'}},{op:'update_track',trackId:'new',patch:{name:'Renamed',hidden:true}},{op:'update_layer',clipId:'new-title',patch:{text:'Edited',x:12,opacity:0.5}},{op:'reorder_track',trackId:'new',index:0}])
 expect(JSON.stringify(input)).toBe(before);expect(output.compositions.untouched).toBe(input.compositions.untouched)
 expect(output.compositions.root!.content.tracks[1]).toBe(input.compositions.root!.content.tracks[0])
 expect(output.compositions.root!.content.canvas).toBe(input.compositions.root!.content.canvas)
 expect(output.compositions.root!.content.tracks[0]).toMatchObject({name:'Renamed',hidden:true,clips:[{text:'Edited',transform:{x:{mode:'constant',value:12}},opacity:{mode:'constant',value:0.5}}]})
})
it('duplicates, splits and trims using existing native rebase/window helpers and deterministic IDs',async()=>{
 const input=project(),ops=[{op:'duplicate_layer',clipId:'title',id:'copy',startTicks:4000000},{op:'split_layer',clipId:'title',atTicks:2000000,rightClipId:'right'},{op:'trim_layer',clipId:'copy',startTicks:4500000,endTicks:5500000},{op:'set_layers_enabled',clipIds:['right'],enabled:false}]
 const output=await evaluateNativeAgentBatch(input,'root',ops)
 expect(output).toEqual(await evaluateNativeAgentBatch(input,'root',ops))
 const clips=output.compositions.root!.content.tracks[0]!.clips
 expect(clips).toEqual(expect.arrayContaining([expect.objectContaining({id:'title',timelineStartTick:1000000,timelineEndTick:2000000}),expect.objectContaining({id:'right',timelineStartTick:2000000,timelineEndTick:3000000,enabled:false}),expect.objectContaining({id:'copy',timelineStartTick:4500000,timelineEndTick:5500000})]))
})
it('delegates moves to Rust and retains unrelated nodes even when native serde normalizes them',async()=>{
 const input=project(),destination={...track,id:'destination'},prepared=await evaluateNativeAgentBatch(input,'root',[{op:'add_track',track:destination}])
 const wire=prepared.compositions.root!.content.tracks[0]!.clips[0]!
 const moved={...prepared,compositions:{...prepared.compositions,root:{...prepared.compositions.root!,content:{...prepared.compositions.root!.content,tracks:[{...prepared.compositions.root!.content.tracks[0]!,clips:[]},{...prepared.compositions.root!.content.tracks[1]!,clips:[{...wire,timelineStartTick:4000000,timelineEndTick:6000000}]}] as typeof prepared.compositions.root.content.tracks}},untouched:{...prepared.compositions.untouched!,durationUs:123}}}
 const move=vi.spyOn(api,'moveNativeNestedClips').mockResolvedValue({project:moved,changed:true,affectedCompositions:['root'],movedClipIds:['title']})
 const output=await evaluateNativeAgentBatch(prepared,'root',[{op:'move_layer',clipId:'title',trackId:'destination',startTicks:4000000}])
 expect(move.mock.calls[0]![1]).toEqual([{fromComposition:'root',fromTrackId:'titles',clipId:'title',toComposition:'root',toTrackId:'destination',startUs:4000000,createTrack:false}])
 expect(output.compositions.untouched).toBe(prepared.compositions.untouched)
})
it('rejects late invalid/unsupported operations, track locks and lossy per-layer locks without changing input',async()=>{
 const input=project(),before=JSON.stringify(input)
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'delete_layer',clipId:'title'},{op:'not-real'}])).rejects.toThrow('Unsupported')
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_track',trackId:'titles',patch:{locked:true}},{op:'update_layer',clipId:'title',patch:{text:'No'}}])).rejects.toThrow('locked')
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{unknown:true}}])).rejects.toThrow('Unsupported')
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'add_layer',trackId:'titles',clip:{...clip,id:'locked',locked:true}}])).rejects.toThrow('locks')
 await expect(evaluateNativeAgentBatch(input,'missing',[{op:'delete_layer',clipId:'title'}])).rejects.toThrow('Composition')
 expect(JSON.stringify(input)).toBe(before)
})
it('preserves exact canvas fields not patched and supports link creation/removal and explicit unlock/rename',async()=>{
 const input=project(),second={...clip,id:'second',timelineStartTicks:4000000}
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'add_layer',trackId:'titles',clip:second},{op:'create_link',id:'group',memberIds:['title','second']},{op:'remove_link',id:'group'},{op:'update_track',trackId:'titles',patch:{locked:true}},{op:'update_track',trackId:'titles',patch:{name:'Unlocked',locked:false}},{op:'update_canvas',patch:{width:640}},{op:'delete_layer',clipId:'second'}])
 expect(output.compositions.root!.content.canvas).toEqual({...input.compositions.root!.content.canvas,width:640})
 expect(output.compositions.root!.content.tracks[0]).toMatchObject({name:'Unlocked',locked:false,clips:[{id:'title'}]})
 expect(output.compositions.root!.content.metadata?.clipLinks).toEqual({})
})

it('rebinds existing matte dependencies by track identity when inserting/reordering native layers',async()=>{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000FF'},{image:{id:'image',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}})
 for(const id of ['dependent','control'])doc=addTrack(doc,{id,kind:'image',name:id,hidden:false,locked:false,clips:[]})
 doc=addClip(doc,'dependent',{id:'image-clip',kind:'image',sourceId:'image',timelineStartTicks:0,durationTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,effects:[{kind:'track_matte',parameters:{matte:2,reverse:true}}]})
 const input:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(doc),instances:[]}}}
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'add_track',index:0,track:{id:'prefix',kind:'image',name:'Prefix',hidden:false,locked:false,clips:[]}},{op:'reorder_track',trackId:'control',index:1}])
 const dependent=output.compositions.root!.content.tracks.find(track=>track.id==='dependent')!
 expect('effects' in dependent.clips[0]!&&dependent.clips[0].effects?.[0]).toEqual({kind:'track_matte',parameters:{matte:2,reverse:true}})
 const inserted=await evaluateNativeAgentBatch(input,'root',[{op:'add_track',index:0,track:{id:'prefix',kind:'image',name:'Prefix',hidden:false,locked:false,clips:[]}}])
 const shifted=inserted.compositions.root!.content.tracks.find(track=>track.id==='dependent')!
 expect('effects' in shifted.clips[0]!&&shifted.clips[0].effects?.[0]).toMatchObject({parameters:{matte:3}})
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'delete_track',trackId:'control'}])).rejects.toThrow('используется')
})
it('detaches operation arguments before awaiting a native move',async()=>{
 const input=project();let release!:(value:api.NativeClipMoveResult)=>void
 vi.spyOn(api,'moveNativeNestedClips').mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const patch={text:'Captured text'},ops=[{op:'move_layer',clipId:'title',trackId:'titles',startTicks:1000000},{op:'update_layer',clipId:'title',patch}]
 const pending=evaluateNativeAgentBatch(input,'root',ops);patch.text='Changed during await'
 release({project:input,changed:false,affectedCompositions:[],movedClipIds:[]})
 const output=await pending
 expect(output.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'Captured text'})
})

it('adds/removes a transition without modifying its neighboring clips and deletes an empty track',async()=>{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000FF'},{source:{id:'source',kind:'video',durationTicks:4000000,width:16,height:16,hasAudio:false}})
 doc=addTrack(doc,{id:'video',kind:'video',name:'V',hidden:false,muted:false,locked:false,clips:[]})
 for(const [id,start] of [['one',0],['two',1000000]] as const)doc=addClip(doc,'video',{id,kind:'video',sourceId:'source',timelineStartTicks:start,sourceInTicks:start,sourceOutTicks:start+1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,audioGain:1,sourceAudioEnabled:false})
 const input:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(doc),instances:[]}}}
 const transition={id:'join',kind:'dissolve',fromClipId:'one',toClipId:'two',durationTicks:500000}
 const added=await evaluateNativeAgentBatch(input,'root',[{op:'add_transition',trackId:'video',transition}])
 expect(added.compositions.root!.content.tracks[0]).toMatchObject({transitions:[transition]})
 expect(added.compositions.root!.content.tracks[0]!.clips).toEqual(input.compositions.root!.content.tracks[0]!.clips)
 const removed=await evaluateNativeAgentBatch(added,'root',[{op:'remove_transition',trackId:'video',transitionId:'join'},{op:'add_track',track:{...track,id:'temporary'}},{op:'delete_track',trackId:'temporary'}])
 expect(removed.compositions.root!.content.tracks).toEqual(input.compositions.root!.content.tracks)
})
it('delegates one grouped move preserving relative offsets',async()=>{
 const input=await evaluateNativeAgentBatch(project(),'root',[{op:'add_layer',trackId:'titles',clip:{...clip,id:'second',timelineStartTicks:4000000}},{op:'add_track',track:{...track,id:'destination'}}])
 const move=vi.spyOn(api,'moveNativeNestedClips').mockResolvedValue({project:input,changed:false,affectedCompositions:[],movedClipIds:[]})
 await evaluateNativeAgentBatch(input,'root',[{op:'move_layers',clipIds:['second','title'],trackId:'destination',startTicks:7000000}])
 expect(move).toHaveBeenCalledTimes(1)
 expect(move.mock.calls[0]![1].map(item=>({id:item.clipId,start:item.startUs,target:item.toTrackId}))).toEqual([{id:'second',start:7000000,target:'destination'},{id:'title',start:4000000,target:'titles'}])
})
it('duplicates into an explicit lane and can escape a source link without altering the original group',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!,original=source.clips[0]!
 Object.assign(node.content,{metadata:{clipLinks:{source:['title','companion']}},tracks:[{...source,clips:[...source.clips,{...original,id:'companion',timelineStartTick:4000000,timelineEndTick:6000000}]},{...source,id:'target',clips:[]}]})
 const before=JSON.stringify(input)
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(staged,moves)=>{
  expect(moves).toHaveLength(1);expect(moves[0]).toMatchObject({clipId:'copy',toTrackId:'target',startUs:1000000})
  expect(staged.compositions.root!.content.metadata?.clipLinks).toEqual({source:['title','companion']})
  const scratch=staged.compositions[moves[0]!.fromComposition]!
  expect(scratch.content.tracks.flatMap(track=>track.clips.map(clip=>clip.id))).toEqual(['copy'])
  expect(scratch.content.metadata?.clipLinks).toEqual({})
  return {project:staged,changed:true,affectedCompositions:['root'],movedClipIds:['copy']}
 })
 await evaluateNativeAgentBatch(input,'root',[{op:'duplicate_layer',clipId:'title',id:'copy',trackId:'target',startTicks:1000000,escapeLink:true}])
 expect(move).toHaveBeenCalledTimes(1);expect(JSON.stringify(input)).toBe(before)
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'duplicate_layer',clipId:'title',id:'bad',escapeLink:'yes'}])).rejects.toThrow('boolean')
})
it('escapeLink trims and splits only the target while retaining links to untouched locked partners',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!,partner={...source.clips[0]!,id:'partner'}
 Object.assign(node.content,{metadata:{clipLinks:{group:['title','partner']}},tracks:[source,{...source,id:'partner-lane',locked:true,clips:[partner]}]})
 const before=JSON.stringify(input)
 const trimmed=await evaluateNativeAgentBatch(input,'root',[{op:'trim_layer',clipId:'title',startTicks:1250000,endTicks:2750000,escapeLink:true}])
 expect(trimmed.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({timelineStartTick:1250000,timelineEndTick:2750000})
 expect(trimmed.compositions.root!.content.tracks[1]).toEqual(node.content.tracks[1]);expect(trimmed.compositions.root!.content.metadata?.clipLinks).toEqual({group:['title','partner']})
 const split=await evaluateNativeAgentBatch(input,'root',[{op:'split_layer',clipId:'title',atTicks:2000000,rightClipId:'right-only',escapeLink:true}])
 expect(split.compositions.root!.content.tracks[0]!.clips.map(clip=>clip.id)).toEqual(['title','right-only'])
 expect(split.compositions.root!.content.tracks[1]).toEqual(node.content.tracks[1]);expect(split.compositions.root!.content.metadata?.clipLinks).toEqual({group:['title','partner']})
 expect(JSON.stringify(input)).toBe(before)
})
it('escapeLink moves only the target and restores the authored group after native admission',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!
 Object.assign(node.content,{metadata:{clipLinks:{group:['title','partner']}},tracks:[source,{...source,id:'partners',locked:true,clips:[{...source.clips[0]!,id:'partner'}]}]})
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
  expect(project.compositions.root!.content.metadata?.clipLinks).toEqual({});expect(moves).toHaveLength(1)
  const result=structuredClone(project);Object.assign(result.compositions.root!.content.tracks[0]!.clips[0]!,{timelineStartTick:4000000,timelineEndTick:6000000})
  return {project:result,changed:true,affectedCompositions:['root'],movedClipIds:['title']}
 })
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'move_layer',clipId:'title',trackId:'titles',startTicks:4000000,escapeLink:true}])
 expect(move).toHaveBeenCalledTimes(1);expect(output.compositions.root!.content.metadata?.clipLinks).toEqual({group:['title','partner']})
 expect(output.compositions.root!.content.tracks[1]).toEqual(node.content.tracks[1])
})

it('escapeLink no-op keeps the exact authored group ordering and document bytes',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!
 Object.assign(node.content,{metadata:{clipLinks:{first:['title','partner'],second:['other-a','other-b']}},tracks:[source,{...source,id:'others',clips:[{...source.clips[0]!,id:'partner'},{...source.clips[0]!,id:'other-a',timelineStartTick:4000000,timelineEndTick:6000000},{...source.clips[0]!,id:'other-b',timelineStartTick:7000000,timelineEndTick:9000000}]}]})
 const before=JSON.stringify(input)
 const result=await evaluateNativeAgentBatch(input,'root',[{op:'trim_layer',clipId:'title',startTicks:1000000,endTicks:3000000,escapeLink:true}])
 expect(JSON.stringify(result)).toBe(before)
})

it('anchors native multi-layer moves to the first subject and retains companion lanes and authored links',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!
 Object.assign(node.content,{metadata:{clipLinks:{group:['title','early','unselected']}},tracks:[source,{...source,id:'early-lane',clips:[{...source.clips[0]!,id:'early',timelineStartTick:0,timelineEndTick:1000000},{...source.clips[0]!,id:'unselected',timelineStartTick:7000000,timelineEndTick:9000000}]},{...source,id:'target',clips:[]}]})
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
  expect(moves.map(move=>[move.clipId,move.toTrackId,move.startUs])).toEqual([['title','target',4000000],['early','early-lane',3000000]])
  expect(project.compositions.root!.content.metadata?.clipLinks).toEqual({})
  return {project,changed:true,affectedCompositions:['root'],movedClipIds:['title','early']}
 })
 const result=await evaluateNativeAgentBatch(input,'root',[{op:'move_layers',clipIds:['title','early'],trackId:'target',startTicks:4000000}])
 expect(move).toHaveBeenCalledTimes(1);expect(result.compositions.root!.content.metadata?.clipLinks).toEqual({group:['title','early','unselected']})
})
it('keeps linked audio on its own lane when the visual anchor changes lane',async()=>{
 let authored=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'},{media:{id:'media',kind:'video',durationTicks:5000000,width:16,height:16,hasAudio:true}})
 authored=addClip(addTrack(authored,track),'titles',clip)
 authored=addTrack(authored,{id:'sound-lane',kind:'audio',name:'Sound',locked:false,muted:false,clips:[]})
 authored=addClip(authored,'sound-lane',{id:'sound',kind:'audio',sourceId:'media',timelineStartTicks:0,sourceInTicks:0,sourceOutTicks:1000000,gain:1})
 authored=addTrack(authored,{...track,id:'target'})
 const content=buildCompositionWireDocument(authored),input:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:{...content,metadata:{clipLinks:{group:['title','sound']}}},instances:[]}}}
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
  expect(moves.map(move=>[move.clipId,move.toTrackId,move.startUs])).toEqual([['title','target',4000000],['sound','sound-lane',3000000]])
  return {project,changed:false,affectedCompositions:[],movedClipIds:[]}
 })
 await evaluateNativeAgentBatch(input,'root',[{op:'move_layer',clipId:'title',trackId:'target',startTicks:4000000}]);expect(move).toHaveBeenCalledTimes(1)
})
it('supports explicit native per-clip lanes with an anchor independent of placement order',async()=>{
 const input=project(),node=input.compositions.root!,source=node.content.tracks[0]!
 Object.assign(node.content,{metadata:{clipLinks:{group:['title','early']}},tracks:[source,{...source,id:'other',clips:[{...source.clips[0]!,id:'early',timelineStartTick:0,timelineEndTick:1000000}]},{...source,id:'target-a',clips:[]},{...source,id:'target-b',clips:[]}]})
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
  expect(moves.map(move=>[move.clipId,move.toTrackId,move.startUs])).toEqual([['title','target-a',4000000],['early','target-b',3000000]])
  return {project,changed:false,affectedCompositions:[],movedClipIds:[]}
 })
 const result=await evaluateNativeAgentBatch(input,'root',[{op:'move_layer_placements',placements:[{clipId:'early',trackId:'target-b'},{clipId:'title',trackId:'target-a'}],anchorId:'title',startTicks:4000000}])
 expect(move).toHaveBeenCalledTimes(1);expect(result.compositions.root!.content.metadata?.clipLinks).toEqual({group:['title','early']})
 for(const op of [{placements:[{clipId:'title',trackId:'target-a'}],anchorId:'early'},{placements:[{clipId:'title',trackId:'target-a'},{clipId:'title',trackId:'target-b'}],anchorId:'title'},{placements:[{clipId:'title',trackId:'target-a',extra:true}],anchorId:'title'}])await expect(evaluateNativeAgentBatch(input,'root',[{op:'move_layer_placements',startTicks:4000000,...op}])).rejects.toThrow()
 expect(move).toHaveBeenCalledTimes(1)
})


it('patches native static audio controls with exact channel mapping and atomic refusal',async()=>{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#000000FF'},{media:{id:'media',kind:'video',durationTicks:4000000,width:16,height:16,hasAudio:true}})
 doc=addTrack(doc,{id:'video',kind:'video',name:'V',locked:false,hidden:false,muted:false,clips:[]})
 doc=addClip(doc,'video',{id:'picture',kind:'video',sourceId:'media',timelineStartTicks:1000000,sourceInTicks:0,sourceOutTicks:2000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:1,sourceAudioEnabled:true,audioGain:1})
 doc=addTrack(doc,{id:'audio',kind:'audio',name:'A',locked:false,muted:false,clips:[]})
 doc=addClip(doc,'audio',{id:'sound',kind:'audio',sourceId:'media',timelineStartTicks:1500000,sourceInTicks:0,sourceOutTicks:1000000,gain:1})
 const input:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(doc),instances:[]}}},before=JSON.stringify(input)
 const animated=await evaluateNativeAgentBatch(input,'root',[{op:'upsert_keyframe',clipId:'sound',envelope:'audio',property:'gain',tick:0,value:0.5}])
 const output=await evaluateNativeAgentBatch(animated,'root',[{op:'update_layer',clipId:'picture',patch:{audioGain:0.25,audioPan:-1,sourceAudioEnabled:false}},{op:'update_layer',clipId:'sound',patch:{gain:16,pan:1}}])
 expect(output.compositions.root!.content.tracks[0]!.clips[0]).toEqual({...input.compositions.root!.content.tracks[0]!.clips[0],audioGain:{mode:'constant',value:0.25},audioPan:{mode:'constant',value:-1},sourceAudioEnabled:false})
 expect(output.compositions.root!.content.tracks[1]!.clips[0]).toEqual({...input.compositions.root!.content.tracks[1]!.clips[0],gain:{mode:'constant',value:16},pan:{mode:'constant',value:1}})
 for(const [clipId,patch] of [['sound',{gain:-1}],['sound',{gain:17}],['sound',{pan:1.01}],['sound',{pan:NaN}],['picture',{audioGain:Infinity}],['picture',{gain:1}],['sound',{audioPan:0}],['sound',{sourceAudioEnabled:true}],['picture',{sourceAudioEnabled:1}]] as const)
  await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'sound',patch:{gain:0}},{op:'update_layer',clipId,patch}])).rejects.toThrow()
 const silent=structuredClone(input);Object.assign(silent.compositions.root!.content.sources.media!,{hasAudio:false})
 await expect(evaluateNativeAgentBatch(silent,'root',[{op:'update_layer',clipId:'picture',patch:{sourceAudioEnabled:true}}])).rejects.toThrow('does not contain audio')
 expect(JSON.stringify(input)).toBe(before)
})


it('sets every supported native image blend without losing retained effects or transforms',async()=>{
 let doc=createComposition({width:16,height:16,fps:10,backgroundColor:'#808080FF'},{image:{id:'image',kind:'image',durationTicks:0,width:16,height:16,hasAudio:false}})
 doc=addTrack(doc,{id:'pictures',kind:'image',name:'Images',hidden:false,locked:false,clips:[]})
 doc=addClip(doc,'pictures',{id:'picture',kind:'image',sourceId:'image',timelineStartTicks:0,durationTicks:1000000,transform:{x:0,y:0,width:16,height:16,fit:'contain'},opacity:0.75,effects:[]})
 const input:api.NativeNestedProject={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(doc),instances:[]}}},before=JSON.stringify(input)
 for(const blendMode of ['normal','multiply','screen','overlay','darken','lighten','difference','addition']){
  const output=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'picture',patch:{blendMode}}])
  expect(output.compositions.root!.content.tracks[0]!.clips[0]).toEqual({...input.compositions.root!.content.tracks[0]!.clips[0],blendMode})
 }
 for(const blendMode of ['unknown','add',null,1])await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'picture',patch:{blendMode}}])).rejects.toThrow('blend mode')
 await expect(evaluateNativeAgentBatch(project(),'root',[{op:'update_layer',clipId:'title',patch:{blendMode:'screen'}}])).rejects.toThrow('blend mode')
 expect(JSON.stringify(input)).toBe(before)
})


it('merges validated authored text-style fields without round-tripping retained native clips',async()=>{
 const input=project(),before=JSON.stringify(input),original=input.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('style' in original))throw new Error('Text fixture required')
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{style:{fontSizePx:72,color:'#FF000080',backgroundColor:'#01020304',strokeColor:'#00FF00FF',strokeWidthPx:3,shadowColor:'#0000FFFF',shadowX:-7,shadowY:9}}}])
 const changed=output.compositions.root!.content.tracks[0]!.clips[0]!
 expect(changed).toEqual({...original,style:{...original.style,fontSize:72,color:{red:1,green:0,blue:0,alpha:128/255},background:{red:1/255,green:2/255,blue:3/255,alpha:4/255},stroke:{red:0,green:1,blue:0,alpha:1},strokeWidth:3,shadow:{red:0,green:0,blue:1,alpha:1},shadowX:-7,shadowY:9}})
 const partial=await evaluateNativeAgentBatch(output,'root',[{op:'update_layer',clipId:'title',patch:{style:{fontSizePx:12,align:'right'}}}])
 expect(partial.compositions.root!.content.tracks[0]!.clips[0]).toEqual({...changed,style:{...('style' in changed?changed.style:{}),fontSize:12,align:'right'}})
 expect(output.compositions.untouched).toBe(input.compositions.untouched)
 for(const style of [{},{align:'invalid'},{unknown:true},{fontSizePx:0},{fontSizePx:513},{color:'red'},{fontFamily:'Unknown font'},{strokeWidthPx:-1},{strokeWidthPx:101},{shadowX:Infinity},null])await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{text:'Candidate'}},{op:'update_layer',clipId:'title',patch:{style}}])).rejects.toThrow()
 const locked=await evaluateNativeAgentBatch(input,'root',[{op:'update_track',trackId:'titles',patch:{locked:true}}])
 await expect(evaluateNativeAgentBatch(locked,'root',[{op:'update_layer',clipId:'title',patch:{style:{fontSizePx:24}}}])).rejects.toThrow('locked')
 expect(JSON.stringify(input)).toBe(before)
})


it('retains Concat title style channels through partial changes and refuses inactive ordinary style writes',async()=>{
 const input=project(),before=JSON.stringify(input)
 const retained=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{concatTitle:{fontFamily:'Hanken Grotesk',fontSize:0.1,fontWeight:700,italic:true,lineHeight:1.4,tracking:0.003,maxWidth:0.5,background:'#12345680'}}}])
 const changed=await evaluateNativeAgentBatch(retained,'root',[{op:'update_layer',clipId:'title',patch:{text:'Edited',concatTitle:{italic:false,align:'right'}}}])
 expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'Edited',concatTitle:{fontFamily:'Hanken Grotesk',fontSize:0.1,fontWeight:700,italic:false,lineHeight:1.4,tracking:0.003,maxWidth:0.5,background:'#12345680',align:'right'}})
 await expect(evaluateNativeAgentBatch(retained,'root',[{op:'update_layer',clipId:'title',patch:{style:{fontSizePx:24}}}])).rejects.toThrow('concatTitle')
 await expect(evaluateNativeAgentBatch(retained,'root',[{op:'update_layer',clipId:'title',patch:{concatTitle:{unknown:true}}}])).rejects.toThrow('Unsupported')
 expect(JSON.stringify(input)).toBe(before)
})


it('edits all original Concat title blends without replacing typography and rejects non-source modes',async()=>{
 const input=await evaluateNativeAgentBatch(project(),'root',[{op:'update_layer',clipId:'title',patch:{concatTitle:{fontFamily:'Hanken Grotesk',fontWeight:700,color:'#ff8040ff'}}}])
 for(const blendMode of ['normal','multiply','screen','addition','lighten','darken']){
  const changed=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{blendMode}}])
  expect(changed.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({blendMode,concatTitle:{fontFamily:'Hanken Grotesk',fontWeight:700,color:'#ff8040ff'}})
 }
 for(const blendMode of ['overlay','difference','add','unknown'])await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{blendMode}}])).rejects.toThrow('blend mode')
})


it('replaces ordered retained-title effect stacks atomically with detached wire data and canonical clearing',async()=>{
 const input=project(),before=JSON.stringify(input),effects=[{kind:'concat_exposure',parameters:{stops:1}},{kind:'concat_contrast',parameters:{amount:40}}]
 const output=await evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{effects,concatTitle:{fontSize:0.15}}}])
 expect(output.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({effects,concatTitle:{fontSize:0.15}})
 effects.reverse();expect(output.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({effects:[{kind:'concat_exposure'},{kind:'concat_contrast'}]})
 const cleared=await evaluateNativeAgentBatch(output,'root',[{op:'update_layer',clipId:'title',patch:{effects:[]}}]);expect(cleared.compositions.root!.content.tracks[0]!.clips[0]).not.toHaveProperty('effects')
 for(const effects of [null,{},[null],[{}]])await expect(evaluateNativeAgentBatch(output,'root',[{op:'update_layer',clipId:'title',patch:{effects}}])).rejects.toThrow()
 await expect(evaluateNativeAgentBatch(input,'root',[{op:'update_layer',clipId:'title',patch:{effects:[]}}])).rejects.toThrow('visual layer')
 expect(JSON.stringify(input)).toBe(before)
})
