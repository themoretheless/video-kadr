<script lang="ts">
 import {nestedComposition,nestedHistory,reattachNestedConcatAudio} from '$lib/state/nestedComposition.svelte'
 import {reattachNativeConcatAudio} from '$lib/composition/concatDetachedAudio'
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let tracks=$derived(nestedComposition.document?.compositions[id]?.content.tracks??[])
 let origins=$derived(new Set(tracks.flatMap(track=>track.kind==='audio'?track.clips.flatMap(clip=>clip.detachedFrom?[clip.detachedFrom]:[]):[])))
 let choices=$derived(tracks.flatMap(track=>track.clips.filter(clip=>origins.has(clip.id)||(track.kind==='audio'&&'detachedFrom'in clip&&!!clip.detachedFrom)).map(clip=>({id:clip.id,label:`${track.name} · ${clip.id}`}))))
 let current=$derived(choices.find(choice=>choice.id===selected)??choices[0])
 let reason=$derived.by(()=>{if(!current||!nestedComposition.document)return '';try{reattachNativeConcatAudio(nestedComposition.document,id,current.id);return ''}catch(error){return error instanceof Error?error.message:String(error)}})
</script>
{#if current}
 <details><summary>Отделённый звук Concat</summary>
  <label>Клип <select aria-label="Отделённый звук Concat" value={current.id} disabled={disabled||nestedHistory.busy} onchange={event=>{selected=event.currentTarget.value}}>{#each choices as choice(choice.id)}<option value={choice.id}>{choice.label}</option>{/each}</select></label>
  <p>Звуковые фрагменты будут удалены. Эффекты первого исходного фрагмента вернутся в видео; громкость и границы видео сохранятся.</p>
  <button type="button" disabled={disabled||nestedHistory.busy||!!reason} title={reason||undefined} onclick={()=>void reattachNestedConcatAudio(id,current!.id)}>Вернуть отделённый звук в видео</button>
  {#if reason}<p>{reason}</p>{/if}
 </details>
{/if}
