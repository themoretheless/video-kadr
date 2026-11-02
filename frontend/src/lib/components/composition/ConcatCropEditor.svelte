<script lang="ts">
 import type {WireConcatPreparation} from '$lib/composition/types'
 import type {ConcatPreparationEdit} from '$lib/composition/concatPreparation'
 let {preparation,disabled=false,onchange}:{preparation:WireConcatPreparation;disabled?:boolean;onchange:(edit:ConcatPreparationEdit)=>void}=$props()
 const edges=['слева','сверху','справа','снизу']
</script>
<fieldset {disabled}>
 <legend>Обрезка Concat</legend>
 <p>Доля исходного изображения. Оставшаяся часть подгоняется в кадр до применения эффектов.</p>
 {#each edges as edge,index}
  <label>{edge}<input aria-label={`Обрезка Concat ${edge}`} type="number" min="0" max="0.9" step="0.001" value={preparation.crop[index]} onchange={event=>onchange({kind:'edge',index,value:event.currentTarget.valueAsNumber})}/></label>
 {/each}
 <label><input aria-label="Concat отражение по горизонтали" type="checkbox" checked={preparation.flipH??false} onchange={event=>onchange({kind:'flip',axis:'horizontal',value:event.currentTarget.checked})}/>По горизонтали</label>
 <label><input aria-label="Concat отражение по вертикали" type="checkbox" checked={preparation.flipV??false} onchange={event=>onchange({kind:'flip',axis:'vertical',value:event.currentTarget.checked})}/>По вертикали</label>
 <button type="button" onclick={()=>onchange({kind:'reset-crop'})}>Сбросить обрезку</button>
</fieldset>
<style>label{display:flex;gap:0.5rem;margin:0.3rem 0}input[type=number]{width:6rem}p{font-size:0.85rem}</style>
