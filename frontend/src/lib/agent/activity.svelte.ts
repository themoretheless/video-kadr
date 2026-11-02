import {activityReveal} from './activityReveal.svelte'
import {agentConnections} from './connections.svelte'
import {SvelteMap} from 'svelte/reactivity'
import type {AffectedEntity} from './affectedEntities'
import {historyEffect} from './historyProvenance'
import {activityDetails,activityError} from './activityDetails'
export interface ActivitySession {id:string;owner:string;reason:string;startedAt:number;endedAt:number|null;endReason:string|null}
export interface AgentActivity {
 checkpointId?:string;checkpointLabel?:string;client?:string;clientVersion?:string;id:string;actor:string;tool:string;kind:'read'|'operation'|'checkpoint'|'restore';sessionId:string|null
 startedAt:number;endedAt:number|null;durationMs:number|null;state:'running'|'done'|'error'
 affected?:readonly AffectedEntity[];affectedOmitted?:number;args:string;error:string|null;historyIds:readonly string[];effect:null|'applied'|'reverted'|'partial'|'unknown'
}
function withEffect(item:AgentActivity):AgentActivity {return item.historyIds.length?{...item,get effect(){return historyEffect(item.historyIds)}}:item}
export interface ActivityToken {workspaceId:string;id:string}
/** Records are replaced immutably; deep proxies add no reactivity and make retention quadratic in proxy work. */
class ActivityState {
 workspaceId=$state(crypto.randomUUID())
 revision=$state(0)
 activities=$state.raw<AgentActivity[]>([])
 sessions=$state.raw<ActivitySession[]>([])
 evicted=$state(0)
}
export const agentActivity=new ActivityState()
let completedOrder:string[]=[]
const reads=new Set(['command_list','get_project','get_history','list_checkpoints','get_keyframes','get_color_track','analyze_clip','measure_shots','compare_frames','dry_run'])
function prune(){
 const removed=completedOrder.splice(0,Math.max(0,completedOrder.length-1000))
 if(removed.length){
  const ids=new Set(removed)
  agentActivity.activities=agentActivity.activities.filter(item=>!ids.has(item.id));agentActivity.evicted+=removed.length
 }
 const retained=new Set(agentActivity.activities.flatMap(item=>item.sessionId?[item.sessionId]:[]))
 agentActivity.sessions=agentActivity.sessions.filter(item=>item.endedAt===null||retained.has(item.id))
}
export function resetAgentActivity(){
 activityReveal.request=null
 completedOrder=[]
 agentActivity.workspaceId=crypto.randomUUID();agentActivity.revision=0;agentActivity.activities=[];agentActivity.sessions=[];agentActivity.evicted=0
}
export function startAgentActivity(actor:string,tool:string,args:unknown,work:{id:string;owner:string}|null):ActivityToken {
 const id=crypto.randomUUID()
 actor=String(actor).slice(0,128);tool=String(tool).slice(0,128)
 const client=agentConnections.items.find(item=>item.id===actor)
 const commandRead=tool==='command_run'&&args!==null&&typeof args==='object'&&'id' in args&&(args.id==='project.inspect'||args.id==='sequence.inspect'||args.id==='source.inspect'||args.id==='effects.list'||args.id==='masks.list'||args.id==='jobs.list'||args.id==='edit.selectLabelGroup'||args.id==='edit.selectAllMatching'||args.id==='project.selectAll'||args.id==='project.deselectAll'||args.id==='project.select'||args.id==='clip.subclipDefaults'||args.id==='sequence.matchFrame'||args.id==='clip.revealInProject'||args.id==='sequence.revealInProject'||args.id==='source.open'||args.id==='source.setPlayhead'||args.id==='markers.goToIn'||args.id==='markers.goToOut'||args.id==='markers.goToSplitVideoIn'||args.id==='markers.goToSplitVideoOut'||args.id==='markers.goToSplitAudioIn'||args.id==='markers.goToSplitAudioOut')
 const kind=reads.has(tool)||commandRead?'read':tool==='restore_checkpoint'?'restore':tool.includes('checkpoint')||tool==='begin_agent_session'?'checkpoint':'operation'
 agentActivity.activities=[...agentActivity.activities,withEffect({id,actor,client:client?.client,clientVersion:client?.version,tool,kind,sessionId:work?.owner===actor?work.id:null,startedAt:Date.now(),endedAt:null,durationMs:null,state:'running',args:activityDetails(args),error:null,historyIds:[],effect:null})]
 agentActivity.revision++
 return {workspaceId:agentActivity.workspaceId,id}
}
export function finishAgentActivity(token:ActivityToken,error?:unknown,result?:unknown,failed=error!==undefined):boolean {
 if(token.workspaceId!==agentActivity.workspaceId)return false
 const current=agentActivity.activities.find(item=>item.id===token.id)
 if(!current||current.state!=='running')return false
 let sessionId=current.sessionId
 if(result&&typeof result==='object'){
  const record=result as Record<string,unknown>
  const linked=current.tool==='begin_agent_session'?record.session_id:current.tool==='end_agent_session'?record.ended:undefined
  if(typeof linked==='string'&&agentActivity.sessions.some(item=>item.id===linked))sessionId=linked
 }
 const endedAt=Date.now()
 agentActivity.activities=agentActivity.activities.map(item=>item.id!==token.id?item:withEffect({...item,sessionId,endedAt,durationMs:Math.max(0,endedAt-item.startedAt),state:failed?'error':'done',error:failed?activityError(error):null}))
 completedOrder.push(token.id)
 agentActivity.revision++;prune();return true
}
export function recordActivitySession(work:{id:string;owner:string;reason:string;startedAt:number}){
 agentActivity.activities=agentActivity.activities.map(item=>item.state==='running'&&item.tool==='begin_agent_session'&&item.actor===work.owner&&item.sessionId===null?withEffect({...item,sessionId:work.id}):item)
 if(!agentActivity.sessions.some(item=>item.id===work.id)){agentActivity.sessions=[...agentActivity.sessions,{...work,endedAt:null,endReason:null}];agentActivity.revision++}
}
export function endActivitySession(id:string,reason:string){
 agentActivity.sessions=agentActivity.sessions.map(item=>item.id!==id?item:{...item,endedAt:Date.now(),endReason:reason})
 agentActivity.revision++;prune()
}

export function latestCompletedActivityIds(items:readonly AgentActivity[],limit:number):string[]{
 const available=items.map(item=>item.id)
 return completedOrder.filter(id=>available.includes(id)).slice(-limit)
}

/** Called only after this exact request commits an actual history entry. */
export function linkActivityHistory(token:ActivityToken|undefined,id:string){
 if(!token||token.workspaceId!==agentActivity.workspaceId)return
 const item=agentActivity.activities.find(item=>item.id===token.id)
 if(!item||item.historyIds.includes(id))return
 agentActivity.activities=agentActivity.activities.map(item=>item.id===token.id?withEffect({...item,historyIds:[...item.historyIds,id]}):item)
 agentActivity.revision++
}

export function linkActivityEntities(token:ActivityToken|undefined,entities:readonly AffectedEntity[]){
 if(!token||token.workspaceId!==agentActivity.workspaceId)return
 const current=agentActivity.activities.find(item=>item.id===token.id)
 if(!current)return
 const combined=new SvelteMap((current.affected??[]).map(entity=>[JSON.stringify([entity.compositionId,entity.kind,entity.id]),entity]))
 for(const entity of entities)combined.set(JSON.stringify([entity.compositionId,entity.kind,entity.id]),entity)
 const all=[...combined.values()]
 agentActivity.activities=agentActivity.activities.map(item=>item.id===token.id?withEffect({...item,affected:all.slice(0,512),affectedOmitted:(item.affectedOmitted??0)+Math.max(0,all.length-512)}):item)
 agentActivity.revision++
}

/** Exact checkpoint metadata from the operation that created/restored it; never infer IDs from labels. */
export function linkActivityCheckpoint(token:ActivityToken|undefined,id:string,label:string,kind:'checkpoint'|'restore'){
 if(!token||token.workspaceId!==agentActivity.workspaceId)return
 agentActivity.activities=agentActivity.activities.map(item=>item.id===token.id?withEffect({...item,kind,checkpointId:id,checkpointLabel:activityError(label).slice(0,128)}):item)
 agentActivity.revision++
}
