<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type EdgeFeatherChannel='amount'
 type EdgeFeather=Extract<WireVideoEffect,{kind:'edge_feather'}>
 const channels:readonly (readonly [EdgeFeatherChannel,string,number,number,number])[]=[
 ['amount','Растушёвка, %',0,100,0]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:EdgeFeather)=>EdgeFeather):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='edge_feather'?change(effect):effect))
 }
 function keyframe(effect:EdgeFeather,channel:EdgeFeatherChannel,base:number,start:number):EdgeFeather {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }
 function editKey(effect:EdgeFeather,tick:number,patch:{tick?:number;value?:number}|null):EdgeFeather {
  const previous=effect.animation
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}
 }
</script>
<section aria-label="Edge Feather">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'edge_feather',parameters:{}}])}>Добавить Edge Feather</button></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='edge_feather'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Edge Feather {index+1}</legend>

       <div class="channels">
        {#each channels as [channel,label,min,max,base]}
         {@const animation=effect.animation}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} step="0.1" aria-label={`${clip.id} Edge Feather ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Edge Feather ${channel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Edge Feather ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{return {...current,animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}
           <details><summary>Растушёвка · {animation.track.keyframes.length} ключей</summary>
            {#each animation.track.keyframes as key (key.tick)}
             <div class="row">
              <label>Время, с<input aria-label={`${clip.id} Edge Feather ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick/animation.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{tick:Math.round(Number(event.currentTarget.value)*animation.track.timeBase)}))} /></label>
              <label>Значение<input aria-label={`${clip.id} Edge Feather ${key.tick} value`} type="number" {min} {max} step="0.1" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
              <button class="btn ghost sm" aria-label={`${clip.id} Edge Feather ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,null))}>Удалить ключ</button>
             </div>
            {/each}
           </details>
          {/if}
         </div>
        {/each}
       </div>
       <small>0 — исходное изображение. Поле задаёт базовое значение и значение нового ключа.</small>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'edge_feather',parameters:{}}))}>Сбросить Edge Feather</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Edge Feather</button>
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
 fieldset > button {margin-right:6px;}
 .channels {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,22rem),1fr));gap:0.25rem 0.75rem;max-width:60rem;}
 input[type=number] {width:4.5rem;} small {display:block;}
</style>
