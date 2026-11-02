<script lang="ts">
 import {agentSession} from '$lib/agent/session.svelte'
 import {nestedComposition,nestedHistory,insertNestedConcatTreatment} from '$lib/state/nestedComposition.svelte'
 import {concatVisualPackages} from '$lib/composition/concatVisualPackages'
 let {disabled=false}:{disabled?:boolean}=$props()
 let effectId=$state('concat.mono'),duration=$state(3),trackId=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let tracks=$derived(nestedComposition.document?.compositions[id]?.content.tracks.filter(track=>track.kind==='graphic')??[])
 async function insert(){const document=nestedComposition.document,composition=id;await insertNestedConcatTreatment(composition,{effectId,durationUs:Math.round(duration*1000000),startUs:nestedComposition.timeUs,trackId:trackId.startsWith('@')?'':trackId,...(trackId==='@free'?{trackPlacement:'first_free' as const}:trackId==='@above'?{trackPlacement:'first_free_above' as const}:{}),newTrackId:`adjustment-${crypto.randomUUID()}`,clipId:`adjustment-${crypto.randomUUID()}`},()=>{if(nestedComposition.document!==document||id!==composition)throw new Error('Композиция изменилась')})}
</script>
<details><summary>Добавить корректирующий слой Concat</summary>
 <fieldset disabled={disabled||nestedHistory.busy||Boolean(agentSession.lock)}>
  <label>Эффект<select aria-label="Эффект нового корректирующего слоя Concat" bind:value={effectId}>{#each concatVisualPackages as [value,label](value)}<option {value}>{label}</option>{/each}</select></label>
  <label>Длительность, с<input aria-label="Длительность нового корректирующего слоя Concat" type="number" min="0.001" step="0.1" bind:value={duration} /></label>
  <label>Дорожка<select aria-label="Дорожка нового корректирующего слоя Concat" bind:value={trackId}><option value="">Новая верхняя дорожка</option><option value="@free">Первая свободная дорожка</option><option value="@above">Свободная дорожка выше клипов</option>{#each tracks as track(track.id)}<option value={track.id} disabled={track.locked}>{track.name}</option>{/each}</select></label>
  <button class="btn ghost sm" disabled={!Number.isFinite(duration)||duration<=0} onclick={()=>void insert()}>Добавить коррекцию на курсор</button>
 </fieldset>
</details>
<style>fieldset{border:0;padding:0.5rem 0;display:grid;gap:0.5rem}label{display:flex;gap:0.5rem;align-items:center}</style>
