<script lang="ts">
 import {nestedComposition,nestedHistory,editNestedVideoEffects} from '$lib/state/nestedComposition.svelte'
 let {disabled=false}:{disabled?:boolean}=$props()
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
</script>
<section aria-label="Black & White">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row">
     <span>{track.name} · {clip.id}</span>
     <button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>[...effects,{kind:'black_white'}])}>Добавить Black &amp; White</button>
     {#each (clip.effects ?? []) as effect,index}
      {#if effect.kind==='black_white'}
       <button class="btn ghost sm" aria-label={`${clip.id} Black & White ${index} delete`} disabled={disabled || nestedHistory.busy || track.locked} onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить Black &amp; White {index+1}</button>
      {/if}
     {/each}
    </div>
   {/each}
  {/if}
 {/each}
</section>
<style>.row{display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap}</style>
