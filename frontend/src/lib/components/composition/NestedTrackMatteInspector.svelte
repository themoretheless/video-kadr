<script lang="ts">
 import { nestedComposition,nestedHistory,editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireVideoEffect } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 type TrackMatte=Extract<WireVideoEffect,{kind:'track_matte'}>
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 function edit(track:string,clip:string,index:number,change:(effect:TrackMatte)=>TrackMatte):void {
  void editNestedVideoEffects(compositionId,track,clip,effects=>effects.map((effect,i)=>i===index && effect.kind==='track_matte'?change(effect):effect))
 }
</script>
<section aria-label="Track Matte Key">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'track_matte',parameters:{}}])}>Добавить Track Matte Key</button></div>
    {#each (clip.effects ?? []) as wire,index}
     {#if wire.kind==='track_matte'}
      {@const effect=wire}
      <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
       <legend>Track Matte Key {index+1}</legend>
       <label>Маска<select aria-label={`${clip.id} Track Matte matte`} value={String(effect.parameters.matte??0)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,matte:Number(event.currentTarget.value)}}))}>
        <option value="0">Нет</option>{#each [1,2,3,4] as number}<option value={String(number)}>Video {number}</option>{/each}
       </select></label>
       <label>Канал<select aria-label={`${clip.id} Track Matte composite`} value={String(effect.parameters.composite??0)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,composite:Number(event.currentTarget.value)}}))}><option value="0">Альфа</option><option value="1">Яркость</option></select></label>
       <label><input type="checkbox" aria-label={`${clip.id} Track Matte reverse`} checked={effect.parameters.reverse??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,reverse:event.currentTarget.checked}}))} />Инвертировать маску</label>
       <button class="btn ghost sm" onclick={()=>edit(track.id,clip.id,index,()=>({kind:'track_matte',parameters:{}}))}>Сбросить Track Matte Key</button>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Track Matte Key</button>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
</section>
<style>
 section{display:grid;gap:.4rem}fieldset{display:flex;flex-wrap:wrap;align-items:center;gap:.4rem;border:0;padding:.25rem 0}legend{font-size:.8rem}label{display:flex;align-items:center;gap:.25rem;font-size:.8rem}.row{display:flex;align-items:center;gap:.4rem}select{max-width:10rem}
</style>
