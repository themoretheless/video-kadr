<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVignetteChannel,WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type Hsl=Extract<WireVideoEffect,{kind:'color_vignette'}>
 const channels:readonly (readonly [WireVignetteChannel,string,number,number,number])[]=[
 ['amount','Сила',-5,5,0],['midpoint','Середина, %',0,100,50],['roundness','Округлость',-100,100,0],['feather','Растушёвка, %',0,100,50]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:Hsl)=>Hsl):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='color_vignette'?change(effect):effect))
 }
 function keyframe(effect:Hsl,channel:WireVignetteChannel,base:number,start:number):Hsl {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation?.[channel]
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{...effect.animation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
 }
</script>
<section aria-label="Виньетка">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'color_vignette',parameters:{}}])}>Добавить Виньетка</button></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='color_vignette'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Виньетка {index+1}</legend>
       <div class="row"><label>Включён <input aria-label={`${clip.id} Виньетка включён`} type="checkbox" checked={effect.parameters.enabled ?? true} onchange={event=>{const enabled=event.currentTarget.checked;edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,enabled}}))}} /></label>
</div>
       <div class="channels">
        {#each channels as [channel,label,min,max,base]}
         {@const animation=effect.animation?.[channel]}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} step="0.1" aria-label={`${clip.id} Виньетка ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Виньетка ${channel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Виньетка ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{const animation={...current.animation};delete animation[channel];return {...current,animation}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}<small>{animation.track.keyframes.map(p=>`${p.tick}: ${p.value}`).join(' · ')}</small>{/if}
         </div>
        {/each}
       </div>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'color_vignette',parameters:{}}))}>Сбросить Виньетка</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Виньетка</button>
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
