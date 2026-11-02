<script lang="ts">
 import { validateColorLut } from '$lib/api'
 import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
 import type { WireColorLut } from '$lib/composition/types'
 let {disabled=false}:{disabled?:boolean}=$props()
 let importing=$state(false),error=$state('')
 let request:AbortController|undefined
 let active=true
 $effect(()=>{active=true;return ()=>{active=false;request?.abort()}})
 let compositionId=$derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
 let tracks=$derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
 async function load(file:File|undefined, track:string,clip:string):Promise<void> {
  if(!file || importing || disabled || nestedHistory.busy)return
  const composition=compositionId,document=nestedComposition.document
  importing=true;error='';request?.abort();const abort=new AbortController();request=abort
  try {
   if(file.size>16*1024*1024)throw new Error('LUT превышает 16 МБ')
   const ext=file.name.split('.').at(-1)?.toLowerCase()
   if(ext!=='cube' && ext!=='3dl')throw new Error('Выберите .cube или .3dl')
   const lut:WireColorLut={format:ext==='cube'?'cube':'three_dl',text:await file.text(),intensity:100}
   await validateColorLut(lut,abort.signal)
   if(active && !abort.signal.aborted && nestedComposition.document===document && compositionId===composition)await editNestedVideoEffects(composition,track,clip,effects=>[...effects,{kind:'color_lut',lut}])
  }catch(failure){if(active && !abort.signal.aborted)error=failure instanceof Error?failure.message:String(failure)}finally {if(active && request===abort)importing=false}
 }
</script>
<section aria-label="Таблицы цвета LUT">
 {#each tracks as track (track.id)}
  {#if track.kind==='video' || track.kind==='image'}
   {#each track.clips as clip (clip.id)}
    <div class="row"><span>{track.name} · {clip.id}</span><label>Импорт LUT <input type="file" accept=".cube,.3dl" aria-label={`${clip.id} импорт LUT`} disabled={disabled || importing || nestedHistory.busy || track.locked} onchange={event=>{const file=event.currentTarget.files?.[0];event.currentTarget.value='';void load(file,track.id,clip.id)}} /></label></div>
    {#each (clip.effects ?? []) as effect,index}
     {#if effect.kind==='color_lut'}
      <fieldset disabled={disabled || importing || nestedHistory.busy || track.locked}>
       <legend>LUT {index+1} · {effect.lut.format==='cube'?'.cube':'.3dl'}</legend>
       <div class="row"><label>Интенсивность, % <input type="number" min="0" max="200" step="1" aria-label={`${clip.id} LUT ${index+1} интенсивность`} value={effect.lut.intensity ?? 100} onchange={event=>{const intensity=Number(event.currentTarget.value);void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.map((current,i)=>i===index && current.kind==='color_lut'?{...current,lut:{...current.lut,intensity}}:current))}} /></label>
       <button class="btn ghost sm" onclick={()=>void editNestedVideoEffects(compositionId,track.id,clip.id,effects=>effects.filter((_,i)=>i!==index))}>Удалить LUT</button></div>
      </fieldset>
     {/if}
    {/each}
   {/each}
  {/if}
 {/each}
 {#if importing}<p role="status">Проверяю LUT…</p>{/if}
 {#if error}<p role="alert">{error}</p>{/if}
</section>
<style>
 .row {display:flex;align-items:center;gap:0.5rem;flex-wrap:wrap;}
 fieldset {border:0;padding:0.5rem 0;margin:0;}
 input[type=number] {width:4.5rem;} input[type=file] {max-width:18rem;}
</style>
