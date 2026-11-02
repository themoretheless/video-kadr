<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_pixelate'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_pixelate'?change(effect):effect))
 }

</script>
<section aria-label="Concat Pixelate">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_pixelate',parameters:{}}])}>Добавить Concat Pixelate</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_pixelate'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Pixelate {index+1}</legend>
       <label>Размер блока, px <input type="number" min="2" max="64" step="1" aria-label={`${clip.id} Concat Pixelate size`} value={effect.parameters.size ?? 16} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isInteger(value) && value>=2 && value<=64)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,size:value}}))}} /></label>
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_pixelate',parameters:{}}))}>Сбросить Concat Pixelate</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Pixelate</button></div>
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
