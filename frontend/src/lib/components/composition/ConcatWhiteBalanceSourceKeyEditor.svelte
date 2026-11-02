<script lang="ts">
 import SourceKeyRunGraph from './SourceKeyRunGraph.svelte'
 import SourceBezierGraph from './SourceBezierGraph.svelte'
 import {patchWhiteBalanceSourceKey,setWhiteBalanceSourceKey,removeWhiteBalanceSourceKey,whiteBalanceKeyChannels,type WhiteBalanceEffect,type WhiteBalanceChannel} from '$lib/composition/concatWhiteBalanceSourceKeys'
 import {sourceEasePoints,sourceEaseName,sourceEasePresets,type EasePoints} from '$lib/composition/concatRadiusSourceKeys'
 let {effect,channel,clipId,disabled=false,onchange}:{effect:WhiteBalanceEffect;channel:WhiteBalanceChannel;clipId:string;disabled?:boolean;onchange:(change:(current:WhiteBalanceEffect)=>WhiteBalanceEffect)=>void}=$props()
 let info=$derived(whiteBalanceKeyChannels[channel])
 let at=$state('0.5')
 let value=$state<string|undefined>(undefined)
 let valueText=$derived(value??String(effect.parameters[channel]??info.base))
 let canAdd=$derived(!!at.trim() && Number.isFinite(Number(at)) && Number(at)>=0 && Number(at)<=1 && !!valueText.trim() && Number.isFinite(Number(valueText)) && Number(valueText)>=info.min && Number(valueText)<=info.max)
 function number(event:Event,apply:(value:number)=>void){const element=event.currentTarget as HTMLInputElement;const value=Number(element.value);if(element.value.trim()&&Number.isFinite(value)&&element.validity.valid)apply(value)}
 function axis(index:number,axis:number,value:number){onchange(current=>{const key=current.concatKeys?.[channel]?.[index];if(!key)throw new Error('Ключ не найден');const ease=[...sourceEasePoints(key)];ease[axis]=value;return patchWhiteBalanceSourceKey(current,channel,index,{ease:ease as unknown as EasePoints})})}
</script>
<details>
 <summary>Ключи Concat · {info.label}</summary>
 <fieldset {disabled}>
  <SourceKeyRunGraph rest={Number(valueText)} emptyRange={[info.min,info.max]} keys={effect.concatKeys?.[channel]??[]} label={`${clipId} ${effect.kind} ${channel}`} identity={JSON.stringify(effect.concatKeys)} disabled={disabled} onchange={(index,change,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchWhiteBalanceSourceKey(current,channel,index,change)})} oninsert={(key,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return setWhiteBalanceSourceKey(current,channel,{...key,ease:sourceEasePresets.linear})})} onremove={(index,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return removeWhiteBalanceSourceKey(current,channel,index)})} onbatch={(edits,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return edits.reduce((result,edit)=>patchWhiteBalanceSourceKey(result,channel,edit.index,edit.change),current)})} onremoveBatch={(indices,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');return [...indices].sort((a,b)=>b-a).reduce((result,index)=>removeWhiteBalanceSourceKey(result,channel,index),current)})} onreplace={(run,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились');const keys={...current.concatKeys};if(run.length)keys[channel]=run;else delete keys[channel];return {...current,concatKeys:Object.keys(keys).length?keys:undefined}})} />
  {#each effect.concatKeys?.[channel]??[] as key,index (index)}
   <div class="key">
    {#if key.derived}<small>Вычислен при обрезке</small>{/if}
    <label>Доля клипа <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat whiteBalance ${channel} ${index} at`} value={key.at} onchange={event=>number(event,value=>onchange(current=>patchWhiteBalanceSourceKey(current,channel,index,{at:value})))} /></label>
    <label>{info.label} <input type="number" min={info.min} max={info.max} step="any" aria-label={`${clipId} Concat whiteBalance ${channel} ${index} value`} value={key.value} onchange={event=>number(event,value=>onchange(current=>patchWhiteBalanceSourceKey(current,channel,index,{value})))} /></label>
    <label>Easing <select aria-label={`${clipId} Concat whiteBalance ${channel} ${index} easing`} value={sourceEaseName(key)} onchange={event=>{const name=event.currentTarget.value;if(name!=='custom')onchange(current=>patchWhiteBalanceSourceKey(current,channel,index,{ease:name}))}}>
     <option value="linear">Линейный</option><option value="in">Ускорение</option><option value="out">Замедление</option><option value="inOut">Плавный</option><option value="custom" disabled>Свой Безье</option>
    </select></label>
    {#each sourceEasePoints(key) as point,axisIndex (axisIndex)}
     <label>{['x₁','y₁','x₂','y₂'][axisIndex]} <input type="number" min={axisIndex%2===0?0:undefined} max={axisIndex%2===0?1:undefined} step="any" aria-label={`${clipId} Concat whiteBalance ${channel} ${index} ease ${axisIndex}`} value={point} onchange={event=>number(event,value=>axis(index,axisIndex,value))} /></label>
    {/each}
    <SourceBezierGraph points={sourceEasePoints(key)} label={`${clipId} Concat whiteBalance ${channel} ${index}`} identity={JSON.stringify(effect.concatKeys)} {disabled} onchange={(ease,expected)=>onchange(current=>{if(JSON.stringify(current.concatKeys)!==expected)throw new Error('Ключи изменились во время жеста');return patchWhiteBalanceSourceKey(current,channel,index,{ease})})} />
    <button class="btn ghost sm" aria-label={`${clipId} Concat whiteBalance ${channel} ${index} удалить`} onclick={()=>onchange(current=>removeWhiteBalanceSourceKey(current,channel,index))}>Удалить ключ</button>
   </div>
  {/each}
  <div class="key">
   <label>Новая доля <input type="number" min="0" max="1" step="any" aria-label={`${clipId} Concat whiteBalance ${channel} new at`} value={at} onchange={event=>{at=event.currentTarget.value}} /></label>
   <label>{info.label} <input type="number" min={info.min} max={info.max} step="any" aria-label={`${clipId} Concat whiteBalance ${channel} new value`} value={valueText} onchange={event=>{value=event.currentTarget.value}} /></label>
   <button class="btn ghost sm" aria-label={`${clipId} Concat whiteBalance ${channel} добавить`} disabled={!canAdd} onclick={()=>onchange(current=>setWhiteBalanceSourceKey(current,channel,{at:Number(at),value:Number(valueText),ease:sourceEasePresets.linear}))}>Добавить ключ Concat</button>
  </div>
 </fieldset>
</details>
<style>
 .key {display:flex;gap:0.5rem;flex-wrap:wrap;align-items:center;margin:0.5rem 0;}
 fieldset {border:0;padding:0;margin:0;} label {display:flex;gap:0.25rem;align-items:center;} input {width:5rem;}
</style>
