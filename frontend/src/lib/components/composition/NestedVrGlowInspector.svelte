<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type VrGlow = Extract<WireVideoEffect, { kind: 'vr_glow' }>
  type Channel='threshold'|'radius'|'brightness'|'saturation'|'tintR'|'tintG'|'tintB'|'tintA'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[["threshold", "Порог, %", 0, 100, 70], ["radius", "Радиус", 0, 1000, 30], ["brightness", "Яркость", 0, 10, 1], ["saturation", "Насыщенность", 0, 4, 1], ["tintR", "Tint R", 0, 1, 1], ["tintG", "Tint G", 0, 1, 1], ["tintB", "Tint B", 0, 1, 1], ["tintA", "Tint A", 0, 1, 1]]
  function value(effect:VrGlow,channel:Channel,base:number):number{if(channel.startsWith('tint'))return (effect.parameters.tint??[1,1,1,1])['RGBA'.indexOf(channel[4])]??base;return effect.parameters[channel as 'threshold']??base}
  function setValue(effect:VrGlow,channel:Channel,number:number|null):VrGlow{const v=number??channels.find(([name])=>name===channel)![4];if(channel.startsWith('tint')){const tint:[number,number,number,number]=[...(effect.parameters.tint??[1,1,1,1])];tint['RGBA'.indexOf(channel[4])]=v;return {...effect,parameters:{...effect.parameters,tint}}}return {...effect,parameters:{...effect.parameters,[channel]:v}}}
  const choices=[['frameLayout','Размещение кадров',0,['Monoscopic','Over/Under','Side by Side']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: VrGlow) => VrGlow): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'vr_glow' ? change(effect) : effect))
  }
  function keyframe(effect: VrGlow, channel: Channel, base: number, start: number): VrGlow {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: VrGlow, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): VrGlow {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="VR Glow">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'vr_glow', parameters: {} }])}>Добавить VR Glow</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'vr_glow'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>VR Glow {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} VR Glow ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} VR Glow ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} VR Glow ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} VR Glow ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} VR Glow ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} VR Glow ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} VR Glow ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <label>Использовать Tint<input aria-label={`${clip.id} VR Glow useTint`} type="checkbox" checked={effect.parameters.useTint??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,useTint:event.currentTarget.checked}}))}/></label>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'vr_glow', parameters: {} }))}>Сбросить VR Glow</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить VR Glow</button></div>
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
