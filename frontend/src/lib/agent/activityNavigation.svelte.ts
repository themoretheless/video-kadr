import type {AffectedEntity} from './affectedEntities'
import {agentActivity} from './activity.svelte'
import {activityReveal,type ActivityReveal} from './activityReveal.svelte'
import {agentSession} from './session.svelte'
import {compositionState,selectCompositionClip} from '../state/composition/core.svelte'
import {nestedComposition,nestedHistory} from '../state/nestedComposition.svelte'
/** Resolve against live, scoped IDs. Labels are historical and must never identify a destination. */
export function activityNavigationTarget(refs:readonly AffectedEntity[],workspaceId:string|undefined):Omit<ActivityReveal,'id'|'workspaceId'>|null {
 if(workspaceId!==agentActivity.workspaceId)return null
 for(const kind of ['Layer','Track'] as const)for(const ref of refs){
  if(ref.kind!==kind)continue
  const native=nestedComposition.document
  const document=native?native.compositions[ref.compositionId]?.content:ref.compositionId==='ordinary'?compositionState.document:null
  if(!document)continue
  const track=kind==='Layer'?document.tracks.find(track=>track.clips.some(clip=>clip.id===ref.id)):document.tracks.find(track=>track.id===ref.id)
  if(track)return {mode:native?'nested':'ordinary',compositionId:ref.compositionId,trackId:track.id,clipId:kind==='Layer'?ref.id:null}
 }
 return null
}
export function revealActivityAffected(refs:readonly AffectedEntity[],workspaceId:string|undefined):boolean {
 const target=activityNavigationTarget(refs,workspaceId)
 if(!target||target.mode==='nested'&&nestedHistory.busy)return false
 if(target.mode==='ordinary'&&target.clipId)selectCompositionClip(target.trackId,target.clipId,{single:true})
 if(target.mode==='nested')nestedComposition.activeCompositionId=target.compositionId
 // Navigating never seeks, ends agent work, releases locks, or creates an Undo entry.
 agentSession.view='editor'
 activityReveal.request={...target,id:crypto.randomUUID(),workspaceId:workspaceId!}
 return true
}
