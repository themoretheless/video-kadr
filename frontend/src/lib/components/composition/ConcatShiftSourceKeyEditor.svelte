<script lang="ts">
 import SourceKeyRunGraph from './SourceKeyRunGraph.svelte'
 import SourceBezierGraph from './SourceBezierGraph.svelte'
 import type {ChromaticEffect} from '$lib/composition/concatChromaticSourceKeys'
 import {patchSourceShiftKey,setSourceShiftKey,removeSourceShiftKey} from '$lib/composition/concatChromaticSourceKeys'
 import {sourceEasePoints,sourceEaseName,sourceEasePresets,type EasePoints} from '$lib/composition/concatRadiusSourceKeys'
 let {effect,clipId,disabled=false,onchange}:{effect:ChromaticEffect;clipId:string;disabled?:boolean;onchange:(change:(current:ChromaticEffect)=>ChromaticEffect)=>void}=$props()
 let at=$state('0.5')
 let radius=$state<string|undefined>(undefined)
 let radiusText=$derived(radius??String(effect.parameters.shift??4))
 let canAdd=$derived(!!at.trim() && Number.isFinite(Number(at)) && Number(at)>=0 && Number(at)<=1 && !!radiusText.trim() && Number.isFinite(Number(radiusText)) && Number(radiusText)>=1 && Number(radiusText)<=24)
 function number(event:Event,apply:(value:number)=>void){const element=event.currentTarget as HTMLInputElement;const value=Number(element.value);if(element.value.trim()&&Number.isFinite(value)&&element.validity.valid)apply(value)}
 function axis(index:number,axis:number,value:number){onchange(current=>{const key=current.concatKeys?.[index];if(!key)throw new Error('Ключ не найден');const ease=[...sourceEasePoints(key)];ease[axis]=value;return patchSourceShiftKey(current,index,{ease:ease as unknown as EasePoints})})}
</script>
<details>
 <summary>Ключи Concat</summary>
 <fieldset disabled={disabled}>
  <SourceKeyRunGraph rest={Number(radiusText)} emptyRange={[1,24]} keys={effect.concatKeys??[]} label={`${clipId} ${effect.kind}`} identity={JSON.stringify(effect.concatKeys)} disabled={disabled} onchange={(index,change,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchSourceShiftKey(current,index,change)})} oninsert={(key,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return setSourceShiftKey(current,{...key,ease:sourceEasePresets.linear})})} onremove={(index,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return removeSourceShiftKey(current,index)})} onbatch={(edits,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return edits.reduce((result,edit)=>patchSourceShiftKey(result,edit.index,edit.change),current)})} onremoveBatch={(indices,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return [...indices].sort((a,b)=>b-a).reduce((result,index)=>removeSourceShiftKey(result,index),current)})} onreplace={(run,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return {...current,concatKeys:run.length?run:undefined}})} />
  {#each effect.concatKeys??[] as key,index (index)}
   <div class="key">
    <label>Доля клипа <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat source ${index} at`} value={key.at} onchange={event=>number(event,value=>onchange(current=>patchSourceShiftKey(current,index,{at:value})))} /></label>
    <label>Сдвиг, px <input type="number" min="1" max="24" step="any" aria-label={`${clipId} Concat source ${index} value`} value={key.value} onchange={event=>number(event,value=>onchange(current=>patchSourceShiftKey(current,index,{value})))} /></label>
    <label>Easing <select aria-label={`${clipId} Concat source ${index} easing`} value={sourceEaseName(key)} onchange={event=>{const name=event.currentTarget.value;if(name!=='custom')onchange(current=>patchSourceShiftKey(current,index,{ease:name}))}}>
     <option value="linear">Линейный</option><option value="in">Ускорение</option><option value="out">Замедление</option><option value="inOut">Плавный</option><option value="custom" disabled>Свой Безье</option>
    </select></label>
    {#each sourceEasePoints(key) as point,axisIndex (axisIndex)}
     <label>{['x₁','y₁','x₂','y₂'][axisIndex]} <input type="number" min={axisIndex%2===0?0:undefined} max={axisIndex%2===0?1:undefined} step="any" aria-label={`${clipId} Concat source ${index} ease ${axisIndex}`} value={point} onchange={event=>number(event,value=>axis(index,axisIndex,value))} /></label>
    {/each}
    <SourceBezierGraph points={sourceEasePoints(key)} label={`${clipId} Concat source ${index}`} identity={JSON.stringify(effect.concatKeys)} disabled={disabled} onchange={(ease,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchSourceShiftKey(current,index,{ease})})} />
    <button class="btn ghost sm" aria-label={`${clipId} Concat source ${index} удалить`} onclick={()=>onchange(current=>removeSourceShiftKey(current,index))}>Удалить ключ</button>
   </div>
  {/each}
  <div class="key">
   <label>Новая доля <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat source new at`} value={at} onchange={event=>{at=event.currentTarget.value}} /></label>
   <label>Сдвиг, px <input type="number" min="1" max="24" step="any" aria-label={`${clipId} Concat source new value`} value={radiusText} onchange={event=>{radius=event.currentTarget.value}} /></label>
   <button class="btn ghost sm" aria-label={`${clipId} Concat source добавить`} disabled={!canAdd} onclick={()=>onchange(current=>setSourceShiftKey(current,{at:Number(at),value:Number(radiusText),ease:sourceEasePresets.linear}))}>Добавить ключ Concat</button>
  </div>
 </fieldset>
</details>
<style>
 .key {display:flex;gap:0.5rem;flex-wrap:wrap;align-items:center;margin:0.5rem 0;}
 fieldset {border:0;padding:0;margin:0;} label {display:flex;gap:0.25rem;align-items:center;} input {width:5rem;}
</style>
