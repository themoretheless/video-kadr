<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,editNestedConcatTreatment,addNestedConcatTreatmentEffect} from '$lib/state/nestedComposition.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 import {concatVisualPackages} from '$lib/composition/concatVisualPackages'
 let effectId=$state('concat.mono')
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap(track=>track.kind==='graphic'?track.clips.filter(clip=>clip.concatTreatment).map(clip=>({track,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 const fields=[['strength','Сила воздействия',1,0.01],['fadeIn','Нарастание, с',undefined,0.1],['fadeOut','Затухание, с',undefined,0.1]] as const
 function edit(field:typeof fields[number][0],value:number){const target=current,composition=id,document=nestedComposition.document;if(!target||!Number.isFinite(value))return;void editNestedConcatTreatment(composition,target.track.id,target.clip.id,{[field]:value},()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный корректирующий слой изменился')})}
 function addEffect(){const target=current,composition=id,document=nestedComposition.document;if(!target)return;void addNestedConcatTreatmentEffect(composition,target.track.id,target.clip.id,effectId,()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный корректирующий слой изменился')})}
</script>
{#if current}
 <details><summary>Корректирующий слой Concat</summary>
   <label>Клип<select aria-label="Корректирующий клип Concat" disabled={disabled||nestedHistory.busy||Boolean(agentSession.lock)} value={current.clip.id} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.id}</option>{/each}</select></label>
  <fieldset disabled={disabled||nestedHistory.busy||current.track.locked||Boolean(agentSession.lock)}>

   {#each fields as [field,label,max,step](field)}
    <label>{label}<input aria-label={`Concat treatment ${field}`} type="number" min="0" {max} {step} value={current.clip.concatTreatment?.[field]} onchange={event=>{if(event.currentTarget.value!=='')edit(field,event.currentTarget.valueAsNumber)}} /></label>
   {/each}
   <label>Добавить эффект<select aria-label="Пакет эффекта корректирующего слоя Concat" bind:value={effectId}>{#each concatVisualPackages as [value,label](value)}<option {value}>{label}</option>{/each}</select></label>
   <button class="btn ghost sm" onclick={addEffect}>Добавить эффект в корректирующий слой</button>
   <p>Эффекты обрабатывают изображение ниже этого слоя. Фейды линейно меняют силу; верхние слои остаются без изменений.</p>
  </fieldset>
 </details>
{/if}
<style>fieldset{border:0;padding:0.5rem 0;display:grid;gap:0.5rem}label{display:flex;gap:0.5rem;align-items:center}input[type=number]{width:7rem}</style>
