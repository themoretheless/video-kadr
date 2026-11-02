<script lang="ts">
 import {getCompositionFonts,replaceCompositionFonts,type CompositionFontUsage} from '$lib/api'
 import {buildCompositionWireDocument} from '$lib/composition/payload'
 import {applyFontReplacement} from '$lib/composition/fontReplacement'
 import type {Composition} from '$lib/composition/types'
 let {composition,onchange}:{composition:Composition;onchange?:(document:Composition)=>void}=$props()
 let selected=$state(''),family=$state(''),style=$state(''),busy=$state(false),status=$state('')
 async function replace(){
  if(busy||!onchange||!family.trim())return
  const from=fonts.find(font=>`${font.family}/${font.style}`===selected);if(!from)return
  const snapshot=JSON.stringify(composition);busy=true;error='';status=''
  try{
   const result=await replaceCompositionFonts(buildCompositionWireDocument(composition),{family:from.family,style:from.style},{family:family.trim(),...(style.trim()?{style:style.trim()}:{})})
   if(JSON.stringify(composition)!==snapshot)throw new Error('Композиция изменилась; повторите замену')
   onchange(applyFontReplacement(composition,result.composition));status=`Заменено: ${result.replaced}`
  }catch(reason){error=String(reason)}finally{busy=false}
 }
 let open=$state(false),fonts=$state<readonly CompositionFontUsage[]>([]),error=$state(''),loading=$state(false)
 $effect(()=>{
  if(!open)return
  const snapshot=composition,controller=new AbortController()
  fonts=[];error='';loading=true
  try {
   const wire=buildCompositionWireDocument(snapshot)
   void getCompositionFonts(wire,controller.signal).then(result=>{if(!controller.signal.aborted)fonts=result})
    .catch(reason=>{if(!controller.signal.aborted)error=String(reason)})
    .finally(()=>{if(!controller.signal.aborted)loading=false})
  } catch(reason) {error=String(reason);loading=false}
  return ()=>controller.abort()
 })
</script>
<details bind:open><summary>Шрифты композиции</summary>
 {#if loading}<p role="status">Загрузка…</p>
 {:else if error}<p role="alert">{error}</p>
 {:else if fonts.length===0}<p role="status">Шрифты не используются</p>
 {:else}<ul>{#each fonts as font (`${font.family}/${font.style}`)}<li>{font.family} · {font.style} <span>×{font.count}</span>{#if font.missing} <strong class="missing">Не найден</strong>{/if}</li>{/each}</ul>{/if}
 {#if onchange&&fonts.length>0}<fieldset disabled={busy}>
  <label>Заменить<select aria-label="Исходный шрифт" value={selected} onchange={event=>selected=event.currentTarget.value}><option value="">Выберите шрифт</option>{#each fonts as font}<option value={`${font.family}/${font.style}`}>{font.family} · {font.style}</option>{/each}</select></label>
  <label>Семейство<input aria-label="Новое семейство шрифта" value={family} oninput={event=>family=event.currentTarget.value} /></label>
  <label>Начертание<input aria-label="Новое начертание шрифта" placeholder="Сохранить текущее" value={style} oninput={event=>style=event.currentTarget.value} /></label>
  <button type="button" class="btn ghost sm" disabled={!selected||!family.trim()} onclick={()=>void replace()}>Заменить шрифт</button>
 </fieldset>{/if}
 {#if status}<p role="status">{status}</p>{/if}
</details>
<style>details{font-size:.75rem}summary{cursor:pointer}ul{padding-left:1rem;margin:.4rem 0}li{overflow-wrap:anywhere}span{opacity:.65}.missing{color:var(--warn);font-weight:normal}</style>
