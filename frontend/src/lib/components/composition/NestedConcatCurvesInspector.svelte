<script lang="ts">
 import {SvelteSet} from 'svelte/reactivity'
 import {agentSession} from '$lib/agent/session.svelte'
 import {concatEffectInspectorTracks} from '$lib/composition/concatEffectInspectorContext'
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import {sampleConcatCurve}from'$lib/api'
  import{curvePreviewProject,type CurveChannel}from'$lib/composition/concatCurveAuthoring'
  import{nestedEffectPreview,clearEffectPreview}from'$lib/state/nestedEffectPreview.svelte'
  import type{CurvePointDraft,CurveGesture}from'./CurvePointGraph.svelte'
  import type{WireVideoEffect}from'$lib/composition/types'
  import ToneCurveChannel from './ToneCurveChannel.svelte'
  let { disabled = false,token='',kind='concat_curves',embedded=false }: { disabled?: boolean;token?:string;kind?:'concat_curves'|'concat_adjust';embedded?:boolean } = $props()
  const title=$derived(kind==='concat_adjust'?'Concat Adjust':'Concat Curves')
  function matches(effect:WireVideoEffect):effect is Extract<WireVideoEffect,{kind:'concat_curves'|'concat_adjust'}>{return effect.kind===kind}
  const previewOwner=Symbol('concat-curve-preview'),pending=new SvelteSet<object>()
  let previewChannel:CurveChannel|undefined
  const colors={luma:'var(--text)',red:'#ee8d8d',green:'#8bc997',blue:'#8fabff'}
  let channel=$state<'luma'|'red'|'green'|'blue'>('luma')
  const emptyPoints:readonly(readonly[number,number])[]=[]
  const labels={luma:'Яркость (luma)',red:'Красный',green:'Зелёный',blue:'Синий'}
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks=$derived(concatEffectInspectorTracks(nestedComposition.document,compositionId))
  function edit(track:string,clip:string,update:(effects:readonly WireVideoEffect[])=>readonly WireVideoEffect[]){const base=nestedComposition.document,session=token,composition=compositionId,mode=kind;return editNestedVideoEffects(composition,track,clip,update,()=>{if(base!==nestedComposition.document||session!==token||composition!==compositionId||kind!==mode)throw new Error('Контекст цветокоррекции изменился')})}
  async function change(track:string,clip:string,index:number,points:readonly(readonly[number,number])[],meta?:CurveGesture):Promise<void>{
    const currentChannel=channel,base=nestedComposition.document,time=nestedComposition.timeUs
    if(!base||meta&&(meta.scope!==base||meta.clock!==time||meta.session!==token||meta.context!==`${compositionId}:${kind}:${currentChannel}`))return
    const gesture=meta?.gesture??{};pending.add(gesture)
    try{const project=curvePreviewProject(base,compositionId,track,clip,index,currentChannel,points,kind);previewChannel=currentChannel;nestedEffectPreview.value={owner:previewOwner,gesture,base,project,timeUs:time,compositionId,token};await edit(track,clip,effects=>effects.map((effect,i)=>i===index&&matches(effect)?{...effect,parameters:{...effect.parameters,[currentChannel]:points}}:effect))}catch(reason){if(base===nestedComposition.document)nestedHistory.error=reason instanceof Error?reason.message:String(reason)}finally{pending.delete(gesture);clearEffectPreview(previewOwner,gesture)}
  }
  function preview(track:string,clip:string,index:number,draft:CurvePointDraft|null,gesture:object){
    if(!draft){if(!pending.has(gesture))clearEffectPreview(previewOwner,gesture);return}
    const base=nestedComposition.document;if(!base||draft.scope!==base||draft.clock!==nestedComposition.timeUs||draft.session!==token||draft.context!==`${compositionId}:${kind}:${channel}`)return
    try{const project=curvePreviewProject(base,compositionId,track,clip,index,channel,draft.points,kind);previewChannel=channel;nestedEffectPreview.value={owner:previewOwner,gesture,base,project,timeUs:draft.clock,compositionId,token}}catch{clearEffectPreview(previewOwner,gesture)}
  }
  $effect(()=>{const d=nestedEffectPreview.value;if(d?.owner===previewOwner&&(disabled||d.base!==nestedComposition.document||d.timeUs!==nestedComposition.timeUs||d.token!==token||d.compositionId!==compositionId||previewChannel!==channel))clearEffectPreview(previewOwner)})
  $effect(()=>()=>clearEffectPreview(previewOwner))

</script>
<section aria-label={title}>
  {#each tracks as track (track.id)}
    {#if track.kind==='video'||track.kind==='image'||track.kind==='text'||track.kind==='graphic'}
      {#each track.clips as clip (clip.id)}
        {#if !embedded}<div class="clip-row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onclick={() => void edit(track.id, clip.id, (effects) => [...effects, { kind, parameters: {} }])}>Добавить {title}</button></div>{/if}
        {#each (clip.effects ?? []) as effect, index (index)}
          {#if matches(effect)}
            <fieldset disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)}>
              <legend>{title} {index + 1} · кривые</legend>
              <label>Канал <select aria-label={`${clip.id} канал кривой`} value={channel} onchange={(event) => { const next = event.currentTarget.value; if (next === 'luma' || next === 'red' || next === 'green' || next === 'blue') channel = next }}>{#each Object.entries(labels) as [key, label] (key)}<option value={key}>{label}</option>{/each}</select></label>
              <ToneCurveChannel interactive scope={nestedComposition.document} clock={nestedComposition.timeUs} session={token} context={`${compositionId}:${kind}:${channel}`} tint={colors[channel]} references={(Object.keys(colors)as CurveChannel[]).filter(key=>key!==channel).map(key=>({points:effect.parameters[key]??emptyPoints,tint:colors[key]}))} onpreview={(draft,gesture)=>preview(track.id,clip.id,index,draft,gesture)} maxPoints={8} sampleCurve={sampleConcatCurve} points={effect.parameters[channel] ?? emptyPoints} label={labels[channel]} disabled={disabled||nestedHistory.busy||track.locked||Boolean(agentSession.lock)} onchange={(points,meta)=>change(track.id,clip.id,index,points,meta)} />
              {#if !embedded}<button class="btn ghost sm" onclick={() => void edit(track.id, clip.id, (effects) => effects.filter((_, i) => i !== index))}>Удалить {title}</button>{/if}
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  .clip-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  fieldset { min-width:0; border: 0; padding: 0.5rem 0; margin: 0; }
</style>
