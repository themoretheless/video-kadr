<script lang="ts">
 import type {WireVideoEffect} from '$lib/composition/types'
 import {reactivateCompositionSourceEffect,compositionState} from '$lib/state/composition.svelte'
 let {clipId,effects=[],disabled=false}:{clipId:string;effects?:readonly WireVideoEffect[];disabled?:boolean}=$props()
 let busy=$state(false),error=$state(''),generation=0
 $effect(()=>{const selected=clipId;void selected;generation++;busy=false;error=''})
 async function enable(index:number){
  const id=clipId,request=++generation;busy=true;error=''
  try{await reactivateCompositionSourceEffect(id,index,()=>{if(generation!==request||clipId!==id||compositionState.ui.selectedClipId!==id||disabled)throw new Error('Выбранный клип изменился')})}
  catch(reason){if(generation===request)error=reason instanceof Error?reason.message:String(reason)}
  finally{if(generation===request)busy=false}
 }
</script>
{#if effects.length>0}
 <fieldset disabled={disabled||busy}>
  <legend>Визуальные эффекты</legend>
  {#each effects as effect,index}
   {#if effect.kind==='concat_bypass'}<div><span>{effect.link.id}</span><button type="button" aria-label={`Включить исходный эффект ${index+1}`} onclick={()=>void enable(index)}>Включить</button></div>{:else}<div><span>{effect.kind==='native_bypass'?effect.effect.kind:effect.kind}</span><button type="button" aria-label={`Переключить эффект ${index+1}`} onclick={()=>void enable(index)}>{effect.kind==='native_bypass'?'Включить':'Отключить'}</button></div>{/if}
  {/each}
  {#if error}<p role="alert">{error}</p>{/if}
 </fieldset>
{/if}
<style>div{display:flex;gap:0.5rem;align-items:center;margin:0.4rem 0}</style>
