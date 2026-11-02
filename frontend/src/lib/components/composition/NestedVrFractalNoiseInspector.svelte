<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type VrFractalNoise = Extract<WireVideoEffect, { kind: 'vr_fractal_noise' }>
  type Channel='contrast'|'brightness'|'scale'|'complexity'|'evolution'|'opacity'
  const channels:readonly (readonly [Channel,string,number|undefined,number|undefined,number])[]=[["contrast", "Контраст, %", 0, 1000, 100], ["brightness", "Яркость, %", -200, 200, 0], ["scale", "Масштаб, %", 10, 10000, 100], ["complexity", "Сложность", 1, 10, 6], ["evolution", "Эволюция, °", undefined, undefined, 0], ["opacity", "Непрозрачность, %", 0, 100, 100]]
  function value(effect:VrFractalNoise,channel:Channel,base:number):number{return effect.parameters[channel]??base}
  function setValue(effect:VrFractalNoise,channel:Channel,number:number|null):VrFractalNoise{return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}}
  const choices=[['frameLayout','Размещение кадров',0,['Monoscopic','Over/Under','Side by Side']],['fractalType','Тип шума',0,['Basic','Turbulent Smooth','Turbulent Sharp']],['mode','Режим смешивания',0,['Normal','Add','Screen','Multiply','Overlay']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: VrFractalNoise) => VrFractalNoise): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'vr_fractal_noise' ? change(effect) : effect))
  }
  function keyframe(effect: VrFractalNoise, channel: Channel, base: number, start: number): VrFractalNoise {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: VrFractalNoise, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): VrFractalNoise {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="VR Fractal Noise">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'vr_fractal_noise', parameters: {} }])}>Добавить VR Fractal Noise</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'vr_fractal_noise'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>VR Fractal Noise {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} VR Fractal Noise ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} VR Fractal Noise ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} VR Fractal Noise ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} VR Fractal Noise ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} VR Fractal Noise ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} VR Fractal Noise ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} VR Fractal Noise ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <label>Инверсия<input aria-label={`${clip.id} VR Fractal Noise invert`} type="checkbox" checked={effect.parameters.invert??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,invert:event.currentTarget.checked}}))}/></label>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'vr_fractal_noise', parameters: {} }))}>Сбросить VR Fractal Noise</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить VR Fractal Noise</button></div>
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
