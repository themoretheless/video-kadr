<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type LogoCutout = Extract<WireVideoEffect, { kind: 'logo_cutout' }>
  type Channel = 'threshold'|'softness'|'colorRed'|'colorGreen'|'colorBlue'|'colorAlpha'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['threshold','Порог',0,100,5],['softness','Мягкость',0,100,10],['colorRed','Красный',0,1,1],['colorGreen','Зелёный',0,1,1],['colorBlue','Синий',0,1,1],['colorAlpha','Альфа цвета',0,1,1]]
  function colorIndex(channel:Channel):number{return ['colorRed','colorGreen','colorBlue','colorAlpha'].indexOf(channel)}
  function value(effect:LogoCutout,channel:Channel,base:number):number{const i=colorIndex(channel);return i>=0?(effect.parameters.color??[1,1,1,1])[i]!:(effect.parameters[channel as 'threshold'|'softness']??base)}
  function setValue(effect:LogoCutout,channel:Channel,number:number|null):LogoCutout{const v=number??channels.find(item=>item[0]===channel)![4];const i=colorIndex(channel);if(i>=0){const color:[number,number,number,number]=[...(effect.parameters.color??[1,1,1,1])];color[i]=v;return {...effect,parameters:{...effect.parameters,color}}}return {...effect,parameters:{...effect.parameters,[channel]:v}}}

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: LogoCutout) => LogoCutout): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'logo_cutout' ? change(effect) : effect))
  }
  function keyframe(effect: LogoCutout, channel: Channel, base: number, start: number): LogoCutout {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: LogoCutout, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): LogoCutout {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Logo Cutout">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'logo_cutout', parameters: {} }])}>Добавить Logo Cutout</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'logo_cutout'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Logo Cutout {index + 1}</legend>
              <label>Фон<select aria-label={`${clip.id} Logo Cutout background`} value={effect.parameters.background??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,background:Number(event.currentTarget.value) as 0|1|2}}))}>{#each ['Белый','Чёрный','Заданный цвет'] as label,choice}<option value={choice} selected={(effect.parameters.background??0)===choice}>{label}</option>{/each}</select></label>
              <label><input aria-label={`${clip.id} Logo Cutout unmultiply`} type="checkbox" checked={effect.parameters.unmultiply??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,unmultiply:event.currentTarget.checked}}))}/>Удалять кайму</label>
              <label><input aria-label={`${clip.id} Logo Cutout invert`} type="checkbox" checked={effect.parameters.invert??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,invert:event.currentTarget.checked}}))}/>Инверсия</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Logo Cutout ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Logo Cutout ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Logo Cutout ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Logo Cutout ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Logo Cutout ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Logo Cutout ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'logo_cutout', parameters: {} }))}>Сбросить Logo Cutout</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Logo Cutout</button></div>
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
