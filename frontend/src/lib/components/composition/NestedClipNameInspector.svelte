<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type ClipName = Extract<WireVideoEffect, { kind: 'clip_name' }>
  type Channel='positionX'|'positionY'|'size'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[['positionX','Позиция X',undefined,undefined,0],['positionY','Позиция Y',undefined,undefined,0],['size','Размер, %',1,100,15]]
  function value(effect:ClipName,channel:Channel,base:number):number{
    if(channel==='positionX'||channel==='positionY'){
      const axis=channel==='positionX'?0:1
      return effect.parameters.position?.[axis]??base
    }
    return effect.parameters[channel as 'size']??base
  }
  function displayed(effect:ClipName,channel:Channel,base:number):number|undefined{
    if(channel==='positionX'||channel==='positionY')return effect.parameters.position?.[channel==='positionX'?0:1]??undefined
    return value(effect,channel,base)
  }
  function setValue(effect:ClipName,channel:Channel,number:number|null):ClipName{
    if(channel==='positionX'||channel==='positionY'){
      const position:[number|null,number|null]=[...(effect.parameters.position??[null,null])]
      position[channel==='positionX'?0:1]=number
      return {...effect,parameters:{...effect.parameters,position}}
    }
    return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}
  }


  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: ClipName) => ClipName): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'clip_name' ? change(effect) : effect))
  }
  function keyframe(effect: ClipName, channel: Channel, base: number, start: number): ClipName {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: ClipName, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): ClipName {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Clip Name">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'clip_name', parameters: {} }])}>Добавить Clip Name</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'clip_name'}
            {@const source=nestedComposition.document?.compositions[compositionId]?.content.sources?.[clip.sourceId]}
            {@const canvas=nestedComposition.document?.compositions[compositionId]?.content.canvas}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Clip Name {index + 1}</legend>

              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Clip Name ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={displayed(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Clip Name ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, channel==='positionX'?(source?.width??canvas?.width??0)*0.5:channel==='positionY'?(source?.height??canvas?.height??0)*0.5:base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Clip Name ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Clip Name ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Clip Name ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Clip Name ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'clip_name', parameters: {} }))}>Сбросить Clip Name</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Clip Name</button></div>
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
