import {agentActivity} from './activity.svelte'
import {agentSession} from './session.svelte'
import {nestedComposition,nestedHistory} from '../state/nestedComposition.svelte'
import {visibleNestedCheckpoints} from './nestedCheckpoints.svelte'
import {executeAgentRequest} from './runtime.svelte'
export const activityCheckpointAction=$state<{workspaceId:string|null;requestId:string|null}>({workspaceId:null,requestId:null})
function admit(id:string,workspaceId:string|undefined){
 if(!workspaceId||workspaceId!==agentActivity.workspaceId)throw new Error('Проект уже закрыт; обновите ленту')
 if(agentSession.lock&&agentSession.lock.expiresAt>Date.now())throw new Error('История заблокирована')
 if(nestedComposition.document&&nestedHistory.busy)throw new Error('Вложенный проект занят')
 if(!(nestedComposition.document?visibleNestedCheckpoints():agentSession.checkpoints).some(point=>point.id===id))throw new Error('Контрольная точка недоступна')
}
export async function restoreActivityCheckpoint(id:string,workspaceId:string|undefined):Promise<void>{
 admit(id,workspaceId)
 if(activityCheckpointAction.workspaceId===workspaceId&&activityCheckpointAction.requestId)throw new Error('Восстановление уже выполняется')
 const requestId=crypto.randomUUID()
 activityCheckpointAction.workspaceId=workspaceId!;activityCheckpointAction.requestId=requestId
 try{
  const args:Record<string,unknown>={id}
  if(nestedComposition.document){
   const snapshot=await executeAgentRequest('user','get_project',{}) as {projectHash:string}
   admit(id,workspaceId)
   args.expectedProjectHash=snapshot.projectHash
  }
  await executeAgentRequest('user','restore_checkpoint',args)
 }finally{if(activityCheckpointAction.requestId===requestId){activityCheckpointAction.requestId=null;activityCheckpointAction.workspaceId=null}}
}
