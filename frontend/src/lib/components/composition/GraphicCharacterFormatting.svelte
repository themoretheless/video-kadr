<script lang="ts">
 import {editGraphicTextRuns,type GraphicCharacterStyle,type GraphicTextEditOperation,type GraphicTextParameters} from '$lib/api'
 type Runs=NonNullable<GraphicTextParameters['runs']>
 let {text,runs,label,disabled=false,onchange}:{text:string;runs:Runs;label:string;disabled?:boolean;onchange:(runs:Runs)=>void|Promise<void>}=$props()
 let start=$state(0)
 let end=$state<number|undefined>(undefined)
 let style=$state<GraphicCharacterStyle>({})
 let busy=$state(false)
 let error=$state('')
 const numeric=[['size','Размер'],['tracking','Трекинг'],['baselineShift','Смещение базовой линии']] as const
 const flags=[['fauxBold','Полужирный'],['fauxItalic','Курсив'],['underline','Подчёркивание']] as const
 let characters=$derived(Array.from(text).length)
 async function command(kind:'apply'|'clear') {
  const finish=end??characters
  if(!Number.isSafeInteger(start)||!Number.isSafeInteger(finish)||start<0||finish<start||finish>characters){error='Укажите диапазон символов внутри текста';return}
  const originalText=text,originalRuns=JSON.stringify(runs)
  busy=true;error=''
  try {
   const operation:GraphicTextEditOperation=kind==='apply'?{kind,start,end:finish,style}:{kind,start,end:finish}
   const next=await editGraphicTextRuns(originalText,runs,operation)
   if(text!==originalText||JSON.stringify(runs)!==originalRuns)throw new Error('Текст изменился во время форматирования; повторите действие')
   await onchange(next)
  }catch(failure){error=String(failure)}finally{busy=false}
 }
</script>
<details>
 <summary>Форматирование символов · {runs.length} диапазонов</summary>
 <fieldset disabled={disabled||busy}>
  <div class="row"><label>От символа<input type="number" min="0" max={characters} step="1" aria-label={`${label} characters start`} bind:value={start}/></label><label>До символа<input type="number" min="0" max={characters} step="1" placeholder={String(characters)} aria-label={`${label} characters end`} bind:value={end}/></label></div>
  <p>Диапазон от нуля; последний символ не включён. Пустые поля наследуют стиль слоя.</p>
  {#each [['font','Шрифт'],['fontStyle','Начертание']] as [field,title]}
   <label>{title}<input aria-label={`${label} characters ${field}`} value={style[field as 'font'|'fontStyle']??''} onchange={event=>style={...style,[field]:event.currentTarget.value||undefined}}/></label>
  {/each}
  {#each numeric as [field,title]}<label>{title}<input type="number" step="0.1" aria-label={`${label} characters ${field}`} value={style[field]} onchange={event=>style={...style,[field]:event.currentTarget.value===''?undefined:Number(event.currentTarget.value)}}/></label>{/each}
  {#each flags as [field,title]}<label>{title}<select aria-label={`${label} characters ${field}`} value={style[field]===undefined?'':String(style[field])} onchange={event=>style={...style,[field]:event.currentTarget.value===''?undefined:event.currentTarget.value==='true'}}><option value="">Наследовать</option><option value="true">Включить</option><option value="false">Выключить</option></select></label>{/each}
  <label>Регистр<select aria-label={`${label} characters caps`} value={style.caps===undefined?'':String(style.caps)} onchange={event=>style={...style,caps:event.currentTarget.value===''?undefined:Number(event.currentTarget.value)}}><option value="">Наследовать</option><option value="0">Обычный</option><option value="1">Прописные</option><option value="2">Капитель</option></select></label>
  <label><input type="checkbox" aria-label={`${label} characters custom fill`} checked={style.fill!==undefined} onchange={event=>style={...style,fill:event.currentTarget.checked?[1,1,1,1]:undefined}}/>Цвет символов</label>
  {#if style.fill}<div class="row">{#each ['R','G','B','A'] as axis,i}<label>{axis}<input type="number" min="0" max="1" step="0.01" aria-label={`${label} characters fill ${axis}`} value={style.fill[i]} onchange={event=>{const fill:[number,number,number,number]=[...style.fill!];fill[i]=Number(event.currentTarget.value);style={...style,fill}}}/></label>{/each}</div>{/if}
  <div class="row"><button class="btn ghost sm" onclick={()=>command('apply')}>Применить к символам</button><button class="btn ghost sm" onclick={()=>command('clear')}>Очистить стиль символов</button></div>
 </fieldset>
 {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
 fieldset{border:0;padding:0;display:grid;gap:6px}label,.row{display:flex;align-items:center;gap:6px}.row{flex-wrap:wrap}input[type=number]{width:5.5rem}p{font-size:.8rem;opacity:.75;margin:4px 0}
</style>
