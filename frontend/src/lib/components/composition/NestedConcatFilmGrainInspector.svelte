<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import ConcatFilmGrainSourceKeyEditor from './ConcatFilmGrainSourceKeyEditor.svelte'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_film_grain'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_film_grain'?change(effect):effect))
 }

</script>
<section aria-label="Concat Film Grain">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_film_grain',parameters:{}}])}>Добавить Concat Film Grain</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_film_grain'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Film Grain {index+1}</legend>
       <label>Интенсивность <input type="number" min="2" max="40" step="any" aria-label={`${clip.id} Concat Film Grain amount`} disabled={!!effect.concatKeys?.length} value={effect.parameters.amount ?? 12} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isFinite(value) && value>=2 && value<=40)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,amount:value}}))}} /></label>
       {#if effect.concatKeys?.length}<small>Интенсивность · база; воспроизводятся ключи Concat.</small><button class="btn ghost sm" aria-label={`${clip.id} Concat Film Grain удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <ConcatFilmGrainSourceKeyEditor {effect} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_film_grain',parameters:{}}))}>Сбросить Concat Film Grain</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Film Grain</button></div>
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
