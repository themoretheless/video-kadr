<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks,concatEffectNativeKeyOrigin} from '$lib/composition/concatEffectInspectorContext'
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type SharpChannel='amount'|'intensity'
 type Hsl=Extract<WireVideoEffect,{kind:'concat_vibrance'}>
 const channels:readonly (readonly [SharpChannel,string,number,number,number,number])[]=[
 ['amount','Насыщенность',0.1,2,0.05,0.7],['intensity','Сила фильтра, %',0,100,1,100]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Hsl)=>Hsl):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_vibrance'?change(effect):effect))
 }
 function keyframe(effect:Hsl,channel:SharpChannel,base:number,start:number):Hsl {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation?.[channel]
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{...effect.animation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
 }
</script>
<section aria-label="Concat Vibrance">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_vibrance',parameters:{}}])}>Добавить Concat Vibrance</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_vibrance'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat Vibrance {index+1}</legend>

       <div class="channels">
        {#each channels as [channel,label,min,max,step,base] (channel)}
         {@const animation=effect.animation?.[channel]}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} {step} aria-label={`${clip.id} Concat Vibrance ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Concat Vibrance ${channel} ключ`} disabled={nestedComposition.timeUs<(concatEffectNativeKeyOrigin(clip))} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,concatEffectNativeKeyOrigin(clip)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Concat Vibrance ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{const animation={...current.animation};delete animation[channel];return {...current,animation:Object.keys(animation).length?animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}<small>{animation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
         </div>
        {/each}
       </div>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_vibrance',parameters:{}}))}>Сбросить Concat Vibrance</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat Vibrance</button>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>
 .row {display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap;}
 fieldset {border:0;padding:0.5rem 0;margin:0;}
 .channels {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,22rem),1fr));gap:0.25rem 0.75rem;max-width:60rem;}
 input[type=number] {width:4.5rem;} small {display:block;}
</style>
