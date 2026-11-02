import {untrack} from 'svelte'
import {agentActivity,latestCompletedActivityIds} from './activity.svelte'
import {agentSession} from './session.svelte'
import {nestedComposition} from '../state/nestedComposition.svelte'
import {visibleNestedCheckpoints} from './nestedCheckpoints.svelte'
import {ActivityPublisher,type ActivityTransport} from './activitySync'
const publisher=new ActivityPublisher(()=>({
 workspaceId:agentActivity.workspaceId,activities:agentActivity.activities,sessions:agentActivity.sessions,evicted:agentActivity.evicted,
 completedIds:latestCompletedActivityIds(agentActivity.activities,1000),sessionId:agentSession.work?.id??null,
 lockReason:agentSession.lock&&agentSession.lock.expiresAt>Date.now()?agentSession.lock.reason:null,
 checkpoints:(nestedComposition.document?visibleNestedCheckpoints():agentSession.checkpoints).map(({id,label})=>({id,label}))
}))
let subscribers=0,stop:(()=>void)|null=null
/** Local owned event boundary. The renderer consumes detached deltas instead of owner state. */
export const activityTransport:ActivityTransport={
 snapshot:()=>publisher.snapshot(),
 subscribe(listener){
  const unsubscribe=publisher.subscribe(value=>untrack(()=>listener(value)))
  subscribers++
  if(!stop)stop=$effect.root(()=>{$effect(()=>{publisher.refresh()})})
  return ()=>{unsubscribe();subscribers--;if(!subscribers){stop?.();stop=null}}
 }
}
