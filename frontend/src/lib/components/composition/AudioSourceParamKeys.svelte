<script lang="ts">
 import type {WireConcatParamKey} from '$lib/composition/types'
 import {sourceEaseName,sourceEasePoints,sourceEasePresets} from '$lib/composition/concatRadiusSourceKeys'
 import {sampleConcatSourceRun} from '$lib/composition/concatSourceWindow'
 let {keys=[],rest=0,label,onchange,description="Сохраняются в проекте. В аудиодвижке Concat эти ключи не меняют звук: слышно постоянное значение параметра."}:{keys?:readonly WireConcatParamKey[];rest?:number;label:string;description?:string;onchange:(keys:readonly WireConcatParamKey[])=>void}=$props()
 let at=$state(0.5),error=$state('')
 function commit(next:readonly WireConcatParamKey[]){
  if(next.length>4096||next.some(key=>!Number.isFinite(key.at)||key.at<0||key.at>1||!Number.isFinite(key.value)||Array.isArray(key.ease)&&(key.ease.length!==4||!key.ease.every(Number.isFinite)||key.ease[0]!<0||key.ease[0]!>1||key.ease[2]!<0||key.ease[2]!>1))){error='Недопустимые исходные ключи';return}
  onchange(next.map(key=>({...key,...(Array.isArray(key.ease)?{ease:[...key.ease] as [number,number,number,number]}:{})})));error=''
 }
 function patch(index:number,change:Partial<WireConcatParamKey>){commit(keys.map((key,i)=>i===index?{...key,...change,...(change.value!==undefined&&key.derived?{derived:false}:{})}:key))}
 function add(){
  if(!Number.isFinite(at)||at<0||at>1){error='Положение должно быть от 0 до 1';return}
  let nearest=-1,distance=Infinity;keys.forEach((key,i)=>{const d=Math.abs(key.at-at);if(d<=0.002&&d<distance){nearest=i;distance=d}})
  const key={at,value:keys.length?sampleConcatSourceRun(keys,at):rest}
  commit((nearest<0?[...keys,key]:keys.map((previous,i)=>i===nearest?{...previous,...key}:previous)).sort((a,b)=>a.at-b.at))
 }
</script>
<details>
 <summary>Исходные ключи {label} ({keys.length})</summary>
 <p>{description}</p>
 {#each keys as key,index}
  <div class="source-key">
   <label>Доля клипа <input aria-label={`${label}: ключ ${index+1} положение`} type="number" min="0" max="1" step="0.001" value={key.at} onchange={event=>patch(index,{at:event.currentTarget.valueAsNumber})} /></label>
   <label>Значение <input aria-label={`${label}: ключ ${index+1} значение`} type="number" step="any" value={key.value} onchange={event=>patch(index,{value:event.currentTarget.valueAsNumber})} /></label>
   <label>Easing <select aria-label={`${label}: ключ ${index+1} easing`} value={sourceEaseName(key)} onchange={event=>{const name=event.currentTarget.value;if(name!=='custom')patch(index,{ease:sourceEasePresets[name as keyof typeof sourceEasePresets]})}}><option value="linear">Линейно</option><option value="in">Ускорение</option><option value="out">Замедление</option><option value="inOut">Плавно</option><option value="custom" disabled>Своя кривая</option></select></label>
   {#each ['x1','y1','x2','y2'] as axis,n}
    <label>{axis}<input aria-label={`${label}: ключ ${index+1} ${axis}`} type="number" step="0.01" min={n%2===0?0:undefined} max={n%2===0?1:undefined} value={sourceEasePoints(key)[n]} onchange={event=>{const ease=[...sourceEasePoints(key)] as [number,number,number,number];ease[n]=event.currentTarget.valueAsNumber;patch(index,{ease})}} /></label>
   {/each}
   <button type="button" aria-label={`${label}: удалить ключ ${index+1}`} onclick={()=>commit(keys.filter((_,i)=>i!==index))}>Удалить</button>
  </div>
 {/each}
 <label>Положение нового ключа <input aria-label={`${label}: новый ключ положение`} type="number" min="0" max="1" step="0.001" bind:value={at} /></label>
 <button type="button" aria-label={`${label}: добавить исходный ключ`} disabled={keys.length>=4096} onclick={add}>Добавить исходный ключ</button>
 {#if keys.length}<button type="button" aria-label={`${label}: удалить исходные ключи`} onclick={()=>commit([])}>Удалить все исходные ключи</button>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>.source-key{display:flex;gap:0.4rem;flex-wrap:wrap;margin:0.5rem 0}label{display:flex;gap:0.25rem;align-items:center}input{width:5rem}p{font-size:0.85rem}</style>
