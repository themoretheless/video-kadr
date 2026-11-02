<script lang="ts">
 import {describeHistoryStacks} from '$lib/agent/historyNavigation'
 import {agentSession} from '$lib/agent/session.svelte'
 import {compositionState} from '$lib/state/composition/core.svelte'
 import {jumpCompositionHistory} from '$lib/state/composition/transport.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 let error=$state('')
 const history=$derived(describeHistoryStacks(compositionState.history.past,compositionState.history.future))
 const blocked=$derived(disabled||compositionState.save.busy||Boolean(agentSession.lock))
 $effect(()=>{const revision=agentSession.revision;void revision;error=''})
 function jump(id:string,side:'before'|'after'='after'){
  if(blocked)return
  error=''
  try{jumpCompositionHistory(id,side)}catch(cause){error=cause instanceof Error?cause.message:String(cause)}
 }
</script>
<details class="card" aria-label="Шаги истории обычной композиции">
 <summary>Шаги истории · {history.cursor} / {history.entries.length}</summary>
 {#if history.entries.length}
  <button class="btn ghost sm" type="button" disabled={blocked||history.cursor===0} aria-current={history.cursor===0?'step':undefined} onclick={()=>jump(history.entries[0]!.id,'before')}>До первого доступного изменения</button>
  <ol>
   {#each history.entries as entry (entry.id)}
    <li><button class="btn ghost sm" type="button" data-history-position={entry.position} disabled={blocked||entry.current} aria-current={entry.current?'step':undefined} onclick={()=>jump(entry.id)}>Изменение {entry.position}</button> <span>{entry.state==='applied'?'Применено':'Отменено'}</span></li>
   {/each}
  </ol>
 {:else}<p>История пуста</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
