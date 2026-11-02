import {beforeEach,afterEach,expect,it,vi} from 'vitest'
import * as api from '../api'
import {agentActivity,resetAgentActivity} from './activity.svelte'
import {executeAgentTool,executeAgentRequest} from './runtime.svelte'
import {agentSession,releaseAgentConnection} from './session.svelte'
import {nestedComposition,nestedHistory,closeNestedComposition,undoNestedComposition,redoNestedComposition} from '../state/nestedComposition.svelte'
import {compositionState,resetCompositionForTests,setCompositionStorageForTests} from '../state/composition/core.svelte'
import {buildCompositionWireDocument} from '../composition/payload'
beforeEach(()=>{closeNestedComposition();setCompositionStorageForTests(null);resetCompositionForTests();agentSession.checkpoints=[];resetAgentActivity()})
afterEach(()=>{vi.restoreAllMocks();closeNestedComposition()})
it('manual view switching is independent; first begin checkpoints once and a repeat does not change the view',()=>{
 agentSession.view='agent';expect(agentSession.work).toBeNull();expect(agentSession.checkpoints).toHaveLength(0)
 agentSession.view='editor'
 const before=JSON.stringify(compositionState.document),depth=compositionState.history.past.length
 const first=executeAgentTool('owner-a','begin_agent_session',{reason:'Fix title'}) as {session_id:string;checkpoint_id:string}
 expect(agentSession.view).toBe('agent');expect(agentSession.checkpoints).toHaveLength(1)
 expect(agentSession.checkpoints[0]).toMatchObject({id:first.checkpoint_id,label:'Pre-agent: Fix title',actor:'owner-a'})
 agentSession.view='editor'
 expect(executeAgentTool('owner-a','begin_agent_session',{reason:'Ignored repeat'})).toEqual(first)
 expect(agentSession.view).toBe('editor');expect(agentSession.checkpoints).toHaveLength(1)
 expect(()=>executeAgentTool('owner-b','begin_agent_session',{reason:'Other'})).toThrow('AgentSessionBusy')
 expect(JSON.stringify(compositionState.document)).toBe(before);expect(compositionState.history.past).toHaveLength(depth)
})
it('ending keeps view/checkpoints/logs and releases only the work owner lock; force is explicit',()=>{
 executeAgentTool('owner-a','begin_agent_session',{reason:'Work'})
 agentSession.lock={owner:'owner-a',reason:'Owned',expiresAt:Date.now()+60_000}
 expect(()=>executeAgentTool('owner-b','end_agent_session',{})).toThrow('AgentSessionBusy')
 executeAgentTool('owner-b','end_agent_session',{force:true})
 expect(agentSession.work).toBeNull();expect(agentSession.lock).toBeNull();expect(agentSession.view).toBe('agent')
 expect(agentSession.checkpoints).toHaveLength(1);expect(agentSession.log.length).toBeGreaterThan(0)
 executeAgentTool('owner-a','begin_agent_session',{reason:'Second'})
 agentSession.lock={owner:'owner-b',reason:'Other owner',expiresAt:Date.now()+60_000}
 executeAgentTool('owner-a','end_agent_session',{})
 expect(agentSession.lock?.owner).toBe('owner-b')
 expect(executeAgentTool('owner-a','get_project',{})).toMatchObject({workSession:null,agentView:'agent'})
})
it('definite connection release ends only matching work and no failed begin leaves a partial session',()=>{
 executeAgentTool('owner-a','begin_agent_session',{reason:'Work'})
 releaseAgentConnection('owner-b');expect(agentSession.work?.owner).toBe('owner-a')
 releaseAgentConnection('owner-a');expect(agentSession.work).toBeNull();expect(agentSession.view).toBe('agent')
 agentSession.view='editor'
 for(let i=1;i<20;i++)executeAgentTool('owner-a','create_checkpoint',{label:`Point ${i}`})
 expect(()=>executeAgentTool('owner-a','begin_agent_session',{reason:'No slot'})).toThrow('limit')
 expect(agentSession.work).toBeNull();expect(agentSession.view).toBe('editor')
 expect(()=>executeAgentTool('owner-a','begin_agent_session',{reason:'  '})).toThrow()
})
it('native begin checkpoints the visible native project and project reset ends work',async()=>{
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(compositionState.document),instances:[]}}}
 const before=JSON.stringify(nestedComposition.document)
 const first=await executeAgentRequest('native-owner','begin_agent_session',{reason:'Native work'}) as {checkpoint_id:string}
 expect(await executeAgentRequest('native-owner','list_checkpoints',{})).toMatchObject([{id:first.checkpoint_id,actor:'native-owner'}])
 expect(agentSession.checkpoints).toHaveLength(0);expect(nestedHistory.past).toHaveLength(0)
 expect(JSON.stringify(nestedComposition.document)).toBe(before)
 closeNestedComposition();expect(agentSession.work).toBeNull();expect(agentSession.view).toBe('editor')
})
it('ending a session does not cancel an already running native operation',async()=>{
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:buildCompositionWireDocument(compositionState.document),instances:[]}}}
 const work=await executeAgentRequest('owner','begin_agent_session',{reason:'Observe'}) as {session_id:string}
 const snapshot=await executeAgentRequest('owner','get_project',{}) as {projectHash:string}
 let release!:(value:Record<string,number>)=>void
 const validate=vi.spyOn(api,'nestedDurations').mockImplementation(()=>new Promise(resolve=>{release=resolve}))
 const pending=executeAgentRequest('owner','restyle_captions',{expectedProjectHash:snapshot.projectHash,font_size_px:48})
 await vi.waitFor(()=>expect(validate).toHaveBeenCalled())
 expect(nestedHistory.busy).toBe(true)
 const activity=agentActivity.activities.find(item=>item.tool==='restyle_captions')!
 expect(activity).toMatchObject({state:'running',sessionId:work.session_id})
 await executeAgentRequest('owner','end_agent_session',{})
 expect(agentSession.work).toBeNull();expect(nestedHistory.busy).toBe(true)
 release({root:0})
 expect(await pending).toMatchObject({kind:'nested',restyled:0})
 expect(nestedHistory.busy).toBe(false)
 expect(agentActivity.activities.filter(item=>item.tool==='restyle_captions')).toHaveLength(1)
 expect(agentActivity.activities.find(item=>item.id===activity.id)).toMatchObject({state:'done',sessionId:work.session_id})
})

it('records exactly one activity per async dispatch and records synchronous failures independently of diagnostics',async()=>{
 await executeAgentRequest('owner','get_project',{})
 expect(agentActivity.activities).toHaveLength(1)
 expect(agentActivity.activities[0]).toMatchObject({tool:'get_project',state:'done'})
 expect(()=>executeAgentTool('owner','no_such_tool',{})).toThrow()
 expect(agentActivity.activities).toHaveLength(2)
 expect(agentActivity.activities[1]).toMatchObject({tool:'no_such_tool',state:'error'})
 agentSession.log=[]
 expect(agentActivity.activities).toHaveLength(2)
})

it('links native checkpoint restore to its real history entry and retains identity through Undo/Redo',async()=>{
 const wire=buildCompositionWireDocument(compositionState.document)
 nestedComposition.document={schemaVersion:1,root:'root',compositions:{root:{content:wire,instances:[]}}}
 const point=await executeAgentRequest('owner','create_checkpoint',{label:'Native original'}) as {id:string}
 // A different real document makes restore a mutation rather than a no-op.
 nestedComposition.document={...nestedComposition.document,compositions:{root:{content:{...wire,canvas:{...wire.canvas,width:640}},instances:[]}}}
 vi.spyOn(api,'nestedDurations').mockResolvedValue({root:0})
 const snapshot=await executeAgentRequest('owner','get_project',{}) as {projectHash:string}
 await executeAgentRequest('owner','restore_checkpoint',{id:point.id,expectedProjectHash:snapshot.projectHash})
 const restore=agentActivity.activities.at(-1)!
 expect(restore.historyIds).toHaveLength(1);expect(restore.effect).toBe('applied')
 expect(restore.affected).toEqual(expect.arrayContaining([expect.objectContaining({kind:'Composition',id:'root'})]))
 undoNestedComposition()
 expect(agentActivity.activities.find(item=>item.id===restore.id)?.effect).toBe('reverted')
 redoNestedComposition()
 expect(agentActivity.activities.find(item=>item.id===restore.id)?.effect).toBe('applied')
})
