<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import ConcatShiftSourceKeyEditor from './ConcatShiftSourceKeyEditor.svelte'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_chromatic_aberration'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_chromatic_aberration'?change(effect):effect))
 }

</script>
<section aria-label="Concat Chromatic Aberration">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_chromatic_aberration',parameters:{}}])}>Добавить Concat Chromatic Aberration</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_chromatic_aberration'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Chromatic Aberration {index+1}</legend>
       <label>Сдвиг каналов, px <input type="number" min="1" max="24" step="any" aria-label={`${clip.id} Concat Chromatic Aberration shift`} disabled={!!effect.concatKeys?.length} value={effect.parameters.shift ?? 4} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isFinite(value) && value>=1 && value<=24)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,shift:value}}))}} /></label>
       {#if effect.concatKeys?.length}<small>Сдвиг · база; воспроизводятся ключи Concat.</small><button class="btn ghost sm" aria-label={`${clip.id} Concat Chromatic Aberration удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <ConcatShiftSourceKeyEditor {effect} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_chromatic_aberration',parameters:{}}))}>Сбросить Concat Chromatic Aberration</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Chromatic Aberration</button></div>
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
