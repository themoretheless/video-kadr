<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type VrDenoiseChannel='amount'
 type VrDenoise=Extract<WireVideoEffect,{kind:'vr_denoise'}>
 const channels:readonly (readonly [VrDenoiseChannel,string,number,number,number])[]=[
 ['amount','Уровень шума, %',0,100,0]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:VrDenoise)=>VrDenoise):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='vr_denoise'?change(effect):effect))
 }
 function keyframe(effect:VrDenoise,channel:VrDenoiseChannel,base:number,start:number):VrDenoise {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }
 function editKey(effect:VrDenoise,tick:number,patch:{tick?:number;value?:number}|null):VrDenoise {
  const previous=effect.animation
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}
 }
</script>
<section aria-label="VR De-Noise">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'vr_denoise',parameters:{}}])}>Добавить VR De-Noise</button></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='vr_denoise'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>VR De-Noise {index+1}</legend>

       <label>Размещение кадров<select aria-label={`${clip.id} VR De-Noise frameLayout`} value={String(effect.parameters.frameLayout??0)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,frameLayout:Number(event.currentTarget.value)}}))}><option value="0">Monoscopic</option><option value="1">Over/Under</option><option value="2">Side by Side</option></select></label>
       <label>Тип шума<select aria-label={`${clip.id} VR De-Noise noiseType`} value={String(effect.parameters.noiseType??0)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,noiseType:Number(event.currentTarget.value)}}))}><option value="0">Fast</option><option value="1">Slow</option></select></label>
       <label>Показать только шум<input aria-label={`${clip.id} VR De-Noise showNoise`} type="checkbox" checked={effect.parameters.showNoise??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,showNoise:event.currentTarget.checked}}))}/></label>
       <div class="channels">
        {#each channels as [channel,label,min,max,base]}
         {@const animation=effect.animation}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} step="0.1" aria-label={`${clip.id} VR De-Noise ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} VR De-Noise ${channel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} VR De-Noise ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{return {...current,animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}
           <details><summary>Уровень шума · {animation.track.keyframes.length} ключей</summary>
            {#each animation.track.keyframes as key (key.tick)}
             <div class="row">
              <label>Время, с<input aria-label={`${clip.id} VR De-Noise ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick/animation.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{tick:Math.round(Number(event.currentTarget.value)*animation.track.timeBase)}))} /></label>
              <label>Значение<input aria-label={`${clip.id} VR De-Noise ${key.tick} value`} type="number" {min} {max} step="0.1" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
              <button class="btn ghost sm" aria-label={`${clip.id} VR De-Noise ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,null))}>Удалить ключ</button>
             </div>
            {/each}
           </details>
          {/if}
         </div>
        {/each}
       </div>
       <small>0 — исходное изображение. Поле задаёт базовое значение и значение нового ключа.</small>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'vr_denoise',parameters:{}}))}>Сбросить VR De-Noise</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить VR De-Noise</button>
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
 fieldset > label { display:flex; align-items:center; gap:6px; }
</style>
