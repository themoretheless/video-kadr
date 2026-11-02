<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type LightLeaks = Extract<WireVideoEffect, { kind: 'light_leaks' }>
  type Channel='intensity'|'scale'|'direction'|'speed'|'seed'|'c1Red'|'c1Green'|'c1Blue'|'c1Alpha'|'c2Red'|'c2Green'|'c2Blue'|'c2Alpha'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[["intensity", "Интенсивность, %", 0, 100, 70], ["scale", "Масштаб, %", 10, 1000, 100], ["direction", "Направление, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 30], ["speed", "Скорость", 0, 20, 1], ["seed", "Seed", 0, 10000, 0], ["c1Red", "Цвет 1 · Red", 0, 1, 1], ["c1Green", "Цвет 1 · Green", 0, 1, 0.45], ["c1Blue", "Цвет 1 · Blue", 0, 1, 0.1], ["c1Alpha", "Цвет 1 · Alpha", 0, 1, 1], ["c2Red", "Цвет 2 · Red", 0, 1, 1], ["c2Green", "Цвет 2 · Green", 0, 1, 0.15], ["c2Blue", "Цвет 2 · Blue", 0, 1, 0.35], ["c2Alpha", "Цвет 2 · Alpha", 0, 1, 1]]
  const colors={c1:[1,0.45,0.1,1],c2:[1,0.15,0.35,1]} as const
  function colorChannel(channel:Channel):{color:'c1'|'c2';index:number}|null {return channel.startsWith('c')?{color:channel.slice(0,2) as 'c1'|'c2',index:['Red','Green','Blue','Alpha'].indexOf(channel.slice(2))}:null}
  function value(effect:LightLeaks,channel:Channel,base:number):number{const c=colorChannel(channel);return c?(effect.parameters[c.color]??colors[c.color])[c.index]??base:effect.parameters[channel as 'intensity'|'scale'|'direction'|'speed'|'seed']??base}
  function setValue(effect:LightLeaks,channel:Channel,number:number|null):LightLeaks{const v=number??channels.find(([name])=>name===channel)![4];const c=colorChannel(channel);if(!c)return {...effect,parameters:{...effect.parameters,[channel]:v}};const color:[number,number,number,number]=[...(effect.parameters[c.color]??colors[c.color])];color[c.index]=v;return {...effect,parameters:{...effect.parameters,[c.color]:color}}}
  const choices=[['mode','Смешивание',0,['Screen','Add','Overlay']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: LightLeaks) => LightLeaks): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'light_leaks' ? change(effect) : effect))
  }
  function keyframe(effect: LightLeaks, channel: Channel, base: number, start: number): LightLeaks {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: LightLeaks, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): LightLeaks {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Light Leaks">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'light_leaks', parameters: {} }])}>Добавить Light Leaks</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'light_leaks'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Light Leaks {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Light Leaks ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Light Leaks ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Light Leaks ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Light Leaks ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Light Leaks ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Light Leaks ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} Light Leaks ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'light_leaks', parameters: {} }))}>Сбросить Light Leaks</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Light Leaks</button></div>
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
