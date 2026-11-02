<script lang="ts">
 import type {CaptionStyle} from '$lib/composition/types'
 let {style={},onchange}:{style?:Partial<CaptionStyle>;onchange:(style:Partial<CaptionStyle>)=>void}=$props()
 function color(value:CaptionStyle['color']|undefined,fallback:string):string{return typeof value==='string'?value:value?'#'+value.map(byte=>byte.toString(16).padStart(2,'0')).join(''):fallback}
 function number(key:'size'|'outline'|'margin'|'lineSpacing',value:string){const parsed=Number(value);if(Number.isFinite(parsed))onchange({...style,[key]:parsed})}
</script>
<details><summary>Стиль субтитров</summary><div class="fields">
 <label>Шрифт<input aria-label="Шрифт субтитров" value={style.font??'Inter'} onchange={event=>onchange({...style,font:event.currentTarget.value})} /></label>
 <label>Размер<input aria-label="Размер субтитров" type="number" min="4" max="400" value={style.size??54} onchange={event=>number('size',event.currentTarget.value)} /></label>
 <label>Цвет<input aria-label="Цвет субтитров" value={color(style.color,'#ffffffff')} onchange={event=>onchange({...style,color:event.currentTarget.value})} /></label>
 <label>Выравнивание<select aria-label="Выравнивание субтитров" value={style.align??'center'} onchange={event=>onchange({...style,align:event.currentTarget.value as CaptionStyle['align']})}><option value="left">Слева</option><option value="center">По центру</option><option value="right">Справа</option></select></label>
 <label>Положение<select aria-label="Положение субтитров" value={style.anchor??'bottom'} onchange={event=>onchange({...style,anchor:event.currentTarget.value as CaptionStyle['anchor']})}><option value="top">Сверху</option><option value="middle">В середине</option><option value="bottom">Снизу</option></select></label>
 <label>Отступ<input aria-label="Отступ субтитров" type="number" min="0" max="0.45" step="0.01" value={style.margin??0.08} onchange={event=>number('margin',event.currentTarget.value)} /></label>
 <label>Межстрочный интервал<input aria-label="Межстрочный интервал субтитров" type="number" min="0.8" max="4" step="0.05" value={style.lineSpacing??1.25} onchange={event=>number('lineSpacing',event.currentTarget.value)} /></label>
 <label class="check"><input aria-label="Фон субтитров" type="checkbox" checked={style.background??true} onchange={event=>onchange({...style,background:event.currentTarget.checked})} />Фон</label>
 <label>Цвет фона<input aria-label="Цвет фона субтитров" value={color(style.backgroundColor,'#000000bf')} onchange={event=>onchange({...style,backgroundColor:event.currentTarget.value})} /></label>
 <label>Обводка<input aria-label="Обводка субтитров" type="number" min="0" max="40" step="0.5" value={style.outline??0} onchange={event=>number('outline',event.currentTarget.value)} /></label>
 <label>Цвет обводки<input aria-label="Цвет обводки субтитров" value={color(style.outlineColor,'#000000ff')} onchange={event=>onchange({...style,outlineColor:event.currentTarget.value})} /></label>
 </div></details>
<style>summary{cursor:pointer}.fields{display:grid;grid-template-columns:1fr 1fr;gap:.35rem;margin:.35rem 0}label{display:grid;gap:.15rem;min-width:0}input,select{width:100%;min-width:0}.check{display:flex;align-items:center}.check input{width:auto}</style>
