import {expect,it} from 'vitest'
import {activitySections} from './activityRows'
import type {AgentActivity} from './activity.svelte'
const item=(id:string,sessionId:string|null='a',actor='actor-a',patch:Partial<AgentActivity>={}):AgentActivity=>({id,sessionId,actor,tool:'get_project',kind:'read',startedAt:0,endedAt:10,durationMs:10,state:'done',args:'{}',error:null,historyIds:[],effect:null,...patch})
it('folds only adjacent successful fast reads of the same actor',()=>{
 const sections=activitySections([item('1'),item('2'),item('3','a','actor-b'),item('4','a','actor-b',{state:'error'}),item('5','a','actor-b',{durationMs:250}),item('6','a','actor-b',{kind:'operation'})])
 expect(sections).toHaveLength(1)
 expect(sections[0]!.rows.map(row=>row.kind)).toEqual(['reads','activity','activity','activity','activity'])
})
it('preserves interleaving instead of regrouping disjoint runs of one work session',()=>{
 const sections=activitySections([item('1','a'),item('2',null,'outside'),item('3','a')])
 expect(sections.map(section=>section.sessionId)).toEqual(['a',null,'a'])
 expect(sections.map(section=>section.id)).toEqual(['1','2','3'])
})
