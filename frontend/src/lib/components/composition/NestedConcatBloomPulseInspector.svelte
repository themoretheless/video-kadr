<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import ConcatBloomPulseSourceKeyEditor from './ConcatBloomPulseSourceKeyEditor.svelte'
 import type {BloomPulseChannel} from '$lib/composition/concatBloomPulseSourceKeys'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_bloom_pulse'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_bloom_pulse'?change(effect):effect))
 }

</script>
<section aria-label="Concat Bloom Pulse">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_bloom_pulse',parameters:{}}])}>Добавить Concat Bloom Pulse</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_bloom_pulse'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Bloom Pulse {index+1}</legend>
       {#each [{key:'amount',label:'Сила, %',value:50,min:0,max:100},{key:'speed',label:'Скорость',value:2,min:0.5,max:5}] as field (field.key)}
        <label>{field.label}{effect.concatKeys?.[field.key as BloomPulseChannel]?.length?' · база':''} <input disabled={!!effect.concatKeys?.[field.key as BloomPulseChannel]?.length} type="number" min={field.min} max={field.max} step="any" aria-label={`${clip.id} Concat Bloom Pulse ${field.key}`} value={effect.parameters[field.key as keyof typeof effect.parameters] ?? field.value} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim()&&Number.isFinite(value)&&value>=field.min&&value<=field.max)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[field.key]:value}}))}} /></label>
       {/each}
       {#each ['amount','speed'] as channel (channel)}
        <ConcatBloomPulseSourceKeyEditor {effect} channel={channel as BloomPulseChannel} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />
       {/each}
       {#if effect.concatKeys}<button class="btn ghost sm" aria-label={`${clip.id} Concat Bloom Pulse удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_bloom_pulse',parameters:{}}))}>Сбросить Concat Bloom Pulse</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Bloom Pulse</button></div>
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
