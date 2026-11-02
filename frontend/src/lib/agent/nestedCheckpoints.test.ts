import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {executeAgentRequest,executeAgentTool} from './runtime.svelte'
import {agentSession} from './session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,resetNestedHistory} from '../state/nestedComposition.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {importSrtToComposition} from '../state/composition/timeline.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
const tool=(name:string,args:Record<string,unknown>={})=>executeAgentRequest('checkpoint-agent',name,args)
const read=async()=>await tool('get_project') as {projectHash:string;document:api.NativeNestedProject}
beforeEach(()=>{
 closeNestedComposition();resetNestedHistory();setCompositionStorageForTests(null);resetCompositionForTests();agentSession.checkpoints=[]
 importSrtToComposition('1\n00:00:01,000 --> 00:00:02,000\nOriginal')
 executeAgentTool('ordinary-agent','create_checkpoint',{label:'Ordinary'})
 const content=JSON.parse(JSON.stringify(buildCompositionWireDocument(compositionState.document)))
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
 nestedComposition.activeCompositionId='root';nestedComposition.renderedOutputs={root:'old'}
 vi.spyOn(api,'nestedDurations').mockResolvedValue({root:2_000_000})
})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition();agentSession.lock=null})
it('lists only current-context metadata, with actor and chronological timestamps, without recording edits',async()=>{
 const before=JSON.stringify(nestedComposition.document),ordinary=JSON.stringify(compositionState.document)
 const first=await tool('create_checkpoint',{label:'First'}) as {id:string;createdAt:number}
 const second=await tool('create_checkpoint',{label:'Second'}) as {id:string;createdAt:number}
 expect(await tool('list_checkpoints')).toEqual([{id:first.id,label:'First',actor:'checkpoint-agent',created_at:new Date(first.createdAt).toISOString()},{id:second.id,label:'Second',actor:'checkpoint-agent',created_at:new Date(second.createdAt).toISOString()}])
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(JSON.stringify(compositionState.document)).toBe(ordinary)
 expect(nestedHistory.past).toHaveLength(0)
 expect((await tool('get_history') as {checkpoints:unknown[]}).checkpoints).toHaveLength(2)
})
it('restores an immutable checkpoint as one native history step, undoable without changing ordinary history',async()=>{
 const before=await read(),ordinary=JSON.stringify(compositionState.document)
 const checkpoint=await tool('create_checkpoint',{label:'Before'}) as {id:string}
 await tool('restyle_captions',{expectedProjectHash:before.projectHash,font_size_px:66})
 const changed=await read()
 await expect(tool('restore_checkpoint',{id:checkpoint.id,expectedProjectHash:before.projectHash})).rejects.toThrow('Project changed')
 await tool('restore_checkpoint',{id:checkpoint.id,expectedProjectHash:changed.projectHash})
 expect(nestedComposition.document).toEqual(before.document)
 expect(nestedHistory.past).toHaveLength(2)
 await tool('undo');expect(nestedComposition.document).toEqual(changed.document)
 await tool('redo');expect(nestedComposition.document).toEqual(before.document)
 expect(JSON.stringify(compositionState.document)).toBe(ordinary)
 expect(await tool('list_checkpoints')).toHaveLength(1)
})
it('bounds storage at twenty, deletes by id and refuses snapshots from another opened context',async()=>{
 const snapshot=await read()
 const entries:{id:string}[]=[]
 for(let i=0;i<20;i++)entries.push(await tool('create_checkpoint',{label:`Point ${i}`}) as {id:string})
 await expect(tool('create_checkpoint',{label:'Overflow'})).rejects.toThrow('limit')
 await tool('delete_checkpoint',{id:entries[0]!.id})
 expect(await tool('list_checkpoints')).toHaveLength(19)
 await expect(tool('restore_checkpoint',{id:entries[0]!.id,expectedProjectHash:snapshot.projectHash})).rejects.toThrow('not found')
 closeNestedComposition()
 expect(await tool('list_checkpoints')).toMatchObject([{label:'Ordinary',actor:'ordinary-agent'}])
 nestedComposition.document=JSON.parse(JSON.stringify(snapshot.document))
 expect(await tool('list_checkpoints')).toEqual([])
 await expect(tool('restore_checkpoint',{id:entries[1]!.id,expectedProjectHash:(await read()).projectHash})).rejects.toThrow('not found')
})

it('keeps checkpoint snapshots independent from both the supplied document and a returned copy',async()=>{
 const {createNestedCheckpoint,readNestedCheckpoint}=await import('./nestedCheckpoints.svelte')
 const snapshot=await read()
 const checkpoint=createNestedCheckpoint('Immutable','unit',snapshot.document)
 Object.assign(snapshot.document.compositions.root!.content.tracks[0]!.clips[0]!,{text:'Input tampered'})
 const restored=readNestedCheckpoint(checkpoint.id)
 expect(restored.compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'Original'})
 Object.assign(restored.compositions.root!.content.tracks[0]!.clips[0]!,{text:'Returned copy tampered'})
 expect(readNestedCheckpoint(checkpoint.id).compositions.root!.content.tracks[0]!.clips[0]).toMatchObject({text:'Original'})
})

it('repairs a removed active composition and restores the previous view with Undo',async()=>{
 const original=await read()
 const checkpoint=await tool('create_checkpoint',{label:'Before new composition'}) as {id:string}
 nestedComposition.document={...original.document,compositions:{...original.document.compositions,later:{content:original.document.compositions.root!.content,instances:[]}}}
 nestedComposition.activeCompositionId='later';nestedComposition.timeUs=1_500_000
 await tool('restore_checkpoint',{id:checkpoint.id,expectedProjectHash:(await read()).projectHash})
 expect(nestedComposition.activeCompositionId).toBe('root');expect(nestedComposition.timeUs).toBe(0)
 await tool('undo')
 expect(nestedComposition.activeCompositionId).toBe('later');expect(nestedComposition.timeUs).toBe(1_500_000)
 await tool('redo')
 expect(nestedComposition.activeCompositionId).toBe('root');expect(nestedComposition.timeUs).toBe(0)
})
it('clamps a retained composition cursor to the restored native duration',async()=>{
 const original=await read()
 const checkpoint=await tool('create_checkpoint',{label:'Short'}) as {id:string}
 nestedComposition.document={...original.document,compositions:{root:{...original.document.compositions.root!,durationUs:6_000_000}}}
 nestedComposition.timeUs=5_000_000
 await tool('restore_checkpoint',{id:checkpoint.id,expectedProjectHash:(await read()).projectHash})
 expect(nestedComposition.timeUs).toBe(1_999_999)
 await tool('undo');expect(nestedComposition.timeUs).toBe(5_000_000)
})
