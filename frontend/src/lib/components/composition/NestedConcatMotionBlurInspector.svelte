<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
 import ConcatMotionSourceKeyEditor from './ConcatMotionSourceKeyEditor.svelte'
 import type {MotionChannel} from '$lib/composition/concatMotionSourceKeys'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_motion_blur'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_motion_blur'?change(effect):effect))
 }

</script>
<section aria-label="Concat Motion Blur">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_motion_blur',parameters:{}}])}>Добавить Concat Motion Blur</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_motion_blur'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Motion Blur {index+1}</legend>
       {#each [{key:'length',label:'Длина, px',value:18,min:2,max:60},{key:'angle',label:'Угол, °',value:0,min:-90,max:90}] as field (field.key)}
        <label>{field.label}{effect.concatKeys?.[field.key as MotionChannel]?.length?' · база':''} <input disabled={!!effect.concatKeys?.[field.key as MotionChannel]?.length} type="number" min={field.min} max={field.max} step="any" aria-label={`${clip.id} Concat Motion Blur ${field.key}`} value={effect.parameters[field.key as keyof typeof effect.parameters] ?? field.value} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim()&&Number.isFinite(value)&&value>=field.min&&value<=field.max)edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[field.key]:value}}))}} /></label>
       {/each}
       {#each ['length','angle'] as channel (channel)}
        <ConcatMotionSourceKeyEditor {effect} channel={channel as MotionChannel} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />
       {/each}
       {#if effect.concatKeys}<button class="btn ghost sm" aria-label={`${clip.id} Concat Motion Blur удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_motion_blur',parameters:{}}))}>Сбросить Concat Motion Blur</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Motion Blur</button></div>
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
