import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {restoreActivityCheckpoint,activityCheckpointAction} from './activityCheckpoint.svelte'
import {executeAgentTool,executeAgentRequest} from './runtime.svelte'
import {agentActivity,resetAgentActivity} from './activity.svelte'
import {agentSession} from './session.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {undoComposition} from '../state/composition/transport.svelte'
import {closeNestedComposition,nestedComposition,nestedHistory} from '../state/nestedComposition.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
beforeEach(()=>{closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests();resetAgentActivity();agentSession.checkpoints=[];activityCheckpointAction.requestId=null;activityCheckpointAction.workspaceId=null})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition()})
const ordinaryPoint=()=>executeAgentTool('owner','create_checkpoint',{label:'Before edit'}) as {id:string}
it('links the exact point and restores ordinary content once through the existing Undo path',async()=>{
 const point=ordinaryPoint(),row=agentActivity.activities.at(-1)!,before=JSON.stringify(compositionState.document)
 expect(row).toMatchObject({checkpointId:point.id,checkpointLabel:'Before edit',kind:'checkpoint'})
 compositionState.document={...compositionState.document,canvas:{...compositionState.document.canvas,width:640}}
 const edited=JSON.stringify(compositionState.document),depth=compositionState.history.past.length
 await restoreActivityCheckpoint(point.id,agentActivity.workspaceId)
 expect(JSON.stringify(compositionState.document)).toBe(before);expect(compositionState.history.past).toHaveLength(depth+1)
 expect(agentActivity.activities.at(-1)).toMatchObject({checkpointId:point.id,checkpointLabel:'Before edit',kind:'restore',state:'done',effect:'applied'})
 undoComposition();expect(JSON.stringify(compositionState.document)).toBe(edited)
 executeAgentTool('owner','delete_checkpoint',{id:point.id})
 expect(agentActivity.activities.find(item=>item.id===row.id)?.checkpointLabel).toBe('Before edit')
 await expect(restoreActivityCheckpoint(point.id,agentActivity.workspaceId)).rejects.toThrow('недоступна')
})
it('refuses active locks and stale openings while preserving documents',async()=>{
 const point=ordinaryPoint(),workspace=agentActivity.workspaceId,before=JSON.stringify(compositionState.document)
 agentSession.lock={owner:'owner',reason:'Hold',expiresAt:Date.now()+60000}
 await expect(restoreActivityCheckpoint(point.id,workspace)).rejects.toThrow('заблокирована')
 expect(JSON.stringify(compositionState.document)).toBe(before)
 agentSession.lock=null;resetAgentActivity()
 await expect(restoreActivityCheckpoint(point.id,workspace)).rejects.toThrow('закрыт')
})
it('associates the first begin-session point but does not create another association on a repeat',()=>{
 const first=executeAgentTool('owner','begin_agent_session',{reason:'Repair'}) as {checkpoint_id:string}
 expect(agentActivity.activities.at(-1)).toMatchObject({checkpointId:first.checkpoint_id,checkpointLabel:'Pre-agent: Repair'})
 executeAgentTool('owner','begin_agent_session',{reason:'Repeat'})
 expect(agentActivity.activities.at(-1)?.checkpointId).toBeUndefined();expect(agentSession.checkpoints).toHaveLength(1)
})
it('restores native checkpoints with a current hash and records the real native entry',async()=>{
 const content=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
 const before=JSON.stringify(nestedComposition.document)
 const point=await executeAgentRequest('owner','create_checkpoint',{label:'Native original'}) as {id:string}
 expect(agentActivity.activities.at(-1)).toMatchObject({checkpointId:point.id,checkpointLabel:'Native original'})
 nestedComposition.document={...nestedComposition.document,compositions:{root:{content:{...content,canvas:{...content.canvas,width:640}},instances:[]}}}
 vi.spyOn(api,'nestedDurations').mockResolvedValue({root:0})
 await restoreActivityCheckpoint(point.id,agentActivity.workspaceId)
 expect(JSON.stringify(nestedComposition.document)).toBe(before);expect(nestedHistory.past).toHaveLength(1)
 expect(agentActivity.activities.at(-1)).toMatchObject({checkpointId:point.id,checkpointLabel:'Native original',kind:'restore',effect:'applied'})
})
it('does not duplicate an in-flight restore or let old-opening completion clear a new restore',async()=>{
 const content=buildCompositionWireDocument(compositionState.document)
 const project=()=>({schemaVersion:1 as const,root:'root',compositions:{root:{content,instances:[]}}})
 nestedComposition.document=project()
 const point=await executeAgentRequest('owner','create_checkpoint',{label:'Old point'}) as {id:string}
 nestedComposition.document={...project(),compositions:{root:{content:{...content,canvas:{...content.canvas,width:640}},instances:[]}}}
 let releaseOld!:(value:Record<string,number>)=>void,releaseNew!:(value:Record<string,number>)=>void
 const validate=vi.spyOn(api,'nestedDurations').mockImplementationOnce(()=>new Promise(resolve=>{releaseOld=resolve})).mockImplementationOnce(()=>new Promise(resolve=>{releaseNew=resolve}))
 const old=restoreActivityCheckpoint(point.id,agentActivity.workspaceId)
 await vi.waitFor(()=>expect(validate).toHaveBeenCalledTimes(1))
 await expect(restoreActivityCheckpoint(point.id,agentActivity.workspaceId)).rejects.toThrow(/занят|выполняется/)
 closeNestedComposition();nestedComposition.document=project()
 const next=await executeAgentRequest('owner','create_checkpoint',{label:'New point'}) as {id:string}
 nestedComposition.document={...project(),compositions:{root:{content:{...content,canvas:{...content.canvas,width:content.canvas.width+64}},instances:[]}}}
 const pending=restoreActivityCheckpoint(next.id,agentActivity.workspaceId)
 await vi.waitFor(()=>expect(validate).toHaveBeenCalledTimes(2))
 const requestId=activityCheckpointAction.requestId
 const rejected=expect(old).rejects.toThrow();releaseOld({root:0});await rejected
 expect(activityCheckpointAction.requestId).toBe(requestId);expect(nestedHistory.busy).toBe(true)
 releaseNew({root:0});await pending
 expect(activityCheckpointAction.requestId).toBeNull();expect(nestedHistory.past).toHaveLength(1)
 expect(nestedComposition.document?.compositions.root?.content.canvas.width).toBe(content.canvas.width)
})

it('resetting activity display does not cancel an already admitted restore in the same actual project',async()=>{
 const content=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content,instances:[]}}}
 const point=await executeAgentRequest('owner','create_checkpoint',{label:'Keep operation'}) as {id:string}
 nestedComposition.document={...nestedComposition.document,compositions:{root:{content:{...content,canvas:{...content.canvas,width:content.canvas.width+64}},instances:[]}}}
 let release!:(value:Record<string,number>)=>void
 const validate=vi.spyOn(api,'nestedDurations').mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const pending=restoreActivityCheckpoint(point.id,agentActivity.workspaceId)
 await vi.waitFor(()=>expect(validate).toHaveBeenCalledTimes(1))
 resetAgentActivity();release({root:0});await pending
 expect(nestedComposition.document?.compositions.root?.content.canvas.width).toBe(content.canvas.width)
 expect(nestedHistory.past).toHaveLength(1);expect(agentActivity.activities).toEqual([])
})
