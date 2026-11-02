import {beforeEach,expect,it} from 'vitest'
import {agentConnections,updateAgentConnections} from './connections.svelte'
import {agentActivity,resetAgentActivity,startAgentActivity,finishAgentActivity} from './activity.svelte'
beforeEach(()=>{agentConnections.items=[];resetAgentActivity()})
it('validates, bounds, and sanitizes connection metadata without accepting duplicate IDs',()=>{
 updateAgentConnections([{id:'a',client:'Client Bearer secret-token',version:'2',connectedAt:1,lastActivityAt:2},{id:'a',client:'duplicate',version:'',connectedAt:1,lastActivityAt:2},{id:'bad',client:'bad',version:'2',connectedAt:'yesterday',lastActivityAt:2},null])
 expect(agentConnections.items).toHaveLength(1)
 expect(agentConnections.items[0]!.client).toContain('[redacted]')
 expect(agentConnections.items[0]!.client).not.toContain('secret-token')
 updateAgentConnections({invalid:true});expect(agentConnections.items).toHaveLength(1)
 updateAgentConnections(Array.from({length:200},(_,i)=>({id:String(i),client:'x'.repeat(1000),version:'v'.repeat(1000),connectedAt:1,lastActivityAt:2})))
 expect(agentConnections.items).toHaveLength(128);expect(agentConnections.items[0]!.client).toHaveLength(128);expect(agentConnections.items[0]!.version).toHaveLength(64)
})
it('captures client name/version on entry and retains them after disconnect or newer metadata',()=>{
 updateAgentConnections([{id:'a',client:'Named client',version:'3.1',connectedAt:1,lastActivityAt:2}])
 const call=startAgentActivity('a','get_project',{},null)
 updateAgentConnections([])
 finishAgentActivity(call)
 expect(agentActivity.activities[0]).toMatchObject({client:'Named client',clientVersion:'3.1',actor:'a',state:'done'})
 const unknown=startAgentActivity('b','get_project',{},null);finishAgentActivity(unknown)
 expect(agentActivity.activities[1]!.client).toBeUndefined()
})
