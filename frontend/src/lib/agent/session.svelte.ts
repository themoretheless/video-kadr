import {resetAgentActivity,recordActivitySession,endActivitySession} from './activity.svelte'
import {SvelteDate} from 'svelte/reactivity'
import type { Composition, CompositionRenderOutput } from '../composition/types.js'
import type { CompositionMedia } from '../state/composition/core.svelte.js'

export interface AgentCheckpoint {
  actor?: string
  id: string
  label: string
  createdAt: number
  document: Composition
  media: Record<string, CompositionMedia>
  output: CompositionRenderOutput
}

export interface AgentWorkSession {id:string;owner:string;reason:string;checkpointId:string;startedAt:number;kind:'ordinary'|'nested'}
export const agentSession = $state({
  view:'editor' as 'editor'|'agent',
  work:null as AgentWorkSession|null,
  revision: 0,
  lock: null as { owner: string; reason: string; expiresAt: number } | null,
  checkpoints: [] as AgentCheckpoint[],
  log: [] as { id: string; actor: string; tool: string; status: 'started' | 'ok' | 'error'; time: number; detail?: string }[],
})

export function assertHistoryUnlocked(owner?: string): void {
  const lock = agentSession.lock
  if (lock && lock.expiresAt <= Date.now()) agentSession.lock = null
  else if (lock && lock.owner !== owner) throw new Error(`История занята агентом: ${lock.reason}`)
}

export function clearAgentWorkContext(): void { resetAgentActivity();agentSession.work=null;agentSession.view='editor';agentSession.lock=null }
export function beginAgentWork(actor:string,reason:unknown,kind:AgentWorkSession['kind'],checkpoint:(label:string)=>{id:string}) {
  if(!actor||actor.length>128)throw new Error('Invalid actor')
  if(typeof reason!=='string'||!reason.trim()||reason.length>512)throw new Error('Invalid session reason')
  const current=agentSession.work
  if(current){if(current.owner!==actor)throw new Error(`AgentSessionBusy: ${current.owner}`);return workRecord(current)}
  let label='Pre-agent: '
  for(const ch of reason){if(label.length+ch.length>128)break;label+=ch}
  const point=checkpoint(label)
  const work={id:crypto.randomUUID(),owner:actor,reason,checkpointId:point.id,startedAt:Date.now(),kind}
  agentSession.work=work;recordActivitySession(work);agentSession.view='agent'
  return workRecord(work)
}
function workRecord(work:AgentWorkSession){return {session_id:work.id,checkpoint_id:work.checkpointId,started_at:new SvelteDate(work.startedAt).toISOString(),owner:work.owner,reason:work.reason}}
export function endAgentWork(actor:string,force:unknown=false,endReason?:string){
  if(typeof force!=='boolean')throw new Error('force must be boolean')
  const work=agentSession.work
  if(!work)return {ended:null}
  if(work.owner!==actor&&!force)throw new Error(`AgentSessionBusy: ${work.owner}`)
  if(agentSession.lock?.owner===work.owner)agentSession.lock=null
  endActivitySession(work.id,endReason??(actor==='user'?'user':force&&actor!==work.owner?'forced':'agent'))
  agentSession.work=null
  return {ended:work.id}
}
export function releaseAgentConnection(actor:string):void {
  if(agentSession.work?.owner===actor)endAgentWork(actor,false,'disconnected')
  if(agentSession.lock?.owner===actor)agentSession.lock=null
}
export function clearAgentProjectSession(): void {
  clearAgentWorkContext()
  agentSession.lock = null
  agentSession.checkpoints = []
}

export function bumpAgentRevision(): void { agentSession.revision += 1 }
