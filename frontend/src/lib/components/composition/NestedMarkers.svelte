<script lang="ts">
 import {nestedComposition,nestedHistory,editNestedMetadata} from '$lib/state/nestedComposition.svelte'
 import {agentSession,assertHistoryUnlocked} from '$lib/agent/session.svelte'
 import {createComposition} from '$lib/composition/commands'
 import {upsertCompositionMarker,deleteCompositionMarker,type CompositionMarker} from '$lib/composition/markers'
 let {disabled=false}:{disabled?:boolean}=$props()
 const project=$derived(nestedComposition.document)
 const active=$derived(nestedComposition.activeCompositionId??project?.root??'')
 const markers=$derived(project?.compositions[active]?.content.metadata?.markers??[])
 const blocked=$derived(disabled||nestedHistory.busy||Boolean(agentSession.lock))
 let label=$state('Маркер'),color=$state('#ffd166')
 async function change(marker:CompositionMarker|string){
  if(blocked)return
  await editNestedMetadata(active,metadata=>{
   const source={...createComposition({width:16,height:16,fps:10,backgroundColor:'#000000'}),markers:metadata.markers??[]}
   const next=typeof marker==='string'?deleteCompositionMarker(source,marker):upsertCompositionMarker(source,marker)
   return {...metadata,markers:(next as typeof source).markers}
  },()=>assertHistoryUnlocked())
 }
 function seek(tick:number){if(!disabled&&!nestedHistory.busy)nestedComposition.timeUs=tick}
 const previous=$derived([...markers].filter(marker=>marker.tick<nestedComposition.timeUs).sort((a,b)=>b.tick-a.tick)[0])
 const next=$derived([...markers].filter(marker=>marker.tick>nestedComposition.timeUs).sort((a,b)=>a.tick-b.tick)[0])
</script>
{#if project}
 <section class="card" aria-label="Маркеры нативной композиции">
  <h3>Маркеры</h3>
  <button class="btn ghost sm" disabled={disabled||nestedHistory.busy||!previous} onclick={()=>previous&&seek(previous.tick)}>Предыдущий маркер</button>
  <button class="btn ghost sm" disabled={disabled||nestedHistory.busy||!next} onclick={()=>next&&seek(next.tick)}>Следующий маркер</button>
  <fieldset disabled={blocked}>
   <legend>Добавить на позиции курсора</legend>
   <label>Название <input aria-label="Название нового маркера" bind:value={label}/></label>
   <label>Цвет <input aria-label="Цвет нового маркера" type="color" bind:value={color}/></label>
   <button class="btn ghost sm" onclick={()=>void change({id:`marker-${crypto.randomUUID()}`,tick:nestedComposition.timeUs,label,color})}>Добавить маркер</button>
  </fieldset>
  {#if !markers.length}<p>Нет маркеров</p>{/if}
  {#each markers as marker (marker.id)}
   <div data-native-marker={marker.id}>
    <button class="btn ghost sm" disabled={disabled||nestedHistory.busy} onclick={()=>seek(marker.tick)}>Перейти к маркеру {marker.label}</button>
    <fieldset disabled={blocked}>
     <label>Название <input aria-label={`Название маркера ${marker.id}`} value={marker.label} onchange={event=>void change({...marker,label:event.currentTarget.value})}/></label>
     <label>Время, с <input aria-label={`Время маркера ${marker.id}`} type="number" min="0" step="0.000001" value={marker.tick/1000000} onchange={event=>void change({...marker,tick:Math.round(event.currentTarget.valueAsNumber*1000000)})}/></label>
     <label>Цвет <input aria-label={`Цвет маркера ${marker.id}`} type="color" value={marker.color??'#ffd166'} onchange={event=>void change({...marker,color:event.currentTarget.value})}/></label>
     <button class="btn ghost sm" onclick={()=>void change(marker.id)}>Удалить маркер {marker.label}</button>
    </fieldset>
   </div>
  {/each}
 </section>
{/if}
