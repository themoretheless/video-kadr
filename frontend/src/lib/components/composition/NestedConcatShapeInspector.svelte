<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,editNestedConcatShape} from '$lib/state/nestedComposition.svelte'
 import type {WireConcatShapeStyle,CompositionBlendMode} from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap(track=>track.kind==='graphic'?track.clips.filter(clip=>clip.concatShape).map(clip=>({track,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 const figures=[['square','Квадрат'],['circle','Круг'],['triangle','Треугольник'],['parallelogram','Параллелограмм'],['trapezoid','Трапеция'],['line','Линия'],['arrow','Стрелка']] as const
 const blends=[['normal','Обычный'],['multiply','Умножение'],['screen','Экран'],['addition','Сложение'],['lighten','Светлее'],['darken','Темнее']] as const
 function edit(patch:Partial<WireConcatShapeStyle>,blend?:CompositionBlendMode){
  const target=current,composition=id,document=nestedComposition.document;if(!target)return
  void editNestedConcatShape(composition,target.track.id,target.clip.id,patch,blend,()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранная фигура изменилась')})
 }
</script>
{#if current}
 <details><summary>Оформление фигуры Concat</summary>
  <fieldset disabled={disabled||nestedHistory.busy||current.track.locked||Boolean(agentSession.lock)}>
   <label>Клип<select aria-label="Фигура Concat" value={current.clip.id} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.id}</option>{/each}</select></label>
   <label>Вид<select aria-label="Concat shape kind" value={current.clip.concatShape?.kind??'square'} onchange={event=>edit({kind:event.currentTarget.value as WireConcatShapeStyle['kind']})}>{#each figures as [kind,label](kind)}<option value={kind}>{label}</option>{/each}</select></label>
   <label>Размер<input aria-label="Concat shape size" type="number" min="0.01" max="2" step="0.01" value={current.clip.concatShape?.size??0.35} onchange={event=>{if(event.currentTarget.value!==''&&Number.isFinite(event.currentTarget.valueAsNumber))edit({size:event.currentTarget.valueAsNumber})}} /></label>
   <label>Заливка<input aria-label="Concat shape fill" value={current.clip.concatShape?.fill??'#ffffff'} placeholder="#RRGGBBAA" onchange={event=>edit({fill:event.currentTarget.value})} /></label>
   <label>Обводка<input aria-label="Concat shape stroke" value={current.clip.concatShape?.stroke??''} placeholder="#RRGGBBAA" onchange={event=>edit({stroke:event.currentTarget.value})} /></label>
   <label>Толщина обводки<input aria-label="Concat shape strokeWidth" type="number" min="0" max="0.5" step="0.001" value={current.clip.concatShape?.strokeWidth??0.012} onchange={event=>{if(event.currentTarget.value!==''&&Number.isFinite(event.currentTarget.valueAsNumber))edit({strokeWidth:event.currentTarget.valueAsNumber})}} /></label>
   <label>Смешивание<select aria-label="Concat shape blend" value={current.clip.concatShapeBlend??'normal'} onchange={event=>edit({},event.currentTarget.value as CompositionBlendMode)}>{#each blends as [mode,label](mode)}<option value={mode}>{label}</option>{/each}</select></label>
   <p>Размер и толщина обводки — доли высоты кадра. Пустая заливка оставляет только обводку; линия и стрелка используют цвет обводки либо заливки.</p>
  </fieldset>
 </details>
{/if}
<style>fieldset{border:0;padding:0.5rem 0;display:grid;gap:0.5rem}label{display:flex;gap:0.5rem;align-items:center}input[type=number]{width:7rem}</style>
