<script lang="ts">
 import {untrack} from 'svelte'
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedAgentGeneration,nestedHistory,editNestedConcatTitle} from '$lib/state/nestedComposition.svelte'
 import type {WireTextClip} from '$lib/composition/types'
 let {compositionId,trackId,clip,disabled=false}:{compositionId:string;trackId:string;clip:WireTextClip;disabled?:boolean}=$props()
 let draft=$state(''),base=$state(''),scope=$state(''),error=$state('')
 const conflict=$derived(base!==clip.text)
 $effect(()=>{
  const next=`${nestedComposition.projectId}/${nestedAgentGeneration()}/${compositionId}/${trackId}/${clip.id}`,text=clip.text
  untrack(()=>{if(scope!==next||draft===base){scope=next;base=text;draft=text;error=''}})
 })
 function reset(){base=clip.text;draft=clip.text;error=''}
 async function save(){
  const submitted=draft,expected=base,startedScope=scope,target=clip.id,composition=compositionId,track=trackId
  error=''
  await editNestedConcatTitle(composition,track,target,{},()=>{
   if(scope!==startedScope||compositionId!==composition||trackId!==track||clip.id!==target)throw new Error('Выбранный титр изменился')
  },{value:submitted,expected})
  if(scope!==startedScope)return
  if(nestedHistory.error)error=nestedHistory.error
  else if(nestedComposition.document?.compositions[composition]?.content.tracks.some(lane=>lane.id===track&&lane.kind==='text'&&lane.clips.some(title=>title.id===target&&title.text===submitted)))base=submitted
  else error='Сохранение отменено: проект изменился'
 }
</script>
<label>Текст титра<textarea aria-label="Текст титра Concat" rows="4" bind:value={draft} disabled={disabled||nestedHistory.busy||Boolean(agentSession.lock)}></textarea></label>
{#if conflict}<p role="alert">Текст титра изменился. Сбросьте черновик, чтобы загрузить актуальный текст.</p>{/if}
<div>
 <button class="btn ghost sm" type="button" disabled={disabled||nestedHistory.busy||Boolean(agentSession.lock)||draft===base||conflict} onclick={()=>void save()}>Сохранить текст титра</button>
 <button class="btn ghost sm" type="button" disabled={nestedHistory.busy} onclick={reset}>Сбросить черновик титра</button>
</div>
{#if error}<p role="alert">{error}</p>{/if}
<style>label{display:grid;gap:0.5rem}textarea{width:100%;box-sizing:border-box}div{display:flex;gap:0.5rem}</style>
