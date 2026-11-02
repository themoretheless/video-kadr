<script lang="ts">
 import type {CompositionAnimatableValue} from '$lib/composition/types'
 import {resolvedKeyframeTangents,editKeyframeTangent,setKeyframeTangentMode,updateKeyframeTangents} from '$lib/composition/tangents'
 type Props = {animation:CompositionAnimatableValue;tick:number;label:string} & (
  | {onchange:(animation:CompositionAnimatableValue)=>void;onintent?:never}
  | {onintent:(intent:import('$lib/composition/tangents').KeyframeTangentIntent)=>void;onchange?:never}
 )
 let {animation,tick,label,onchange,onintent}:Props=$props()
 const tangents=$derived(animation.mode==='keyframes'?resolvedKeyframeTangents(animation.track.keyframes,animation.track.keyframes.findIndex(key=>key.tick===tick),animation.track.interpolation):null)
 let error=$state('')
 function commit(change:()=>CompositionAnimatableValue,intent:import('$lib/composition/tangents').KeyframeTangentIntent){
  try{const next=change();if(onintent)onintent(intent);else onchange?.(next);error=''}catch(reason){error=String(reason)}
 }
 function mode(value:'auto'|'free'){commit(()=>setKeyframeTangentMode(animation,[tick],value),{kind:'mode',mode:value})}
 function continuity(value:'smooth'|'broken'){commit(()=>updateKeyframeTangents(animation,[tick],{continuity:value}),{kind:'continuity',continuity:value})}
 function point(side:'incoming'|'outgoing',axis:'x'|'y',value:number){
  if(!tangents)return
  const x=axis==='x'?value:tangents[side].x,y=axis==='y'?value:tangents[side].y
  commit(()=>editKeyframeTangent(animation,tick,side,x,y),{kind:'point',side,x,y})
 }
</script>
{#if tangents}
 <details>
  <summary>Касательные</summary>
  <label>Режим<select aria-label={`${label} tangent mode`} value={tangents.incoming.mode===tangents.outgoing.mode?tangents.incoming.mode:'mixed'} onchange={event=>mode(event.currentTarget.value as 'auto'|'free')}>
   {#if tangents.incoming.mode!==tangents.outgoing.mode}<option value="mixed" disabled>Смешанный</option>{/if}<option value="auto">Auto</option><option value="free">Free</option>
  </select></label>
  <label>Связь<select aria-label={`${label} tangent continuity`} value={tangents.continuity} onchange={event=>continuity(event.currentTarget.value as 'smooth'|'broken')}><option value="smooth">Smooth</option><option value="broken">Broken</option></select></label>
  {#each ['incoming','outgoing'] as side (side)}
   <div class="point">
    {#each ['x','y'] as axis (axis)}
     <label>{side} {axis}<input aria-label={`${label} tangent ${side} ${axis}`} type="number" step="0.01" min={axis==='x'?0:undefined} max={axis==='x'?1:undefined} value={tangents[side as 'incoming'|'outgoing'][axis as 'x'|'y']} onchange={event=>point(side as 'incoming'|'outgoing',axis as 'x'|'y',Number(event.currentTarget.value))}/></label>
    {/each}
   </div>
  {/each}
  {#if error}<p role="alert">{error}</p>{/if}
 </details>
{/if}
<style>details{grid-column:1/-1}summary{cursor:pointer;font-size:.8rem}.point{display:grid;grid-template-columns:1fr 1fr;gap:.3rem}label{display:flex;flex-direction:column;min-width:0;font-size:.8rem}input,select{min-width:0;max-width:100%}</style>
