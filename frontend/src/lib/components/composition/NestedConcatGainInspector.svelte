<script lang="ts">
 import ConcatGainSourceKeyEditor from './ConcatGainSourceKeyEditor.svelte'
 import {nestedComposition,nestedHistory,editNestedSourceGainKeys} from '$lib/state/nestedComposition.svelte'
 import {nativeClipEnd} from '$lib/composition/nativeClipInsert'
 import type {WireVideoClip,WireAudioClip,WireConcatParamKey} from '$lib/composition/types'
 type Entry={trackId:string;name:string;locked:boolean;clip:WireVideoClip|WireAudioClip}
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap<Entry>(track=>track.kind==='audio'||track.kind==='video'?track.clips.map(clip=>({trackId:track.id,name:track.name,locked:track.locked,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 let gain=$derived(current?('audioGain'in current.clip?current.clip.audioGain:current.clip.gain):undefined)
 function edit(keys:readonly WireConcatParamKey[]){const target=current,composition=id,document=nestedComposition.document;if(!target)return;void editNestedSourceGainKeys(composition,target.trackId,target.clip.id,keys,()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный клип изменился')})}
</script>
{#if current}
 <details><summary>Анимация громкости клипа</summary>
  <label>Клип <select aria-label="Клип для ключей громкости" value={current.clip.id} disabled={disabled||nestedHistory.busy} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.name} · {entry.clip.id}</option>{/each}</select></label>
  {#if gain?.mode==='keyframes'}<p>Сначала отключите анимацию громкости по времени.</p>{/if}
  <ConcatGainSourceKeyEditor keys={current.clip.concatGainKeys} rest={gain?.mode==='constant'?gain.value:1} progress={(nestedComposition.timeUs-current.clip.placement.timelineStartTick)/(nativeClipEnd(current.clip)-current.clip.placement.timelineStartTick)} disabled={disabled||nestedHistory.busy||current.locked||gain?.mode==='keyframes'} onchange={edit} />
 </details>
{/if}
