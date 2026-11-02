<script lang="ts">
 import SourceKeyRunGraph from './SourceKeyRunGraph.svelte'
 import SourceBezierGraph from './SourceBezierGraph.svelte'
 import type {SaturationEffect} from '$lib/composition/concatSaturationSourceKeyEditing'
 import {patchSourceSaturationKey,setSourceSaturationKey,removeSourceSaturationKey,sourceEasePoints,sourceEaseName,sourceEasePresets,type EasePoints} from '$lib/composition/concatSaturationSourceKeyEditing'
 let {effect,clipId,disabled=false,onchange}:{effect:SaturationEffect;clipId:string;disabled?:boolean;onchange:(change:(current:SaturationEffect)=>SaturationEffect)=>void}=$props()
 let at=$state('0.5')
 let amount=$state<string|undefined>(undefined)
 let amountText=$derived(amount??String(effect.parameters.amount??0))
 let canAdd=$derived(!!at.trim() && Number.isFinite(Number(at)) && Number(at)>=0 && Number(at)<=1 && !!amountText.trim() && Number.isFinite(Number(amountText)) && Number(amountText)>=-100 && Number(amountText)<=100)
 function number(event:Event,apply:(value:number)=>void){const element=event.currentTarget as HTMLInputElement;const value=Number(element.value);if(element.value.trim()&&Number.isFinite(value)&&element.validity.valid)apply(value)}
 function axis(index:number,axis:number,value:number){onchange(current=>{const key=current.concatKeys?.[index];if(!key)throw new Error('Ключ не найден');const ease=[...sourceEasePoints(key)];ease[axis]=value;return patchSourceSaturationKey(current,index,{ease:ease as unknown as EasePoints})})}
</script>
<details>
 <summary>Ключи Concat</summary>
 <fieldset disabled={disabled||!!effect.animation}>
  {#if effect.animation}<p>Для исходных ключей сначала удалите анимацию по шкале времени.</p>{/if}
  <SourceKeyRunGraph rest={Number(amountText)} emptyRange={[-100,100]} keys={effect.concatKeys??[]} label={`${clipId} ${effect.kind}`} identity={JSON.stringify(effect.concatKeys)} disabled={disabled||!!effect.animation} onchange={(index,change,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchSourceSaturationKey(current,index,change)})} oninsert={(key,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return setSourceSaturationKey(current,{...key,ease:sourceEasePresets.linear})})} onremove={(index,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return removeSourceSaturationKey(current,index)})} onbatch={(edits,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return edits.reduce((result,edit)=>patchSourceSaturationKey(result,edit.index,edit.change),current)})} onremoveBatch={(indices,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return [...indices].sort((a,b)=>b-a).reduce((result,index)=>removeSourceSaturationKey(result,index),current)})} onreplace={(run,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return {...current,concatKeys:run.length?run:undefined}})} />
  {#each effect.concatKeys??[] as key,index (index)}
   <div class="key">
    {#if key.derived}<small>Вычислен при обрезке</small>{/if}
    <label>Доля клипа <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat source ${index} at`} value={key.at} onchange={event=>number(event,value=>onchange(current=>patchSourceSaturationKey(current,index,{at:value})))} /></label>
    <label>Насыщенность <input type="number" min="-100" max="100" step="any" aria-label={`${clipId} Concat source ${index} value`} value={key.value} onchange={event=>number(event,value=>onchange(current=>patchSourceSaturationKey(current,index,{value})))} /></label>
    <label>Easing <select aria-label={`${clipId} Concat source ${index} easing`} value={sourceEaseName(key)} onchange={event=>{const name=event.currentTarget.value;if(name!=='custom')onchange(current=>patchSourceSaturationKey(current,index,{ease:name}))}}>
     <option value="linear">Линейный</option><option value="in">Ускорение</option><option value="out">Замедление</option><option value="inOut">Плавный</option><option value="custom" disabled>Свой Безье</option>
    </select></label>
    {#each sourceEasePoints(key) as point,axisIndex (axisIndex)}
     <label>{['x₁','y₁','x₂','y₂'][axisIndex]} <input type="number" min={axisIndex%2===0?0:undefined} max={axisIndex%2===0?1:undefined} step="any" aria-label={`${clipId} Concat source ${index} ease ${axisIndex}`} value={point} onchange={event=>number(event,value=>axis(index,axisIndex,value))} /></label>
    {/each}
    <SourceBezierGraph points={sourceEasePoints(key)} label={`${clipId} Concat source ${index}`} identity={JSON.stringify(effect.concatKeys)} disabled={disabled||!!effect.animation} onchange={(ease,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchSourceSaturationKey(current,index,{ease})})} />
    <button class="btn ghost sm" aria-label={`${clipId} Concat source ${index} удалить`} onclick={()=>onchange(current=>removeSourceSaturationKey(current,index))}>Удалить ключ</button>
   </div>
  {/each}
  <div class="key">
   <label>Новая доля <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat source new at`} value={at} onchange={event=>{at=event.currentTarget.value}} /></label>
   <label>Насыщенность <input type="number" min="-100" max="100" step="any" aria-label={`${clipId} Concat source new value`} value={amountText} onchange={event=>{amount=event.currentTarget.value}} /></label>
   <button class="btn ghost sm" aria-label={`${clipId} Concat source добавить`} disabled={!canAdd} onclick={()=>onchange(current=>setSourceSaturationKey(current,{at:Number(at),value:Number(amountText),ease:sourceEasePresets.linear}))}>Добавить ключ Concat</button>
  </div>
 </fieldset>
</details>
<style>
 .key {display:flex;gap:0.5rem;flex-wrap:wrap;align-items:center;margin:0.5rem 0;}
 fieldset {border:0;padding:0;margin:0;} label {display:flex;gap:0.25rem;align-items:center;} input {width:5rem;}
</style>
