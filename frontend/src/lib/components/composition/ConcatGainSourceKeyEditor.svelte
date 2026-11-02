<script lang="ts">
 import SourceKeyRunGraph from './SourceKeyRunGraph.svelte'
 import type {WireConcatParamKey} from '$lib/composition/types'
 import {cloneGainSourceKeys,patchGainSourceKey,upsertGainSourceKey} from '$lib/composition/concatGainSourceKeys'
 import {sampleConcatSourceRun} from '$lib/composition/concatSourceWindow'
 import {sourceEaseName,sourceEasePoints,sourceEasePresets} from '$lib/composition/concatRadiusSourceKeys'
 let {keys=[],rest=1,progress=0,disabled=false,onchange}:{keys?:readonly WireConcatParamKey[];rest?:number;progress?:number;disabled?:boolean;onchange:(keys:readonly WireConcatParamKey[])=>void}=$props()
 let error=$state('')
 function commit(change:()=>readonly WireConcatParamKey[]){try{const next=cloneGainSourceKeys(change());onchange(next);error=''}catch(reason){error=reason instanceof Error?reason.message:String(reason)}}
</script>
<fieldset disabled={disabled}>
 <legend>Ключи громкости Concat</legend>
 <p>Ключи заменяют постоянную громкость. Положение задаётся долей длительности клипа.</p>
 <SourceKeyRunGraph {rest} emptyRange={[0,Math.max(2,Math.min(Number.MAX_VALUE,rest*2))]} {keys} label="Громкость Concat" identity={JSON.stringify(keys)} {disabled} onchange={(index,change,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились во время жеста');return patchGainSourceKey(keys,index,change)})} oninsert={(key,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились');return upsertGainSourceKey(keys,key.at,key.value)})} onremove={(index,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились');return keys.filter((_,at)=>at!==index)})} onbatch={(edits,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились');return cloneGainSourceKeys(keys.map((key,index)=>{const change=edits.find(edit=>edit.index===index)?.change;return {...key,...change,...(change?.value===undefined?{}:{derived:false})}})).sort((a,b)=>a.at-b.at)})} onremoveBatch={(indices,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились');return keys.filter((_,index)=>!indices.includes(index))})} onreplace={(run,expected)=>commit(()=>{if(JSON.stringify(keys)!==expected)throw new Error('Ключи изменились');return run})} />
 {#each keys as key,index (index)}
  <div class="gain-key">
   <label>Положение <input aria-label={`Ключ громкости ${index+1}: положение`} type="number" min="0" max="1" step="0.001" value={key.at} onchange={event=>{const at=Number(event.currentTarget.value);commit(()=>patchGainSourceKey(keys,index,{at}))}} /></label>
   <label>Громкость <input aria-label={`Ключ громкости ${index+1}: значение`} type="number" min="0" step="0.05" value={key.value} onchange={event=>{const value=Number(event.currentTarget.value);commit(()=>patchGainSourceKey(keys,index,{value}))}} /></label>
   <label>Easing <select aria-label={`Ключ громкости ${index+1}: easing`} value={sourceEaseName(key)} onchange={event=>{const name=event.currentTarget.value;if(name!=='custom')commit(()=>patchGainSourceKey(keys,index,{ease:sourceEasePresets[name as keyof typeof sourceEasePresets]}))}}><option value="linear">Линейно</option><option value="in">Ускорение</option><option value="out">Замедление</option><option value="inOut">Плавно</option><option value="custom" disabled>Своя кривая</option></select></label>
   {#each ['x1','y1','x2','y2'] as axis,n (n)}
    <label>{axis}<input aria-label={`Ключ громкости ${index+1}: ${axis}`} type="number" step="0.01" min={n%2===0?0:undefined} max={n%2===0?1:undefined} value={sourceEasePoints(key)[n]} onchange={event=>{const ease=[...sourceEasePoints(key)] as [number,number,number,number];ease[n]=Number(event.currentTarget.value);commit(()=>patchGainSourceKey(keys,index,{ease}))}} /></label>
   {/each}
   <button type="button" aria-label={`Удалить ключ громкости ${index+1}`} onclick={()=>commit(()=>keys.filter((_,i)=>i!==index))}>Удалить</button>
  </div>
 {/each}
 <button type="button" disabled={disabled||keys.length>=4096} onclick={()=>commit(()=>{const at=Math.max(0,Math.min(1,progress));return upsertGainSourceKey(keys,at,Math.max(0,keys.length?sampleConcatSourceRun(keys,at):rest))})}>Добавить ключ громкости в позиции курсора</button>
 {#if keys.length}<button type="button" onclick={()=>commit(()=>[])}>Удалить все ключи громкости</button>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</fieldset>
<style>fieldset{margin:0.5rem 0;border:1px solid var(--border-color,#555);padding:0.5rem}.gain-key{display:flex;gap:0.4rem;flex-wrap:wrap;margin-bottom:0.5rem}label{display:flex;gap:0.25rem;align-items:center}input{width:5.5rem}p{font-size:0.85rem}</style>
