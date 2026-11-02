<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type AlphaGlow = Extract<WireVideoEffect, { kind: 'alpha_glow' }>
  type Channel = 'glow'|'brightness'|'startColorR'|'startColorG'|'startColorB'|'startColorA'|'endColorR'|'endColorG'|'endColorB'|'endColorA'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [["glow", "Свечение", 0, 100, 30], ["brightness", "Яркость", 0, 255, 252], ["startColorR", "startColorR", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["startColorG", "startColorG", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["startColorB", "startColorB", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["startColorA", "startColorA", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["endColorR", "endColorR", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["endColorG", "endColorG", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["endColorB", "endColorB", -3.4028234663852886e+38, 3.4028234663852886e+38, 1], ["endColorA", "endColorA", -3.4028234663852886e+38, 3.4028234663852886e+38, 1]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: AlphaGlow) => AlphaGlow): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'alpha_glow' ? change(effect) : effect))
  }
  function value(effect:AlphaGlow,channel:Channel,base:number):number{
    if(channel==='glow'||channel==='brightness')return effect.parameters[channel]??base
    const color=channel.startsWith('start')?effect.parameters.startColor:effect.parameters.endColor
    return color?.['RGBA'.indexOf(channel.slice(-1))]??base
  }
  function setValue(effect:AlphaGlow,channel:Channel,next:number):AlphaGlow{
    if(channel==='glow'||channel==='brightness')return {...effect,parameters:{...effect.parameters,[channel]:next}}
    const field=channel.startsWith('start')?'startColor':'endColor';const color:[number,number,number,number]=[...(effect.parameters[field]??[1,1,1,1])];color['RGBA'.indexOf(channel.slice(-1))]=next;return {...effect,parameters:{...effect.parameters,[field]:color}}
  }
  function keyframe(effect: AlphaGlow, channel: Channel, base: number, start: number): AlphaGlow {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: AlphaGlow, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): AlphaGlow {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="AlphaGlow">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'alpha_glow', parameters: {} }])}>Добавить AlphaGlow</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'alpha_glow'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>AlphaGlow {index + 1}</legend>
              <label><input aria-label={`${clip.id} AlphaGlow useEnd`} type="checkbox" checked={effect.parameters.useEnd??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,useEnd:event.currentTarget.checked}}))} />Использовать конечный цвет</label>
              <label><input aria-label={`${clip.id} AlphaGlow fadeOut`} type="checkbox" checked={effect.parameters.fadeOut??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,fadeOut:event.currentTarget.checked}}))} />Затухание</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} AlphaGlow ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} AlphaGlow ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} AlphaGlow ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} AlphaGlow ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} AlphaGlow ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} AlphaGlow ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'alpha_glow', parameters: {} }))}>Сбросить AlphaGlow</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить AlphaGlow</button></div>
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
