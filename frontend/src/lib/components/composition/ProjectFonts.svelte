<script lang="ts">
 import ProjectFontFiles from './ProjectFontFiles.svelte'
 import {getProjectFonts,type CompositionFontUsage} from '$lib/api'
 import {nestedComposition,nestedHistory,replaceNestedFonts} from '$lib/state/nestedComposition.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 let open=$state(false),fonts=$state<readonly CompositionFontUsage[]>([]),loading=$state(false),error=$state('')
 let selected=$state(''),family=$state(''),style=$state('')
 $effect(()=>{
  const project=nestedComposition.document
  if(!open||!project)return
  const controller=new AbortController();loading=true;error='';fonts=[]
  void getProjectFonts(project,controller.signal).then(result=>{if(!controller.signal.aborted){fonts=result;if(!result.some(font=>`${font.family}/${font.style}`===selected))selected=''}})
   .catch(reason=>{if(!controller.signal.aborted)error=String(reason)})
   .finally(()=>{if(!controller.signal.aborted)loading=false})
  return ()=>controller.abort()
 })
 function replace(){
  const from=fonts.find(font=>`${font.family}/${font.style}`===selected)
  if(!from||!family.trim()||disabled||nestedHistory.busy)return
  void replaceNestedFonts({family:from.family,style:from.style},{family:family.trim(),...(style.trim()?{style:style.trim()}:{})})
 }
</script>
<details bind:open><summary>Шрифты всего проекта</summary>
 <ProjectFontFiles {disabled} />
 {#if loading}<p role="status">Загрузка…</p>
 {:else if error}<p role="alert">{error}</p>
 {:else if !fonts.length}<p role="status">Шрифты не используются</p>
 {:else}<ul>{#each fonts as font (`${font.family}/${font.style}`)}<li>{font.family} · {font.style} ×{font.count}{#if font.missing} <strong class="missing">Не найден</strong>{/if}</li>{/each}</ul>
 <fieldset disabled={disabled||nestedHistory.busy}>
  <label>Заменить<select aria-label="Исходный шрифт проекта" value={selected} onchange={event=>selected=event.currentTarget.value}><option value="">Выберите шрифт</option>{#each fonts as font (`${font.family}/${font.style}`)}<option value={`${font.family}/${font.style}`}>{font.family} · {font.style}</option>{/each}</select></label>
  <label>Семейство<input aria-label="Новое семейство шрифта проекта" value={family} oninput={event=>family=event.currentTarget.value} /></label>
  <label>Начертание<input aria-label="Новое начертание шрифта проекта" value={style} oninput={event=>style=event.currentTarget.value} placeholder="Сохранить текущее" /></label>
  <button class="btn ghost sm" type="button" disabled={!selected||!family.trim()} onclick={replace}>Заменить шрифт во всём проекте</button>
 </fieldset>{/if}
</details>
<style>details{font-size:.75rem}summary{cursor:pointer}ul{margin:.4rem 0;padding-left:1rem}li{overflow-wrap:anywhere}.missing{color:var(--warn);font-weight:normal}</style>
