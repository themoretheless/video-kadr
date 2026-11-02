<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import ConcatChromaSourceKeyEditor from './ConcatChromaSourceKeyEditor.svelte'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_chroma_key'}>
 type Channel='similarity'|'softness'|'spill'
 const channels:readonly (readonly [Channel,string,number,number,number])[]=[['similarity','Сходство, %',1,60,30],['softness','Мягкость, %',0,50,10],['spill','Подавление оттенка, %',0,100,50]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_chroma_key'?change(effect):effect))
 }
 function color(value:number):string {return '#'+Math.floor(value/256).toString(16).padStart(6,'0')}
</script>
<section aria-label="Concat Chroma Key">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_chroma_key',parameters:{}}])}>Добавить Concat Chroma Key</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_chroma_key'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Chroma Key {index+1}</legend>
       <label>Цвет ключа{effect.concatKeys?.color?.length?' · база':''} <input type="color" disabled={!!effect.concatKeys?.color?.length} aria-label={`${clip.id} Concat Chroma Key color`} value={color(effect.parameters.color ?? 0x00b140ff)} onchange={event=>{const hex=event.currentTarget.value;if(/^#[0-9a-f]{6}$/i.test(hex))edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,color:parseInt(hex.slice(1),16)*256+((current.parameters.color ?? 0x00b140ff)%256)}}))}} /></label>
       {#each channels as [channel,label,min,max,base] (channel)}
        <label>{label}{effect.concatKeys?.[channel]?.length?' · база':''} <input type="number" disabled={!!effect.concatKeys?.[channel]?.length} {min} {max} step="1" aria-label={`${clip.id} Concat Chroma Key ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isFinite(value) && value>=min && value<=max)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
       {/each}
       {#if effect.concatKeys}{#each Object.entries(effect.concatKeys) as [channel,keys] (channel)}{#if keys?.length}<small>Ключи Concat {channel}: {keys.map(key=>`${key.at}: ${key.value}`).join(' · ')}</small>{/if}{/each}
       <button class="btn ghost sm" aria-label={`${clip.id} Concat Chroma Key удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       {#each ['color','similarity','softness','spill'] as channel (channel)}<ConcatChromaSourceKeyEditor {effect} channel={channel as 'color'|'similarity'|'softness'|'spill'} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />{/each}
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_chroma_key',parameters:{}}))}>Сбросить Concat Chroma Key</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Chroma Key</button></div>
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
