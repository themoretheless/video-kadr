<script lang="ts">
 import NestedConcatTitleTextEditor from './NestedConcatTitleTextEditor.svelte'
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,editNestedConcatTitle} from '$lib/state/nestedComposition.svelte'
 import type {WireConcatTitleStyle} from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap(track=>track.kind==='text'?track.clips.filter(clip=>clip.concatTitle).map(clip=>({track,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 const numbers=[
  ['fontSize','Размер относительно высоты',0.09,0.01,1,0.01],['fontWeight','Насыщенность',700,100,900,1],
  ['opacity','Непрозрачность титра',1,0,1,0.01],['strokeWidth','Толщина обводки',0,0,undefined,0.001],
  ['backgroundRadius','Радиус фона',0.0135,0,undefined,0.001],['backgroundPaddingX','Отступ фона X',0.0315,0,undefined,0.001],
  ['backgroundPaddingY','Отступ фона Y',0.018,0,undefined,0.001],['lineHeight','Межстрочный интервал',1.2,0.5,undefined,0.1],
  ['tracking','Межбуквенный интервал',0,undefined,undefined,0.001],['maxWidth','Ширина блока',0,0,undefined,0.01],['maxHeight','Высота блока',0,0,undefined,0.01],
 ] as const
 function edit(patch:Partial<WireConcatTitleStyle>){
  const target=current,composition=id,document=nestedComposition.document
  if(!target)return
  void editNestedConcatTitle(composition,target.track.id,target.clip.id,patch,()=>{
   if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный титр изменился')
  })
 }
</script>
{#if current}
 <details><summary>Оформление титра Concat</summary>
  <fieldset disabled={disabled||nestedHistory.busy||current.track.locked||Boolean(agentSession.lock)}>
   <label>Титр<select aria-label="Титр Concat" value={current.clip.id} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.text.slice(0,40)}</option>{/each}</select></label>
   <NestedConcatTitleTextEditor compositionId={id} trackId={current.track.id} clip={current.clip} disabled={disabled||current.track.locked} />
   <label>Шрифт<input aria-label="Concat fontFamily" value={current.clip.concatTitle?.fontFamily??'"Cabinet Grotesk"'} onchange={event=>edit({fontFamily:event.currentTarget.value})} /></label>
   {#each numbers as [field,label,base,min,max,step] (field)}
    <label>{label}<input aria-label={`Concat ${field}`} type="number" {min} {max} {step} value={current.clip.concatTitle?.[field]??base} onchange={event=>{if(event.currentTarget.value!==''&&Number.isFinite(event.currentTarget.valueAsNumber))edit({[field]:event.currentTarget.valueAsNumber})}} /></label>
   {/each}
   <label><input aria-label="Concat italic" type="checkbox" checked={current.clip.concatTitle?.italic??false} onchange={event=>edit({italic:event.currentTarget.checked})} />Курсив</label>
   <label><input aria-label="Concat shadow" type="checkbox" checked={current.clip.concatTitle?.shadow??true} onchange={event=>edit({shadow:event.currentTarget.checked})} />Тень</label>
   <label>Выравнивание<select aria-label="Concat align" value={current.clip.concatTitle?.align??'center'} onchange={event=>edit({align:event.currentTarget.value as 'left'|'center'|'right'})}><option value="left">Слева</option><option value="center">По центру</option><option value="right">Справа</option></select></label>
   {#each [['color','Цвет текста','#ffffff'],['strokeColor','Цвет обводки','#000000'],['background','Цвет фона','']] as const as [field,label,base] (field)}
    <label>{label}<input aria-label={`Concat ${field}`} value={current.clip.concatTitle?.[field]??base} placeholder={field==='background'?'Без фона':'#RRGGBBAA'} onchange={event=>edit({[field]:event.currentTarget.value})} /></label>
   {/each}
   <p>Ширина блока задаётся в долях ширины кадра; размер шрифта, обводка, отступы и высота блока — в долях высоты кадра. Нулевая ширина или высота блока — автоматическая. Пустой цвет фона отключает фон.</p>
  </fieldset>
 </details>
{/if}
<style>fieldset{border:0;padding:0.5rem 0;display:grid;gap:0.5rem}label{display:flex;gap:0.5rem;align-items:center}input[type=number]{width:7rem}</style>
