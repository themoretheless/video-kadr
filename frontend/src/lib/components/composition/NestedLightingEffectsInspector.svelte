<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type LightingEffects = Extract<WireVideoEffect, { kind: 'lighting_effects' }>
  import type {WireLightingChannel} from '$lib/composition/types'
  import {lightingChannels as channels,lightingLights,lightingValue,setLightingValue} from '$lib/composition/lightingEffects'
  type Channel=WireLightingChannel
  function center(sourceId:string):readonly [number,number]{const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[sourceId];return [(source?.width??0)/2,(source?.height??0)/2]}
  function value(effect:LightingEffects,channel:Channel,base:number,sourceId:string):number{return lightingValue(effect,channel,base,center(sourceId))}
  function setValue(effect:LightingEffects,channel:Channel,number:number|null):LightingEffects{return setLightingValue(effect,channel,number)}

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: LightingEffects) => LightingEffects): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'lighting_effects' ? change(effect) : effect))
  }
  function keyframe(effect: LightingEffects, channel: Channel, base: number, start: number,sourceId:string): LightingEffects {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base,sourceId) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: LightingEffects, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): LightingEffects {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Lighting Effects">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'lighting_effects', parameters: {} }])}>Добавить Lighting Effects</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'lighting_effects'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Lighting Effects {index + 1}</legend>
              {#each lightingLights(effect) as light,lightIndex}
                <label>Light {lightIndex+1}<select aria-label={`${clip.id} Lighting Effects l${lightIndex+1}Type`} value={light.kind??0} onchange={event=>edit(track.id,clip.id,index,current=>{const lights=lightingLights(current);lights[lightIndex]={...lights[lightIndex],kind:Number(event.currentTarget.value) as 0|1|2|3};return {...current,parameters:{...current.parameters,lights}}})}>{#each ['None','Directional','Omni','Spotlight'] as name,kind}<option value={kind} selected={(light.kind??0)===kind}>{name}</option>{/each}</select></label>
              {/each}
              <label><input aria-label={`${clip.id} Lighting Effects whiteHigh`} type="checkbox" checked={effect.parameters.whiteHigh??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,whiteHigh:event.currentTarget.checked}}))}/>White Is High</label>
              {#each [0,1,2,3,4,5] as group}
              <details open={group===0}><summary>{group===5?'Surface':`Light ${group+1}`}</summary>
              {#each channels.filter(c=>group===5?!/^l[1-5]/.test(c[0]):c[0].startsWith(`l${group+1}`)) as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Lighting Effects ${channel}`} type="number" {min} {max} step="any" placeholder="Авто" value={value(effect, channel, base,clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Lighting Effects ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick),clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Lighting Effects ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Lighting Effects ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Lighting Effects ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Lighting Effects ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              </details>
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'lighting_effects', parameters: {} }))}>Сбросить Lighting Effects</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Lighting Effects</button></div>
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
