<script lang="ts">
 import {describeHistoryStacks} from '$lib/agent/historyNavigation'
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,jumpNestedHistory} from '$lib/state/nestedComposition.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 const history=$derived(describeHistoryStacks(nestedHistory.past,nestedHistory.future))
 const blocked=$derived(disabled||nestedHistory.busy||Boolean(agentSession.lock))
 function jump(id:string,side:'before'|'after'='after'){
  if(blocked)return
  try{jumpNestedHistory(id,side)}catch(error){nestedHistory.error=error instanceof Error?error.message:String(error)}
 }
</script>
{#if nestedComposition.document||nestedHistory.future.length}
 <details class="card" aria-label="Шаги нативной истории">
  <summary>{nestedComposition.document?'Шаги истории':'История вложенной композиции'} · {history.cursor} / {history.entries.length}</summary>
  {#if history.entries.length}
   <button class="btn ghost sm" type="button" disabled={blocked||history.cursor===0} aria-current={history.cursor===0?'step':undefined} onclick={()=>jump(history.entries[0]!.id,'before')}>До первого доступного изменения</button>
   <ol>
    {#each history.entries as entry (entry.id)}
     <li><button class="btn ghost sm" type="button" data-native-history-position={entry.position} disabled={blocked||entry.current} aria-current={entry.current?'step':undefined} onclick={()=>jump(entry.id)}>Изменение {entry.position}</button> <span>{entry.state==='applied'?'Применено':'Отменено'}</span></li>
    {/each}
   </ol>
  {:else}<p>История пуста</p>{/if}
 </details>
{/if}
