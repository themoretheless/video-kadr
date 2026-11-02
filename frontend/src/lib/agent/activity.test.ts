import {beforeEach,expect,it} from 'vitest'
import {agentActivity,resetAgentActivity,startAgentActivity,finishAgentActivity,recordActivitySession,endActivitySession,latestCompletedActivityIds} from './activity.svelte'
import {activityDetails} from './activityDetails'
beforeEach(resetAgentActivity)
it('keeps overlapping call identity/session attribution after a session ends and another begins',()=>{
 const a={id:'session-a',owner:'actor-a',reason:'First',startedAt:Date.now()}
 recordActivitySession(a)
 const old=startAgentActivity('actor-a','restyle_captions',{font_size_px:77},a)
 endActivitySession(a.id,'forced')
 const b={id:'session-b',owner:'actor-b',reason:'Second',startedAt:Date.now()}
 recordActivitySession(b)
 const newer=startAgentActivity('actor-b','get_project',{},b)
 finishAgentActivity(newer)
 finishAgentActivity(old)
 expect(agentActivity.activities.find(item=>item.id===old.id)).toMatchObject({actor:'actor-a',sessionId:'session-a',state:'done',effect:null,historyIds:[]})
 expect(agentActivity.activities.find(item=>item.id===newer.id)).toMatchObject({actor:'actor-b',sessionId:'session-b',kind:'read'})
 expect(agentActivity.sessions.find(item=>item.id==='session-a')?.endReason).toBe('forced')
})
it('retains every running call and the latest thousand completions, including an old running call finishing last',()=>{
 const running=startAgentActivity('slow','analyze_clip',{},null)
 for(let i=0;i<1005;i++){const call=startAgentActivity('reader','get_project',{},null);finishAgentActivity(call)}
 expect(agentActivity.activities).toHaveLength(1001);expect(agentActivity.evicted).toBe(5)
 expect(agentActivity.activities.some(item=>item.id===running.id&&item.state==='running')).toBe(true)
 finishAgentActivity(running)
 expect(agentActivity.activities).toHaveLength(1000);expect(agentActivity.evicted).toBe(6)
 expect(agentActivity.activities.find(item=>item.id===running.id)?.state).toBe('done')
 expect(latestCompletedActivityIds(agentActivity.activities,1)).toEqual([running.id])
})
it('discards late results from a previous opening and never inserts them into the new activity panel',()=>{
 const old=startAgentActivity('a','measure_shots',{},null),workspace=agentActivity.workspaceId
 resetAgentActivity()
 expect(agentActivity.workspaceId).not.toBe(workspace)
 expect(finishAgentActivity(old,new Error('Late failure'))).toBe(false)
 expect(agentActivity.activities).toEqual([]);expect(agentActivity.revision).toBe(0)
})
it('records failure and duration without inventing a history association',()=>{
 const token=startAgentActivity('a','apply_batch',{},null)
 finishAgentActivity(token,new Error('Authorization: Bearer hidden-token'))
 expect(agentActivity.activities[0]).toMatchObject({state:'error',historyIds:[],effect:null})
 expect(agentActivity.activities[0]!.durationMs).toBeGreaterThanOrEqual(0)
 expect(agentActivity.activities[0]!.error).not.toContain('hidden-token')
 expect(finishAgentActivity(token)).toBe(false)
 const weird=startAgentActivity('a','unknown',{},null)
 finishAgentActivity(weird,undefined,undefined,true)
 expect(agentActivity.activities.at(-1)!.state).toBe('error')
})
it('bounds/redacts argument detail, omits binary and cycles, and tolerates unreadable values',()=>{
 const cyclic:Record<string,unknown>={token:'secret-token',nested:{password:'secret-password'},note:'Bearer abc+/def== api_key=secret-key token=secret-url'};cyclic.self=cyclic;cyclic.binary=new Uint8Array(100_000)
 const details=activityDetails(cyclic)
 for(const secret of ['secret-token','secret-password','abc+/def==','secret-key','secret-url'])expect(details).not.toContain(secret)
 expect(details).toContain('[circular]');expect(details).toContain('[binary omitted]')
 expect(new TextEncoder().encode(activityDetails(Array.from({length:1000},()=>({long:'🙂'.repeat(1000)})))).length).toBeLessThanOrEqual(4096)
 expect(activityDetails({get broken(){throw new Error('Cannot read')}})).toBe('"[unavailable]"')
})
