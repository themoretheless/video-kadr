<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type AlphaAdjust = Extract<WireVideoEffect, { kind: 'alpha_adjust' }>
  type Channel='opacity'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[['opacity','Непрозрачность',0,100,100]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: AlphaAdjust) => AlphaAdjust): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'alpha_adjust' ? change(effect) : effect))
  }
 function keyframe(effect:AlphaAdjust,channel:Channel,base:number,start:number):AlphaAdjust {
  const tick=nestedComposition.timeUs-start
  if(!Number.isSafeInteger(tick) || tick<0)throw new Error('Курсор должен находиться после начала клипа')
  const previous=effect.animation
  if(previous?.mode==='keyframes' && previous.track.timeBase!==1000000)throw new Error('Шкала канала должна быть в микросекундах')
  const points=previous?.mode==='keyframes'?previous.track.keyframes:[]
  const keyframes=[...points.filter(p=>p.tick!==tick),{...points.find(p=>p.tick===tick),tick,value:effect.parameters[channel] ?? base}].sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:{mode:'keyframes',track:previous?.mode==='keyframes'?{...previous.track,keyframes}:{timeBase:1000000,interpolation:'linear',keyframes}}}
 }
 function editKey(effect:AlphaAdjust,tick:number,patch:{tick?:number;value?:number}|null):AlphaAdjust {
  const previous=effect.animation
  if(previous?.mode!=='keyframes')return effect
  const keyframes=previous.track.keyframes.flatMap(key=>key.tick===tick?patch?[{...key,...patch}]:[]:[key]).sort((a,b)=>a.tick-b.tick)
  return {...effect,animation:keyframes.length?{...previous,track:{...previous.track,keyframes}}:undefined}
 }
</script>
<section aria-label="Alpha Adjust">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'alpha_adjust', parameters: {} }])}>Добавить Alpha Adjust</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'alpha_adjust'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Alpha Adjust {index + 1}</legend>
              <label><input aria-label={`${clip.id} Alpha Adjust ignore`} type="checkbox" checked={effect.parameters.ignore ?? false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,ignore:event.currentTarget.checked}}))} />Игнорировать альфу</label>
              <label><input aria-label={`${clip.id} Alpha Adjust invert`} type="checkbox" checked={effect.parameters.invert ?? false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,invert:event.currentTarget.checked}}))} />Инвертировать альфу</label>
              <label><input aria-label={`${clip.id} Alpha Adjust maskOnly`} type="checkbox" checked={effect.parameters.maskOnly ?? false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,maskOnly:event.currentTarget.checked}}))} />Показать маску</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Alpha Adjust ${channel}`} type="number" {min} {max} step="0.1" value={effect.parameters[channel] ?? base} onchange={event => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, [channel]: Number(event.currentTarget.value) } }))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Alpha Adjust ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Alpha Adjust ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { return { ...current, animation: undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Alpha Adjust ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Alpha Adjust ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Alpha Adjust ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'alpha_adjust', parameters: {} }))}>Сбросить Alpha Adjust</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Alpha Adjust</button></div>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  section, fieldset { display: grid; gap: 6px; }
  fieldset { border: 0; padding: 0; margin: 0; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  label { display: flex; gap: 6px; align-items: center; }
  input[type=number] { width: 5.5rem; }
</style>
