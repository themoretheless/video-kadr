<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks,concatEffectNativeKeyOrigin} from '$lib/composition/concatEffectInspectorContext'
 import ConcatWhiteBalanceSourceKeyEditor from "./ConcatWhiteBalanceSourceKeyEditor.svelte"
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type SharpChannel='temperature'|'tint'
 type Hsl=Extract<WireVideoEffect,{kind:'concat_white_balance'}>
 const channels:readonly (readonly [SharpChannel,string,number,number,number,number])[]=[
 ['temperature','Температура, K',2000,11000,50,6500],['tint','Оттенок, %',-100,100,1,0]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
 function edit(track:string,clip:string,index:number,change:(effect:Hsl)=>Hsl):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='concat_white_balance'?change(effect):effect))
 }
 function keyframe(effect:Hsl,channel:SharpChannel,base:number,start:number):Hsl {
  if(Object.values(effect.concatKeys??{}).some(run=>run.length))throw new Error("Исходные ключи Concat уже управляют балансом белого")
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation?.[channel]
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{...effect.animation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
 }
</script>
<section aria-label="Concat White Balance">
 {#each tracks as track (track.id)}
  {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'concat_white_balance',parameters:{}}])}>Добавить Concat White Balance</button></div>
    {#each (clip.effects ?? []) as effect,index (index)}
     {#if effect.kind==='concat_white_balance'}
      <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
       <legend>Concat White Balance {index+1}</legend>

       <div class="channels">
        {#each channels as [channel,label,min,max,step,base] (channel)}
         {@const animation=effect.animation?.[channel]}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} {step} aria-label={`${clip.id} Concat White Balance ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Concat White Balance ${channel} ключ`} disabled={Object.values(effect.concatKeys??{}).some(run=>run.length) || nestedComposition.timeUs<(concatEffectNativeKeyOrigin(clip))} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,concatEffectNativeKeyOrigin(clip)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Concat White Balance ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{const animation={...current.animation};delete animation[channel];return {...current,animation:Object.keys(animation).length?animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          <ConcatWhiteBalanceSourceKeyEditor {effect} {channel} clipId={clip.id} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)||!!effect.animation} onchange={change=>edit(track.id,clip.id,index,change)} />
          {#if animation?.mode==='keyframes'}<small>{animation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
         </div>
        {/each}
       </div>
       {#if effect.concatKeys}<button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,current=>({...current,concatKeys:undefined}))}>Удалить ключи Concat</button>{/if}
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'concat_white_balance',parameters:{}}))}>Сбросить Concat White Balance</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Concat White Balance</button>
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
