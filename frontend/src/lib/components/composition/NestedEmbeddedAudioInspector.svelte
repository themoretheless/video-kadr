<script lang="ts">
 import AudioDspChain from './AudioDspChain.svelte'
 import {nestedComposition,nestedHistory,editNestedEmbeddedAudio} from '$lib/state/nestedComposition.svelte'
 import type {WireVideoClip} from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 let selected=$state('')
 let id=$derived(nestedComposition.activeCompositionId??nestedComposition.document?.root??'')
 let entries=$derived((nestedComposition.document?.compositions[id]?.content.tracks??[]).flatMap(track=>track.kind==='video'?track.clips.map(clip=>({track,clip})):[]))
 let current=$derived(entries.find(entry=>entry.clip.id===selected)??entries[0])
 function edit(change:(clip:WireVideoClip)=>Partial<Pick<WireVideoClip,'dspEffects'|'independentAudio'|'sourceAudioEnabled'|'audioGain'|'audioPan'|'audioFadeInTicks'|'audioFadeOutTicks'>>){const target=current,composition=id,document=nestedComposition.document;if(!target)return;void editNestedEmbeddedAudio(composition,target.track.id,target.clip.id,change,()=>{if(nestedComposition.document!==document||id!==composition||current?.clip.id!==target.clip.id)throw new Error('Выбранный видеоклип изменился')})}
</script>
{#if current}
 <details><summary>Встроенный звук видео</summary>
  <fieldset disabled={disabled||nestedHistory.busy||current.track.locked}>
   <label>Клип <select aria-label="Видеоклип для DSP" value={current.clip.id} onchange={event=>{selected=event.currentTarget.value}}>{#each entries as entry(entry.clip.id)}<option value={entry.clip.id}>{entry.track.name} · {entry.clip.id}</option>{/each}</select></label>
   <label><input type="checkbox" aria-label="Включить встроенный звук" checked={current.clip.sourceAudioEnabled} disabled={!nestedComposition.document?.compositions[id]?.content.sources[current.clip.sourceId]?.hasAudio||current.clip.playbackMode?.mode==='freeze'} onchange={event=>{const enabled=event.currentTarget.checked;edit(()=>({sourceAudioEnabled:enabled}))}} />Встроенный звук</label>
   <label><input type="checkbox" aria-label="Независимый микс видеоклипа" disabled={(!!current.clip.dspEffects?.length||!!current.clip.concatGainKeys?.length)||!!current.clip.audioFadeInTicks||!!current.clip.audioFadeOutTicks} checked={(current.clip.independentAudio??false)||(!!current.clip.dspEffects?.length||!!current.clip.concatGainKeys?.length)||!!current.clip.audioFadeInTicks||!!current.clip.audioFadeOutTicks} onchange={event=>{const enabled=event.currentTarget.checked;edit(()=>({independentAudio:enabled}))}} />Микшировать звук этого видеослоя</label>
   {#each [['audioFadeInTicks','Нарастание звука'],['audioFadeOutTicks','Затухание звука']] as [field,label]}
    <label>{label}, с <input aria-label={label} type="number" min="0" max="86400" step="0.01" value={(current.clip[field as 'audioFadeInTicks'|'audioFadeOutTicks']??0)/1000000} onchange={event=>{const ticks=Math.round(Number(event.currentTarget.value)*1000000);if(Number.isSafeInteger(ticks)&&ticks>=0)edit(()=>({[field]:ticks,independentAudio:true}))}} /></label>
   {/each}
   <AudioDspChain effects={current.clip.dspEffects??[]} localTick={nestedComposition.timeUs-current.clip.placement.timelineStartTick} onchange={dspEffects=>edit(()=>({dspEffects,independentAudio:true}))} />
  </fieldset>
 </details>
{/if}
<style>fieldset{border:0;padding:0.5rem 0}label{display:flex;gap:0.5rem;align-items:center;margin-bottom:0.5rem}</style>
