<script lang="ts">
 import {nestedComposition,nestedHistory,editNestedAudioStream} from '$lib/state/nestedComposition.svelte'
 import SourceAudioStreamEditor from './SourceAudioStreamEditor.svelte'
 import type {WireVideoClip,WireAudioClip} from '$lib/composition/types'
 type Entry={trackId:string;name:string;locked:boolean;clip:WireVideoClip|WireAudioClip}
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap<Entry>(track=>track.kind==='audio'||track.kind==='video'?track.clips.map(clip=>({trackId:track.id,name:track.name,locked:track.locked,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 function edit(stream:number|undefined){
  const target=current,composition=id,document=nestedComposition.document;if(!target)return
  void editNestedAudioStream(composition,target.trackId,target.clip.id,stream,()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный клип изменился')})
 }
</script>
{#if current}
 <details><summary>Аудиодорожка источника</summary>
  <fieldset disabled={disabled||nestedHistory.busy}>
   <label>Клип <select aria-label="Клип для выбора аудиодорожки" value={current.clip.id} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.name} · {entry.clip.id}</option>{/each}</select></label>
   <SourceAudioStreamEditor sourceId={current.clip.sourceId} value={current.clip.audioStream} disabled={disabled||nestedHistory.busy||current.locked} onchange={edit} />
  </fieldset>

 </details>
{/if}
<style>fieldset{border:0;padding:0.5rem 0}label{display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem}</style>
