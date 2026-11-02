<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type VrColorGradients = Extract<WireVideoEffect, { kind: 'vr_color_gradients' }>
  type Channel='c1R'|'c1G'|'c1B'|'c1A'|'c2R'|'c2G'|'c2B'|'c2A'|'c3R'|'c3G'|'c3B'|'c3A'|'p1Lon'|'p1Lat'|'p2Lon'|'p2Lat'|'p3Lon'|'p3Lat'|'blend'|'opacity'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[["c1R", "c1R", 0, 1, 1], ["c1G", "c1G", 0, 1, 0.25], ["c1B", "c1B", 0, 1, 0.25], ["c1A", "c1A", 0, 1, 1], ["c2R", "c2R", 0, 1, 0.25], ["c2G", "c2G", 0, 1, 1], ["c2B", "c2B", 0, 1, 0.4], ["c2A", "c2A", 0, 1, 1], ["c3R", "c3R", 0, 1, 0.3], ["c3G", "c3G", 0, 1, 0.4], ["c3B", "c3B", 0, 1, 1], ["c3A", "c3A", 0, 1, 1], ["p1Lon", "p1Lon", -180, 180, -120], ["p1Lat", "p1Lat", -90, 90, 20], ["p2Lon", "p2Lon", -180, 180, 0], ["p2Lat", "p2Lat", -90, 90, -20], ["p3Lon", "p3Lon", -180, 180, 120], ["p3Lat", "p3Lat", -90, 90, 20], ["blend", "blend", 1, 100, 50], ["opacity", "opacity", 0, 100, 100]]
  const colors:readonly (readonly [number,number,number,number])[]=[[1, 0.25, 0.25, 1], [0.25, 1, 0.4, 1], [0.3, 0.4, 1, 1]]
  function value(effect:VrColorGradients,channel:Channel,base:number):number{if(channel.startsWith('c')){const color=channel.slice(0,2) as 'c1'|'c2'|'c3';return (effect.parameters[color]??colors[Number(channel[1])-1])['RGBA'.indexOf(channel[2])]??base}return effect.parameters[channel as 'blend']??base}
  function setValue(effect:VrColorGradients,channel:Channel,number:number|null):VrColorGradients{const v=number??channels.find(([name])=>name===channel)![4];if(channel.startsWith('c')){const name=channel.slice(0,2) as 'c1'|'c2'|'c3';const color:[number,number,number,number]=[...(effect.parameters[name]??colors[Number(channel[1])-1])];color['RGBA'.indexOf(channel[2])]=v;return {...effect,parameters:{...effect.parameters,[name]:color}}}return {...effect,parameters:{...effect.parameters,[channel]:v}}}
  const choices=[['frameLayout','Размещение кадров',0,['Monoscopic','Over/Under','Side by Side']],['mode','Режим смешивания',0,['Normal','Add','Screen','Multiply','Overlay']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: VrColorGradients) => VrColorGradients): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'vr_color_gradients' ? change(effect) : effect))
  }
  function keyframe(effect: VrColorGradients, channel: Channel, base: number, start: number): VrColorGradients {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: VrColorGradients, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): VrColorGradients {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="VR Color Gradients">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'vr_color_gradients', parameters: {} }])}>Добавить VR Color Gradients</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'vr_color_gradients'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>VR Color Gradients {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} VR Color Gradients ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} VR Color Gradients ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} VR Color Gradients ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} VR Color Gradients ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} VR Color Gradients ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} VR Color Gradients ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} VR Color Gradients ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'vr_color_gradients', parameters: {} }))}>Сбросить VR Color Gradients</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить VR Color Gradients</button></div>
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
