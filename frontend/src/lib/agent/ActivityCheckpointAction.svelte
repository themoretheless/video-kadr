<script lang="ts">
 import {onDestroy} from 'svelte'
 import {agentActivity} from './activity.svelte'
 import {activityCheckpointAction,restoreActivityCheckpoint} from './activityCheckpoint.svelte'
 import {activityError} from './activityDetails'
 import {nestedHistory} from '../state/nestedComposition.svelte'
 let {id,workspaceId,available,locked=false}:{id:string;workspaceId?:string;available:boolean;locked?:boolean}=$props()
 let error=$state(''),alive=true
 onDestroy(()=>{alive=false})
 const busy=$derived(activityCheckpointAction.workspaceId===workspaceId&&activityCheckpointAction.requestId!==null)
 const valid=$derived(available&&workspaceId===agentActivity.workspaceId)
 async function restore(){error='';try{await restoreActivityCheckpoint(id,workspaceId)}catch(cause){if(alive)error=activityError(cause)}}
</script>
<button type="button" class="btn ghost sm" disabled={!valid||locked||busy||nestedHistory.busy} onclick={()=>void restore()}>{!valid?'Точка недоступна':busy?'Восстановление…':'Восстановить точку'}</button>
{#if error}<p role="alert">{error}</p>{/if}
