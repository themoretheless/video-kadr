<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks,concatEffectNativeKeyOrigin} from '$lib/composition/concatEffectInspectorContext'
 import ConcatRadiusSourceKeyEditor from './ConcatRadiusSourceKeyEditor.svelte'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import {sampleGaussianBlurRadius,setGaussianBlurRadius,keyGaussianBlurRadius} from '$lib/composition/concatGaussianBlurAuthoring'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Key=Extract<WireVideoEffect,{kind:'concat_gaussian_blur'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Key)=>Key):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_gaussian_blur'?change(effect):effect))
 }

</script>
<section aria-label="Concat Gaussian Blur">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    {@const start=concatEffectNativeKeyOrigin(clip)}
    {@const localTime=Math.max(0,nestedComposition.timeUs-start)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_gaussian_blur',parameters:{}}])}>Добавить Concat Gaussian Blur</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_gaussian_blur'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Gaussian Blur {index+1}</legend>
       <label>{effect.concatKeys?.length?'Радиус · база, px':'Радиус, px'} <input type="number" min="1" max="50" step="1" aria-label={`${clip.id} Concat Gaussian Blur radius`} value={effect.concatKeys?.length?effect.parameters.radius??10:sampleGaussianBlurRadius(effect,localTime)} disabled={!!effect.concatKeys?.length || (!!effect.animation && nestedComposition.timeUs<start)} onchange={event=>{const text=event.currentTarget.value;const value=Number(text);if(text.trim() && Number.isFinite(value) && value>=1 && value<=50)edit(track.id,clip.id,index,current=>setGaussianBlurRadius(current,localTime,value))}} /></label>
       <div class="row"><button class="btn ghost sm" aria-label={`${clip.id} Concat Gaussian Blur radius ключ`} disabled={nestedComposition.timeUs<start || !!effect.concatKeys?.length} onclick={()=>edit(track.id,clip.id,index,current=>keyGaussianBlurRadius(current,localTime))}>Ключ</button>
       {#if effect.animation}<button class="btn ghost sm" aria-label={`${clip.id} Concat Gaussian Blur radius удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,animation:undefined}))}>Удалить анимацию</button>{/if}</div>
       {#if effect.concatKeys?.length}<small>Ключи Concat · доля длительности клипа: {effect.concatKeys.map(p=>`${p.at}: ${p.value}`).join(' · ')}</small><button class="btn ghost sm" aria-label={`${clip.id} Concat Gaussian Blur удалить ключи Concat`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <ConcatRadiusSourceKeyEditor {effect} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={change=>edit(track.id,clip.id,index,change)} />
       {#if effect.animation?.mode==='keyframes'}<small>{effect.animation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
       <div class="row"><button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_gaussian_blur',parameters:{}}))}>Сбросить Concat Gaussian Blur</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Gaussian Blur</button></div>
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
