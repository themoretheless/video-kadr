import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {executeAgentRequest} from './runtime.svelte'
import {agentActivity} from './activity.svelte'
import {agentSession} from './session.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {closeNestedComposition,nestedComposition,nestedHistory,adoptImportedConcatProject} from '../state/nestedComposition.svelte'
import {createComposition,addTrack,addClip} from '../composition/commands'
import {buildCompositionWireDocument} from '../composition/payload'
const clip={id:'title',kind:'text',text:'Native title',timelineStartTicks:0,durationTicks:1000000,x:0,y:0,opacity:1,style:{fontSizePx:48,color:'#FFFFFFFF',align:'center'}}
const operations=[{op:'add_track',track:{id:'new',kind:'text',name:'New',hidden:false,locked:false,clips:[]}},{op:'add_layer',trackId:'new',clip:{...clip,id:'added'}},{op:'update_layer',clipId:'title',patch:{text:'Changed'}}]
const call=(tool:string,args:Record<string,unknown>={},access={})=>executeAgentRequest('batch-agent',tool,args,access)
const read=async()=>await call('get_project') as {projectHash:string;document:api.NativeNestedProject;nativeBatch:{operations:string[]}}
beforeEach(()=>{
 closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests()
 let doc=createComposition({width:320,height:180,fps:10,backgroundColor:'#000000FF'})
 doc=addTrack(doc,{id:'titles',kind:'text',name:'Titles',hidden:false,locked:false,clips:[]});doc=addClip(doc,'titles',clip as Parameters<typeof addClip>[2])
 const content=buildCompositionWireDocument(doc)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]},child:{content:structuredClone(content),instances:[]}}}
 nestedComposition.activeCompositionId='child';nestedComposition.timeUs=500000;nestedComposition.renderedOutputs={root:'old',child:'other'}
 vi.spyOn(api,'nestedDurations').mockResolvedValue({root:1000000,child:1000000})
})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition()})
it('rehearses without mutation and commits the same scoped native candidate as one Undo',async()=>{
 const initial=await read(),ordinary=JSON.stringify(compositionState.document),before=JSON.stringify(nestedComposition.document),depth=nestedHistory.past.length
 expect(initial.nativeBatch.operations).toContain('update_track')
 const args={expectedProjectHash:initial.projectHash,operations}
 const rehearsed=await call('dry_run',args) as {dryRun:boolean;changed:boolean;document:api.NativeNestedProject;projectHash:string}
 expect(rehearsed).toMatchObject({dryRun:true,changed:true,projectHash:initial.projectHash})
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(depth);expect(nestedComposition.renderedOutputs).toEqual({root:'old',child:'other'})
 const applied=await call('apply_batch',args) as {document:api.NativeNestedProject;projectHash:string}
 expect(applied.document).toEqual(rehearsed.document);expect(nestedHistory.past).toHaveLength(depth+1)
 expect(nestedComposition.document?.compositions.child).toEqual(initial.document.compositions.child)
 expect(nestedComposition.document?.compositions.root?.content.tracks[0]?.clips[0]).toMatchObject({text:'Changed'})
 expect(nestedComposition.activeCompositionId).toBe('child');expect(nestedComposition.timeUs).toBe(500000)
 expect(nestedComposition.renderedOutputs).toEqual({});expect(JSON.stringify(compositionState.document)).toBe(ordinary)
 const activity=agentActivity.activities.find(item=>item.tool==='apply_batch')!
 expect(activity.historyIds).toHaveLength(1);expect(activity.effect).toBe('applied')
 expect(activity.affected?.some(item=>item.kind==='Layer'&&item.id==='added'&&item.compositionId==='root')).toBe(true)
 await expect(call('apply_batch',args)).rejects.toThrow('Project changed')
 await call('undo');expect(JSON.stringify(nestedComposition.document)).toBe(before)
 expect(agentActivity.activities.find(item=>item.id===activity.id)?.effect).toBe('reverted')
 await expect(call('apply_batch',args)).rejects.toThrow('Project changed')
 await call('redo');expect(nestedComposition.document).toEqual(applied.document)
})
it('rejects a late invalid operation or failed native validation atomically',async()=>{
 const initial=await read(),before=JSON.stringify(nestedComposition.document),cached=JSON.stringify(nestedComposition.renderedOutputs)
 await expect(call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[operations[0],{op:'not-real'}]})).rejects.toThrow('Unsupported')
 expect(api.nestedDurations).not.toHaveBeenCalled()
 vi.mocked(api.nestedDurations).mockRejectedValue(new Error('Invalid native candidate'))
 await expect(call('apply_batch',{expectedProjectHash:initial.projectHash,operations})).rejects.toThrow('Invalid native candidate')
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toEqual([]);expect(JSON.stringify(nestedComposition.renderedOutputs)).toBe(cached)
 expect(nestedHistory.busy).toBe(false)
})
it('supports explicit repeated-ID composition scope and rejects history locks and mixed admission tokens',async()=>{
 const initial=await read()
 agentSession.lock={owner:'other',reason:'Hold',expiresAt:Date.now()+60000}
 await expect(call('dry_run',{expectedProjectHash:initial.projectHash,operations})).rejects.toThrow('Hold')
 agentSession.lock=null
 await expect(call('apply_batch',{expectedProjectHash:initial.projectHash,expectedRevision:0,operations})).rejects.toThrow('expectedProjectHash')
 await call('apply_batch',{expectedProjectHash:initial.projectHash,composition_id:'child',operations:[{op:'update_layer',clipId:'title',patch:{text:'Child only'}}]})
 expect(nestedComposition.document?.compositions.root).toEqual(initial.document.compositions.root)
 expect(nestedComposition.document?.compositions.child?.content.tracks[0]?.clips[0]).toMatchObject({text:'Child only'})
})
it('keeps no-op batches out of Undo and preserves native render caches',async()=>{
 const initial=await read()
 const result=await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'Native title'}}]})
 expect(result).toMatchObject({changed:false,projectHash:initial.projectHash})
 expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.renderedOutputs).toEqual({root:'old',child:'other'})
 expect(agentActivity.activities.at(-1)).toMatchObject({historyIds:[],state:'done'})
})
it('checks editor admission again after native validation before committing',async()=>{
 const initial=await read(),before=JSON.stringify(nestedComposition.document)
 let live=true,release!:(value:Record<string,number>)=>void
 vi.mocked(api.nestedDurations).mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const pending=call('apply_batch',{expectedProjectHash:initial.projectHash,operations},{isActive:()=>live})
 await vi.waitFor(()=>expect(api.nestedDurations).toHaveBeenCalledTimes(1));live=false
 const failed=expect(pending).rejects.toThrow('expired or disconnected');release({root:1000000,child:1000000});await failed
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
})
it('reads and atomically edits native keyframes through the actual scoped MCP dispatcher',async()=>{
 const initial=await read(),before=JSON.stringify(nestedComposition.document)
 expect(await call('get_keyframes',{clipId:'title',property:'opacity',composition_id:'child'})).toMatchObject({kind:'nested',mode:'constant',value:1,projectHash:initial.projectHash})
 const operations=[{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:0,value:0},{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:1000000,value:1},{op:'set_keyframe_preset',clipId:'title',property:'opacity',ticks:[0],preset:'ease_in_out'}]
 await call('dry_run',{expectedProjectHash:initial.projectHash,composition_id:'child',operations})
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,composition_id:'child',operations})
 expect(nestedHistory.past).toHaveLength(1)
 expect(await call('get_keyframes',{clipId:'title',property:'opacity'})).toMatchObject({mode:'constant',value:1})
 expect(await call('get_keyframes',{clipId:'title',property:'opacity',composition_id:'child'})).toMatchObject({mode:'keyframes',timeBase:1000000,keyframes:[{tick:0,value:0,preset_id:'ease_in_out'},{tick:1000000,value:1}]})
 await call('undo');expect(JSON.stringify(nestedComposition.document)).toBe(before)
 await call('redo');expect(await call('get_keyframes',{clipId:'title',property:'opacity',composition_id:'child'})).toMatchObject({mode:'keyframes'})
})

it('routes native color reads through explicit scope with the current hash and no history mutation',async()=>{
 const before=await read(),depth=nestedHistory.past.length
 const child=nestedComposition.document!.compositions.child!.content.tracks[0]!.clips[0]!
 Object.assign(child,{colorAnimation:{keys:[{t_us:-0.5,value:{r:1,g:2,b:3,a:4},in_:[0.6,0.7],out:[0.3,0.4],segment:{kind:'Linear'}}],extrapolate:{before:'Hold',after:'Hold'}}})
 const current=await read()
 expect(await call('get_color_track',{clipId:'title',composition_id:'child'})).toMatchObject({kind:'nested',projectHash:current.projectHash,mode:'keyframes',keyframes:[{tick:-0.5,value:{r:1,g:2,b:3,a:4}}]})
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({mode:'constant',value:{r:255,g:255,b:255,a:255}})
 expect(nestedHistory.past).toHaveLength(depth);expect(nestedComposition.renderedOutputs).toEqual({root:'old',child:'other'})
 expect(nestedComposition.document!.compositions.root).toEqual(before.document.compositions.root)
})

it('rehearses color writes and commits one native Undo with atomic invalid-channel rejection',async()=>{
 const initial=await read(),ops=[{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:128}}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,operations:ops}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:ops})
 expect(nestedComposition.document).toEqual(preview.document);expect(nestedHistory.past).toHaveLength(1)
 const current=await read()
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,operations:[{op:'delete_color_key',clipId:'title',tick:0},{...ops[0],value:{r:256,g:0,b:0,a:128}}]})).rejects.toThrow()
 expect(nestedComposition.document).toEqual(current.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('moves and recolors a native key as one undoable batch',async()=>{
 const base=await read()
 await call('apply_batch',{expectedProjectHash:base.projectHash,operations:[{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:255}}]})
 const before=await read(),depth=nestedHistory.past.length
 const operations=[{op:'edit_color_key',clipId:'title',tick:500000,newTick:-0.25,value:{r:0,g:255,b:0,a:128}}]
 await call('dry_run',{expectedProjectHash:before.projectHash,operations})
 expect(nestedHistory.past).toHaveLength(depth)
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations})
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({keyframes:[{tick:-0.25,value:{r:0,g:255,b:0,a:128}},{tick:0}]})
 expect(nestedHistory.past).toHaveLength(depth+1)
 await call('undo');expect(nestedComposition.document).toEqual(before.document)
})

it('applies native color extrapolation atomically with key creation and restores it through Undo',async()=>{
 const initial=await read(),operations=[{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:255}},{op:'set_color_extrapolation',clipId:'title',side:'before',extrapolation:'ping_pong'},{op:'set_color_extrapolation',clipId:'title',side:'after',extrapolation:'loop'}]
 const rehearsal=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(rehearsal.document)
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({extrapolation:{before:'PingPong',after:'Loop'}})
 expect(nestedHistory.past).toHaveLength(1)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
 await call('redo');expect(nestedComposition.document).toEqual(rehearsal.document)
})

it('moves native color keys simultaneously across layers, permits swaps and rejects final collisions atomically',async()=>{
 const initial=await read()
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'add_layer',trackId:'titles',clip:{...clip,id:'second'}},{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:255}},{op:'set_color_key',clipId:'second',playheadTicks:500000,value:{r:0,g:255,b:0,a:128}}]})
 const before=await read(),depth=nestedHistory.past.length
 const operations=[{op:'move_color_keys',edits:[{clipId:'title',tick:0,newTick:500000},{clipId:'title',tick:500000,newTick:0},{clipId:'second',tick:500000,newTick:-0.5}]}]
 const rehearsal=await call('dry_run',{expectedProjectHash:before.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(depth)
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations})
 expect(nestedComposition.document).toEqual(rehearsal.document);expect(nestedHistory.past).toHaveLength(depth+1)
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({keyframes:[{tick:0,value:{r:255,g:0,b:0,a:255}},{tick:500000,value:{r:255,g:255,b:255,a:255}}]})
 expect(await call('get_color_track',{clipId:'second'})).toMatchObject({keyframes:[{tick:-0.5,value:{r:0,g:255,b:0,a:128}},{tick:0}]})
 const applied=await read()
 for(const edits of [[{clipId:'title',tick:0,newTick:500000}],[{clipId:'title',tick:0,newTick:1},{clipId:'title',tick:0,newTick:2}],[{clipId:'second',tick:-0.5,newTick:1},{clipId:'title',tick:123,newTick:2}]]){
  await expect(call('apply_batch',{expectedProjectHash:applied.projectHash,operations:[{op:'move_color_keys',edits}]})).rejects.toThrow()
  expect(nestedComposition.document).toEqual(applied.document)
 }
 await call('undo');expect(nestedComposition.document).toEqual(before.document)
 await call('redo');expect(nestedComposition.document).toEqual(applied.document)
})

it('changes native color segments through the batch pipeline with exact Undo',async()=>{
 const initial=await read()
 const operations=[{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:255}},{op:'set_color_preset',clipId:'title',tick:0,preset:'ease'}]
 const rehearsal=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(rehearsal.document)
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({keyframes:[{segment:{kind:'Spline'},outgoing:{x:0.25,y:0.1}},{incoming:{x:0.25,y:1}}]})
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('persists native color tangent modes and handles as one undoable document edit',async()=>{
 const initial=await read(),operations=[{op:'set_color_key',clipId:'title',playheadTicks:500000,value:{r:255,g:0,b:0,a:255}},{op:'set_color_tangent_mode',clipId:'title',tick:0,mode:'auto'},{op:'edit_color_tangents',clipId:'title',tick:0,side:'outgoing',point:{x:0.2,y:0.9},continuity:'broken'}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(preview.document)
 expect(await call('get_color_track',{clipId:'title'})).toMatchObject({keyframes:[{outgoing:{x:0.2,y:0.9,mode:'free'},continuity:'broken',segment:{kind:'Spline'}},{}]})
 expect(nestedHistory.past).toHaveLength(1)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
 await call('redo');expect(nestedComposition.document).toEqual(preview.document)
})

it('creates a native motion path and anchored progress keys atomically with exact Undo',async()=>{
 const initial=await read(),operations=[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'origin'},{op:'set_path_progress',clipId:'title',playheadTicks:500000,value:1}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(preview.document)
 expect(await call('get_keyframes',{clipId:'title',envelope:'path',property:'progress'})).toMatchObject({keyframes:[{tick:0,value:0},{tick:500000,value:1}]})
 expect(nestedHistory.past).toHaveLength(1)
 const current=await read()
 await call('apply_batch',{expectedProjectHash:current.projectHash,operations:[{op:'switch_position_mode',clipId:'title',positionMode:'xy',nodeId:'unused'}]})
 expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({transform:{position:{mode:'xy',x:{mode:'constant',value:0},y:{mode:'constant',value:0}}}})
 await call('undo');expect(nestedComposition.document).toEqual(preview.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('uses active native XY coordinates and preserves progress timeBase while rejecting invalid path batches',async()=>{
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual fixture required')
 Object.assign(clip,{transform:{...clip.transform,position:{mode:'xy',x:{mode:'constant',value:17},y:{mode:'constant',value:23}}}})
 const before=await read()
 await expect(call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'point'},{op:'set_path_progress',clipId:'title',playheadTicks:1000001,value:1}]})).rejects.toThrow('Invalid')
 expect(nestedComposition.document).toEqual(before.document)
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'point'}]})
 expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({transform:{position:{path:{nodes:[{point:{x:17,y:23}}]}}}})
 const pathClip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in pathClip)||pathClip.transform.position?.mode!=='path')throw new Error('Path required')
 Object.assign(pathClip.transform.position,{progress:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',keyframes:[{tick:-100,value:0},{tick:0,value:0.25}]}}})
 const keyed=await read()
 await call('apply_batch',{expectedProjectHash:keyed.projectHash,operations:[{op:'set_path_progress',clipId:'title',playheadTicks:500000,value:0.75}]})
 expect(await call('get_keyframes',{clipId:'title',envelope:'path',property:'progress'})).toMatchObject({timeBase:1000,keyframes:[{tick:-100},{tick:0},{tick:500,value:0.75}]})
})

it('uses the Rust geometry API for native path edits with atomic failure and Undo',async()=>{
 const initial=await read()
 const path={nodes:[{id:'p',point:{x:25,y:30},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line' as const,tangentMode:'corner' as const}]}
 const solve=vi.spyOn(api,'evaluateMotionPath').mockResolvedValue({path,length:0,points:[]})
 const operations=[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'p'},{op:'edit_motion_path',clipId:'title',edit:{operation:'translate',delta:{x:25,y:30}}}]
 const rehearsal=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(solve).toHaveBeenCalledWith({operation:'translate',delta:{x:25,y:30},path:{nodes:[{id:'p',point:{x:0,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]}})
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(rehearsal.document);expect(nestedHistory.past).toHaveLength(1)
 const current=await read()
 solve.mockRejectedValue(new Error('Rust path validation failed'))
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'Must not commit'}},{op:'edit_motion_path',clipId:'title',edit:{operation:'remove',index:0}}]})).rejects.toThrow('Rust path validation failed')
 expect(nestedComposition.document).toEqual(current.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('refuses a late native geometry response after another batch changed the project',async()=>{
 const initial=await read()
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'p'}]})
 let release!:(value:api.NativeMotionPathResponse)=>void
 const pending=new Promise<api.NativeMotionPathResponse>(resolve=>{release=resolve})
 const solve=vi.spyOn(api,'evaluateMotionPath').mockReturnValue(pending)
 const before=await read()
 const writing=call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'edit_motion_path',clipId:'title',edit:{operation:'solve'}}]})
 const rejected=expect(writing).rejects.toThrow('Project changed')
 await vi.waitFor(()=>expect(solve).toHaveBeenCalledTimes(1))
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'Concurrent edit'}}]})
 const current=await read()
 release({path:{nodes:[]},length:0,points:[]})
 await rejected
 expect(nestedComposition.document).toEqual(current.document)
})

it('converts native motion with rational canvas FPS, tolerance admission and exact Undo',async()=>{
 Object.assign(nestedComposition.document!.compositions.root!.content.canvas,{fpsMilli:29970,fpsRate:{numerator:30000,denominator:1001}})
 const initial=await read(),options={startFrame:0,endFrame:10,tolerancePx:0.5,everyFrames:3,xyMode:'editable'}
 const result:api.NativePositionConversionResult={nodes:[{point:{x:0,y:0},incoming:{x:0,y:0},outgoing:{x:0,y:0},cubic:false}],tracks:[{keys:[{t_us:0,value:0,in_:[2/3,2/3],out:[1/3,1/3],segment:{kind:'Linear'}}],extrapolate:{before:'Hold',after:'Hold'}}],sampleCount:11,nodeCount:1,maxErrorPx:0,checkCount:11,withinTolerance:true,limit:null}
 const convert=vi.spyOn(api,'previewPositionConversion').mockResolvedValue(result)
 const operations=[{op:'convert_motion_path',clipId:'title',nodeIdPrefix:'fit',options}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 expect(convert.mock.calls[0]![0].options).toMatchObject({startFrame:0,endFrame:10,everyFrames:1,xyMode:'editable'})
 const fps=convert.mock.calls[0]![0].options;expect(fps.fpsNum).toBe(30000);expect(fps.fpsDen).toBe(1001)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(preview.document)
 expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({transform:{position:{mode:'path',path:{nodes:[{id:'fit-0'}]}}}})
 const current=await read()
 convert.mockResolvedValue({...result,withinTolerance:false,maxErrorPx:5,limit:'node_limit'})
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,operations})).rejects.toThrow('5 px; node_limit')
 expect(nestedComposition.document).toEqual(current.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('sets native scalar extrapolation independently for each side in one atomic Undo',async()=>{
 const initial=await read(),operations=[{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:0,value:0},{op:'upsert_keyframe',clipId:'title',property:'opacity',tick:500000,value:1},{op:'set_keyframe_extrapolation',clipId:'title',property:'opacity',side:'before',extrapolation:'ping_pong'},{op:'set_keyframe_extrapolation',clipId:'title',property:'opacity',side:'after',extrapolation:'loop'}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(nestedComposition.document).toEqual(preview.document)
 expect(await call('get_keyframes',{clipId:'title',property:'opacity'})).toMatchObject({extrapolation:{before:'ping_pong',after:'loop'},keyframes:[{tick:0,value:0},{tick:500000,value:1}]})
 expect(nestedHistory.past).toHaveLength(1)
 const current=await read()
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,operations:[{op:'set_keyframe_extrapolation',clipId:'title',property:'opacity',side:'after',extrapolation:'wrong'}]})).rejects.toThrow('Invalid')
 expect(nestedComposition.document).toEqual(current.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('stores scoped native markers with one Undo and records their affected identities',async()=>{
 const initial=await read(),operations=[{op:'add_marker',marker:{id:'scene',tick:500000,label:'  Scene 🎬  ',color:'#aabbcc'}},{op:'add_marker',marker:{id:'beat',tick:100000,label:'Beat',origin:'auto_beat'}}]
 const preview=await call('dry_run',{expectedProjectHash:initial.projectHash,composition_id:'child',operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:initial.projectHash,composition_id:'child',operations})
 expect(nestedComposition.document).toEqual(preview.document)
 expect(nestedComposition.document!.compositions.child!.content.metadata?.markers).toEqual([{id:'beat',tick:100000,label:'Beat',origin:'auto_beat'},{id:'scene',tick:500000,label:'Scene 🎬',color:'#aabbcc'}])
 expect(nestedComposition.document!.compositions.root).toEqual(initial.document.compositions.root)
 expect(agentActivity.activities.findLast(activity=>activity.tool==='apply_batch')?.affected).toEqual(expect.arrayContaining([expect.objectContaining({kind:'Marker',id:'scene',compositionId:'child',label:'Scene 🎬'})]))
 const current=await read()
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,composition_id:'child',operations:[{op:'remove_marker',markerId:'beat'},{op:'add_marker',marker:{id:'bad',tick:-1,label:'Invalid'}}]})).rejects.toThrow()
 expect(nestedComposition.document).toEqual(current.document)
 await call('apply_batch',{expectedProjectHash:current.projectHash,composition_id:'child',operations:[{op:'remove_marker',markerId:'scene'}]})
 expect(nestedComposition.document!.compositions.child!.content.metadata?.markers).toHaveLength(1)
 await call('undo');expect(nestedComposition.document).toEqual(current.document)
 await call('undo');expect(nestedComposition.document).toEqual(initial.document)
})

it('rehearses native ripple deletion, commits once and restores clips through Undo',async()=>{
 const initial=await read()
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'add_layer',trackId:'titles',clip:{...clip,id:'later',timelineStartTicks:2000000}}]})
 const move=vi.spyOn(api,'moveNativeNestedClips').mockImplementation(async(project,moves)=>{
  const output=structuredClone(project)
  for(const move of moves){const clip=output.compositions.root!.content.tracks[0]!.clips.find(clip=>clip.id===move.clipId)!;Object.assign(clip,{timelineStartTick:move.startUs,timelineEndTick:move.startUs+1000000})}
  return {project:output} as Awaited<ReturnType<typeof api.moveNativeNestedClips>>
 })
 const before=await read(),depth=nestedHistory.past.length,operations=[{op:'ripple_delete_gap',trackId:'titles',startTicks:1000000,endTicks:2000000}]
 const preview=await call('dry_run',{expectedProjectHash:before.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(depth)
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations})
 expect(nestedComposition.document).toEqual(preview.document);expect(nestedHistory.past).toHaveLength(depth+1);expect(move).toHaveBeenCalled()
 expect(nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[1]).toMatchObject({id:'later',timelineStartTick:1000000,timelineEndTick:2000000})
 await call('undo');expect(nestedComposition.document).toEqual(before.document)
})

it('jumps native history by stable entry identity with hash admission and no new entries',async()=>{
 const original=await read(),documents=[]
 for(const text of ['One','Two','Three']){const current=await read();documents.push((await call('apply_batch',{expectedProjectHash:current.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text}}]}) as {document:api.NativeNestedProject}).document)}
 const history=await call('get_history') as {cursor:number;entries:{id:string}[]}
 expect(history.entries).toHaveLength(3)
 const current=await read()
 const first=await call('jump_history',{expectedProjectHash:current.projectHash,entryId:history.entries[0]!.id}) as {projectHash:string;steps:number}
 expect(first.steps).toBe(2);expect(nestedComposition.document).toEqual(documents[0]);expect(nestedHistory.past.length+nestedHistory.future.length).toBe(3)
 await expect(call('jump_history',{expectedProjectHash:current.projectHash,entryId:history.entries[2]!.id})).rejects.toThrow('Project changed')
 const latest=await call('jump_history',{expectedProjectHash:first.projectHash,entryId:history.entries[2]!.id}) as {projectHash:string}
 expect(nestedComposition.document).toEqual(documents[2])
 await call('jump_history',{expectedProjectHash:latest.projectHash,entryId:history.entries[0]!.id,side:'before'})
 expect(nestedComposition.document).toEqual(original.document)
 const base=await read();await call('apply_batch',{expectedProjectHash:base.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'Branch'}}]})
 const branch=await read();await expect(call('jump_history',{expectedProjectHash:branch.projectHash,entryId:history.entries[2]!.id})).rejects.toThrow('no longer')
 expect(nestedComposition.document).toEqual(branch.document)
})

it('can redo a native import after a history jump closes its base state',async()=>{
 const project=JSON.parse(JSON.stringify(nestedComposition.document)) as api.NativeNestedProject
 closeNestedComposition();vi.spyOn(api,'evaluateNestedFrame').mockResolvedValue({entries:[]})
 expect(await adoptImportedConcatProject(project,'Imported')).toBe(true)
 const history=await call('get_history') as {projectHash:string;entries:{id:string}[]}
 expect(await call('jump_history',{expectedProjectHash:history.projectHash,entryId:history.entries[0]!.id,side:'before'})).toMatchObject({closed:true,projectHash:null})
 expect(nestedComposition.document).toBeNull()
 await call('redo');expect(nestedComposition.document).toEqual(project)
 expect(await call('get_history')).toMatchObject({cursor:1,entries:[{id:history.entries[0]!.id}]})
})

it('maps set_path_progress into a retained rational authoring clock without touching shadow XY',async()=>{
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual clip required')
 const shadow=JSON.stringify(clip.transform.x)
 Object.assign(clip.transform,{position:{mode:'path',path:{nodes:[{id:'origin',point:{x:0,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]},progress:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:1250,value:1}]}}}})
 const before=await read()
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'set_path_progress',clipId:'title',playheadTicks:100000,value:0.75}]})
 expect(await call('get_keyframes',{clipId:'title',envelope:'path',property:'progress'})).toMatchObject({sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250},{tick:450,t_local_us:100000,value:0.75},{tick:1250}]})
 const changed=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 expect('transform' in changed&&JSON.stringify(changed.transform.x)).toBe(shadow)
 expect(nestedHistory.past).toHaveLength(1)
})

it('appends a native path node through Rust solve and rejects forged point fields atomically',async()=>{
 const geometry=vi.spyOn(api,'evaluateMotionPath').mockImplementation(async request=>{if(request.operation!=='solve')throw new Error('Solve required');return {path:request.path,length:100,points:[]}})
 const initial=await read(),operations=[{op:'switch_position_mode',clipId:'title',positionMode:'path',nodeId:'origin'},{op:'edit_motion_path',clipId:'title',edit:{operation:'append',id:'end',point:{x:100,y:0}}}]
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations})
 expect(geometry).toHaveBeenCalledWith({operation:'solve',path:{nodes:[{id:'origin',point:{x:0,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'},{id:'end',point:{x:100,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'}]}})
 const before=await read()
 await expect(call('apply_batch',{expectedProjectHash:before.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'must not commit'}},{op:'edit_motion_path',clipId:'title',edit:{operation:'append',id:'forged',point:{x:1,y:2,path:'/private'}}}]})).rejects.toThrow('Invalid appended')
 expect(nestedComposition.document).toEqual(before.document);expect(nestedHistory.past).toHaveLength(1)
})

it('rehearses and commits node arrival from Rust distances and refuses incomplete measurements',async()=>{
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual clip required')
 const node=(id:string,x:number)=>({id,point:{x,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line',tangentMode:'corner'})
 const path={nodes:[node('start',0),node('middle',40),node('end',140)]}
 Object.assign(clip.transform,{position:{mode:'path',path,progress:{mode:'keyframes',track:{timeBase:1000,interpolation:'linear',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:0},{tick:1250,value:1}]}}}})
 const geometry=vi.spyOn(api,'evaluateMotionPath').mockResolvedValue({path:path as api.NativeMotionPathResponse['path'],length:140,points:[],nodeProgresses:[0,2/7,1]})
 const before=await read(),operations=[{op:'arrive_at_path_node',clipId:'title',index:1,playheadTicks:250000}]
 const dry=await call('dry_run',{expectedProjectHash:before.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0)
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations})
 expect(nestedComposition.document).toEqual(dry.document);expect(nestedHistory.past).toHaveLength(1)
 expect(await call('get_keyframes',{clipId:'title',envelope:'path',property:'progress'})).toMatchObject({keyframes:[{tick:250},{tick:750,t_local_us:250000,value:2/7},{tick:1250}]})
 geometry.mockResolvedValue({path:path as api.NativeMotionPathResponse['path'],length:140,points:[]})
 const current=await read()
 await expect(call('apply_batch',{expectedProjectHash:current.projectHash,operations})).rejects.toThrow('omitted node progress')
 expect(nestedComposition.document).toEqual(current.document);expect(nestedHistory.past).toHaveLength(1)
})

it('inserts near a native nearest hit in rehearsal and commits one Undo, refusing misses atomically',async()=>{
 const clip=nestedComposition.document!.compositions.root!.content.tracks[0]!.clips[0]!
 if(!('transform' in clip))throw new Error('Visual clip required')
 const node=(id:string,x:number)=>({id,point:{x,y:0},inHandle:{x:0,y:0},outHandle:{x:0,y:0},segment:'line' as const,tangentMode:'corner' as const})
 const path={nodes:[node('start',0),node('end',100)]},inserted={nodes:[node('start',0),node('middle',40),node('end',100)]}
 Object.assign(clip.transform,{position:{mode:'path',path,progress:{mode:'constant',value:0.5}}})
 const geometry=vi.spyOn(api,'evaluateMotionPath').mockImplementation(async request=>request.operation==='nearest'?{path,length:100,points:[],nearest:{segment:0,t:0.4,distance:2}}:{path:inserted,length:100,points:[]})
 const before=await read(),operations=[{op:'insert_path_node_near',clipId:'title',point:{x:40,y:2},nodeId:'middle',maxDistance:3}]
 const dry=await call('dry_run',{expectedProjectHash:before.projectHash,operations}) as {document:api.NativeNestedProject}
 expect(nestedHistory.past).toHaveLength(0);expect(nestedComposition.document).toEqual(before.document)
 expect(geometry).toHaveBeenNthCalledWith(1,{operation:'nearest',path,point:{x:40,y:2}})
 expect(geometry).toHaveBeenNthCalledWith(2,{operation:'insert',path,segment:0,t:0.4,id:'middle'})
 await call('apply_batch',{expectedProjectHash:before.projectHash,operations})
 expect(nestedComposition.document).toEqual(dry.document);expect(nestedHistory.past).toHaveLength(1)
 await call('undo')
 const restored=await read()
 geometry.mockResolvedValue({path,length:100,points:[],nearest:{segment:0,t:0.4,distance:4}})
 await expect(call('apply_batch',{expectedProjectHash:restored.projectHash,operations:[{op:'update_layer',clipId:'title',patch:{text:'Must roll back'}},...operations]})).rejects.toThrow('No nearby path segment')
 expect(nestedComposition.document).toEqual(restored.document);expect(nestedHistory.past).toHaveLength(0)
})

it('reads, rehearses and atomically edits exact native effect animation addresses',async()=>{
 const {effectScalarProject}=await import('../composition/fixtures/nativeEffectScalarProject');nestedComposition.document=effectScalarProject();nestedComposition.activeCompositionId='root';nestedComposition.timeUs=1500000
 const initial=await read();expect((initial.nativeBatch as unknown as {keyframeEnvelopes:string[]}).keyframeEnvelopes).toContain('effect')
 const address={clipId:'shape',envelope:'effect',property:'stops',effectIndex:0,effectKind:'concat_exposure'}
 expect(await call('get_keyframes',address)).toMatchObject({mode:'keyframes',keyOriginUs:1000000,sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,t_us:1000000},{tick:750,t_us:1250000},{tick:1250,t_us:1500000}]})
 const args={expectedProjectHash:initial.projectHash,operations:[{op:'edit_keyframe',...address,tick:250,value:-3}]},before=JSON.stringify(nestedComposition.document)
 const dry=await call('dry_run',args) as {document:api.NativeNestedProject};expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(0)
 const accepted=await call('apply_batch',args) as {document:api.NativeNestedProject};expect(accepted.document).toEqual(dry.document);expect(nestedHistory.past).toHaveLength(1)
 await expect(call('get_keyframes',{...address,effectKind:'concat_contrast'})).rejects.toThrow('not found')
 await call('undo');expect(JSON.stringify(nestedComposition.document)).toBe(before)
})

it('reads and edits qualified native effect color-component keys independently from amount',async()=>{
 const {effectColorScalarProject}=await import('../composition/fixtures/nativeEffectScalarProject');nestedComposition.document=effectColorScalarProject();nestedComposition.activeCompositionId='root'
 const address={clipId:'shape',envelope:'effect',property:'colorAnimation.whiteR',effectIndex:0,effectKind:'color_tint'},initial=await read()
 expect(await call('get_keyframes',address)).toMatchObject({mode:'keyframes',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:.25},{tick:750,value:.5},{tick:1250,value:.75}]})
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'edit_keyframe',...address,tick:750,value:.125}]})
 expect(await call('get_keyframes',address)).toMatchObject({keyframes:[{value:.25},{value:.125},{value:.75}]});expect(await call('get_keyframes',{...address,property:'amount'})).toMatchObject({mode:'constant',value:100});expect(nestedHistory.past).toHaveLength(1)
 await call('undo');expect(await call('get_keyframes',address)).toMatchObject({keyframes:[{value:.25},{value:.5},{value:.75}]})
})

it('reads and edits nested Lumetri section keys by their qualified native address',async()=>{
 const {effectLumetriScalarProject}=await import('../composition/fixtures/nativeEffectScalarProject');nestedComposition.document=effectLumetriScalarProject();nestedComposition.activeCompositionId='root'
 const address={clipId:'shape',envelope:'effect',effectIndex:0,effectKind:'lumetri',property:'lumetri.basic.exposure'},initial=await read()
 expect(await call('get_keyframes',address)).toMatchObject({mode:'keyframes',sampleClock:{rateNum:2,rateDen:1,offsetNum:250000},keyframes:[{tick:250,value:-2},{tick:750,value:-1},{tick:1250,value:0}]})
 await call('apply_batch',{expectedProjectHash:initial.projectHash,operations:[{op:'edit_keyframe',...address,tick:750,value:-2}]});expect(await call('get_keyframes',address)).toMatchObject({keyframes:[{value:-2},{value:-2},{value:0}]});expect(await call('get_keyframes',{...address,property:'lumetri.grade.shadowsX'})).toMatchObject({mode:'constant',value:0});expect(nestedHistory.past).toHaveLength(1)
 await call('undo');expect(await call('get_keyframes',address)).toMatchObject({keyframes:[{value:-2},{value:-1},{value:0}]})
})
