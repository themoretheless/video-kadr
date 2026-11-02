<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_luma_key'}>
 type Channel='threshold'|'softness'
 const channels:readonly (readonly [Channel,string,number,number,number])[]=[['threshold','Порог, %',0,100,10],['softness','Мягкость, %',0,50,10]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_luma_key'?change(effect):effect))
 }

</script>
<section aria-label="Concat Luma Key">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_luma_key',parameters:{}}])}>Добавить Concat Luma Key</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_luma_key'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Luma Key {index+1}</legend>
       <label>Инверсия <input type="checkbox" aria-label={`${clip.id} Concat Luma Key invert`} checked={(effect.parameters.invert ?? 0)>0.5} onchange={event=>{const invert=event.currentTarget.checked?1:0;edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,invert}}))}} /></label>
       {#each channels as [channel,label,min,max,base] (channel)}
        <label>{label} <input type="number" {min} {max} step="1" aria-label={`${clip.id} Concat Luma Key ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isFinite(value) && value>=min && value<=max)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
       {/each}
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_luma_key',parameters:{}}))}>Сбросить Concat Luma Key</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Luma Key</button></div>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>
 .row {display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap;}
 fieldset {border:0;padding:0.5rem 0;margin:0;display:flex;flex-wrap:wrap;gap:0.75rem;}
 label {display:flex;align-items:center;gap:0.4rem;}
 input[type=number] {width:4.5rem;}
</style>
