<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type TintChannel='amount'
 type ColorChannel='blackR'|'blackG'|'blackB'|'blackA'|'whiteR'|'whiteG'|'whiteB'|'whiteA'
 type Tint=Extract<WireVideoEffect,{kind:'color_tint'}>
 const channels:readonly (readonly [TintChannel,string,number,number,number])[]=[
 ['amount','Количество',0,100,100]]
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:Tint)=>Tint):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='color_tint'?change(effect):effect))
 }
 function keyframe(effect:Tint,channel:TintChannel,base:number,start:number):Tint {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }
 function editKey(effect:Tint,tick:number,patch:{tick?:number;value?:number}|null):Tint {
  const previous=effect.animation
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}
 }
 function colorKey(effect:Tint,channel:ColorChannel,value:number,start:number):Tint {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick)||tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.colorAnimation?.[channel]
  if(previous?.mode==='keyframes'&&previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value}].sort((a,b)=>a.tick-b.tick)
  return {...effect,colorAnimation:{...effect.colorAnimation,[channel]:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}}
 }
 function colorEditKey(effect:Tint,channel:ColorChannel,tick:number,patch:{tick?:number;value?:number}|null):Tint {
  const previous=effect.colorAnimation?.[channel]
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,colorAnimation:{...effect.colorAnimation,[channel]:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}}
 }
</script>
<section aria-label="Tint">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'color_tint',parameters:{}}])}>Добавить Tint</button></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='color_tint'}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Tint {index+1}</legend>

       {#each ['black','white'] as endpoint}
        {@const color=endpoint==='black' ? effect.parameters.black ?? [0,0,0,1] : effect.parameters.white ?? [1,1,1,1]}
        <div class="row"><span>{endpoint==='black' ? 'Цвет чёрного' : 'Цвет белого'}</span>
         {#each ['R','G','B','A'] as component,componentIndex}
          {@const colorChannel=`${endpoint}${component}` as ColorChannel}
          {@const colorCurve=effect.colorAnimation?.[colorChannel]}
          <label>{component}<input type="number" step="0.01" aria-label={`${clip.id} Tint ${endpoint} ${component}`} value={color[componentIndex]} onchange={event=>edit(track.id,clip.id,index,current=>{const next=[...(endpoint==='black' ? current.parameters.black ?? [0,0,0,1] : current.parameters.white ?? [1,1,1,1])] as [number,number,number,number];next[componentIndex]=Number(event.currentTarget.value);return {...current,parameters:{...current.parameters,[endpoint]:next}}})} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Tint ${colorChannel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>colorKey(current,colorChannel,color[componentIndex],('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ {component}</button>
          {#if colorCurve}
           <button class="btn ghost sm" aria-label={`${clip.id} Tint ${colorChannel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>({...current,colorAnimation:{...current.colorAnimation,[colorChannel]:undefined}}))}>Удалить анимацию {component}</button>
          {/if}
          {#if colorCurve?.mode==='keyframes'}
           <details><summary>{component} · {colorCurve.track.keyframes.length} ключей</summary>
            {#each colorCurve.track.keyframes as key (key.tick)}
             <div class="row">
              <label>Время, с<input aria-label={`${clip.id} Tint ${colorChannel} ${key.tick} time`} type="number" step="0.001" value={key.tick/colorCurve.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{tick:Math.round(Number(event.currentTarget.value)*colorCurve.track.timeBase)}))} /></label>
              <label>Значение<input aria-label={`${clip.id} Tint ${colorChannel} ${key.tick} value`} type="number" step="0.01" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
              <button class="btn ghost sm" aria-label={`${clip.id} Tint ${colorChannel} ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>colorEditKey(current,colorChannel,key.tick,null))}>Удалить ключ</button>
             </div>
            {/each}
           </details>
          {/if}
         {/each}
        </div>
       {/each}
       <div class="channels">
        {#each channels as [channel,label,min,max,base]}
         {@const animation=effect.animation}
         <div class="channel">
          <div class="row"><label>{label} · база <input type="number" {min} {max} step="0.1" aria-label={`${clip.id} Tint ${channel}`} value={effect.parameters[channel] ?? base} onchange={event=>{const value=Number(event.currentTarget.value);edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:value}}))}} /></label>
          <button class="btn ghost sm" aria-label={`${clip.id} Tint ${channel} ключ`} disabled={nestedComposition.timeUs<('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={()=>edit(track.id,clip.id,index,current=>keyframe(current,channel,base,('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ</button>
          {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Tint ${channel} удалить анимацию`} onclick={()=>edit(track.id,clip.id,index,current=>{return {...current,animation:undefined}})}>Удалить анимацию</button>{/if}</div>
          {#if animation?.mode==='keyframes'}
           <details><summary>Количество · {animation.track.keyframes.length} ключей</summary>
            {#each animation.track.keyframes as key (key.tick)}
             <div class="row">
              <label>Время, с<input aria-label={`${clip.id} Tint ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick/animation.track.timeBase} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{tick:Math.round(Number(event.currentTarget.value)*animation.track.timeBase)}))} /></label>
              <label>Значение<input aria-label={`${clip.id} Tint ${key.tick} value`} type="number" {min} {max} step="0.1" value={key.value} onchange={event=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,{value:Number(event.currentTarget.value)}))} /></label>
              <button class="btn ghost sm" aria-label={`${clip.id} Tint ${key.tick} delete`} onclick={()=>edit(track.id,clip.id,index,current=>editKey(current,key.tick,null))}>Удалить ключ</button>
             </div>
            {/each}
           </details>
          {/if}
         </div>
        {/each}
       </div>
       <small>0 — исходное изображение. Поле задаёт базовое значение и значение нового ключа.</small>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'color_tint',parameters:{}}))}>Сбросить Tint</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Tint</button>
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
