<script lang="ts">
 import type {CompositionBezierInterpolation,CompositionElasticInterpolation} from '$lib/composition/types'
 let {curve,label,disabled=false,onchange}:{curve:CompositionBezierInterpolation|CompositionElasticInterpolation;label:string;disabled?:boolean;onchange:(curve:CompositionBezierInterpolation|CompositionElasticInterpolation)=>void}=$props()
</script>
<fieldset {disabled}>
 {#if curve.kind==='bezier'}
  {#each ['p1','p2'] as control}
   {#each [0,1] as axis}
    <label>{control} {axis===0?'X':'Y'}<input type="number" step="0.01" aria-label={`${label} ${control} ${axis}`} value={curve[control as 'p1'|'p2'][axis]} onchange={event=>{if(curve.kind!=='bezier')return;const point:[number,number]=[...curve[control as 'p1'|'p2']];point[axis]=Number(event.currentTarget.value);onchange({...curve,[control]:point})}} /></label>
   {/each}
  {/each}
 {:else}
  <label>Направление<select aria-label={`${label} direction`} value={curve.dir} onchange={event=>{if(curve.kind==='elastic')onchange({...curve,dir:event.currentTarget.value as 'in'|'out'|'in_out'})}}><option value="in">В начале</option><option value="out">В конце</option><option value="in_out">С двух сторон</option></select></label>
  {#each ['amplitude','period'] as parameter}
   <label>{parameter==='amplitude'?'Амплитуда':'Период'}<input type="number" step="0.01" aria-label={`${label} ${parameter}`} value={curve[parameter as 'amplitude'|'period']} onchange={event=>{if(curve.kind==='elastic')onchange({...curve,[parameter]:Number(event.currentTarget.value)})}} /></label>
  {/each}
 {/if}
</fieldset>
<style>fieldset{border:0;padding:0;display:flex;gap:0.5rem;flex-wrap:wrap}input[type=number]{width:4.5rem}</style>
