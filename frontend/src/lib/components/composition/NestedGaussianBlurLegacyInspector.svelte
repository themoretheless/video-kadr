<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type GaussianBlurChannel='blurriness'
 type GaussianBlur=Extract<WireVideoEffect,{kind:'gaussian_blur_legacy'}>
 const channels:readonly (readonly [GaussianBlurChannel,string,number,number,number])[]=[
 ['blurriness','Размытие',0,3000,0]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:GaussianBlur)=>GaussianBlur):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='gaussian_blur_legacy'?change(effect):effect))
 }
 function keyframe(effect:GaussianBlur,channel:GaussianBlurChannel,base:number,start:number):GaussianBlur {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }
 function editKey(effect:GaussianBlur,tick:number,patch:{tick?:number;value?:number}|null):GaussianBlur {
  const previous=effect.animation
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}
 }
</script>
<section aria-label="Gaussian Blur Legacy">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'gaussian_blur_legacy',parameters:{}}])}>Добавить Gaussian Blur Legacy</button></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='gaussian_blur_legacy'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Gaussian Blur Legacy {index+1}</legend>

       <label>Оси<select aria-label={`${clip.id} Gaussian Blur Legacy dimensions`} value={effect.parameters.dimensions ?? 0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,dimensions:Number(event.currentTarget.value) as 0|1|2}}))}><option value="0">Горизонталь и вертикаль</option><option value="1">Горизонталь</option><option value="2">Вертикаль</option></select></label>
       <div class="channels">
        {#each channels as [channel,label,min,max,base]}
         {@const animation=effect.animation}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} step="0.1" aria-label={`${clip.id} Gaussian Blur Legacy ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Gaussian Blur Legacy ${channel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Gaussian Blur Legacy ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{return {...current,animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}
           <details><summary>Размытие · {animation.track.keyframes.length} ключей</summary>
            {#each animation.track.keyframes as key (key.tick)}
             <div class="row">
              <label>Время, с<input aria-label={`${clip.id} Gaussian Blur Legacy ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick/animation.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{tick:Math.round(Number(event.currentTarget.value)*animation.track.timeBase)}))} /></label>
              <label>Значение<input aria-label={`${clip.id} Gaussian Blur Legacy ${key.tick} value`} type="number" {min} {max} step="0.1" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
              <button class="btn ghost sm" aria-label={`${clip.id} Gaussian Blur Legacy ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,null))}>Удалить ключ</button>
             </div>
            {/each}
           </details>
          {/if}
         </div>
        {/each}
       </div>
       <small>0 — исходное изображение. Поле задаёт базовое значение и значение нового ключа.</small>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'gaussian_blur_legacy',parameters:{}}))}>Сбросить Gaussian Blur Legacy</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Gaussian Blur Legacy</button>
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
