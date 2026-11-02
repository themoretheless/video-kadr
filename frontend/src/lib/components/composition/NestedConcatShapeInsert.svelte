<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,insertNestedConcatShape} from '$lib/state/nestedComposition.svelte'
 import type {WireConcatShapeStyle} from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 let kind=$state<NonNullable<WireConcatShapeStyle['kind']>>('square'),duration=$state(3),trackId=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let tracks=$derived(nestedComposition.document?.compositions[id]?.content.tracks.filter(track=>track.kind==='graphic')??[])
 const figures=[['square','Квадрат'],['circle','Круг'],['triangle','Треугольник'],['parallelogram','Параллелограмм'],['trapezoid','Трапеция'],['line','Линия'],['arrow','Стрелка']] as const
 async function insert(){const document=nestedComposition.document,composition=id;await insertNestedConcatShape(composition,{kind,durationUs:Math.round(duration*1000000),startUs:nestedComposition.timeUs,trackId:trackId.startsWith('@')?'':trackId,...(trackId==='@free'?{trackPlacement:'first_free' as const}:trackId==='@above'?{trackPlacement:'first_free_above' as const}:{}),newTrackId:`shapes-${crypto.randomUUID()}`,clipId:`shape-${crypto.randomUUID()}`},()=>{if(nestedComposition.document!==document||id!==composition)throw new Error('Композиция изменилась')})}
</script>
<details><summary>Добавить фигуру Concat</summary>
 <fieldset disabled={disabled||nestedHistory.busy||Boolean(agentSession.lock)}>
  <label>Вид<select aria-label="Новая фигура Concat" bind:value={kind}>{#each figures as [value,label](value)}<option {value}>{label}</option>{/each}</select></label>
  <label>Длительность, с<input aria-label="Длительность новой фигуры Concat" type="number" min="0.001" step="0.1" bind:value={duration} /></label>
  <label>Дорожка<select aria-label="Дорожка новой фигуры Concat" bind:value={trackId}><option value="">Новая верхняя дорожка</option><option value="@free">Первая свободная дорожка</option><option value="@above">Свободная дорожка выше клипов</option>{#each tracks as track(track.id)}<option value={track.id} disabled={track.locked}>{track.name}</option>{/each}</select></label>
  <button class="btn ghost sm" disabled={!Number.isFinite(duration)||duration<=0} onclick={()=>void insert()}>Добавить фигуру на курсор</button>
 </fieldset>
</details>
<style>fieldset{border:0;padding:0.5rem 0;display:grid;gap:0.5rem}label{display:flex;gap:0.5rem;align-items:center}</style>
