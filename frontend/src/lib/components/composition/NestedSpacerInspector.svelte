<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Spacer = Extract<WireVideoEffect, { kind: 'spacer' }>
  type Channel = 'left'|'top'|'right'|'bottom'|'radius'|'colorR'|'colorG'|'colorB'|'colorA'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['left','Слева, px',0,4000,40],['top','Сверху, px',0,4000,40],['right','Справа, px',0,4000,40],['bottom','Снизу, px',0,4000,40],['radius','Радиус углов, px',0,2000,0],['colorR','Фон R',-3.402823466e38,3.402823466e38,0],['colorG','Фон G',-3.402823466e38,3.402823466e38,0],['colorB','Фон B',-3.402823466e38,3.402823466e38,0],['colorA','Фон A',-3.402823466e38,3.402823466e38,1]]
  function value(effect: Spacer, channel: Channel, base: number): number {
    if (channel==='colorR'||channel==='colorG'||channel==='colorB'||channel==='colorA') return effect.parameters.color?.[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]] ?? base
    return effect.parameters[channel] ?? base
  }
  function setValue(effect: Spacer, channel: Channel, number: number | null, base: number): Spacer {
    if (channel==='colorR'||channel==='colorG'||channel==='colorB'||channel==='colorA') {
      const color: [number,number,number,number] = [...(effect.parameters.color ?? [0,0,0,1])]
      color[{colorR:0,colorG:1,colorB:2,colorA:3}[channel]] = number ?? base
      return {...effect,parameters:{...effect.parameters,color}}
    }
    return { ...effect, parameters: { ...effect.parameters, [channel]: number ?? base } }
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: Spacer) => Spacer): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'spacer' ? change(effect) : effect))
  }
  function keyframe(effect: Spacer, channel: Channel, base: number, start: number): Spacer {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: Spacer, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): Spacer {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Spacer">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'spacer', parameters: {} }])}>Добавить Spacer</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'spacer'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Spacer {index + 1}</legend>
              <label><input aria-label={`${clip.id} Spacer uniform`} type="checkbox" checked={effect.parameters.uniform??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,uniform:event.currentTarget.checked}}))} />Единые отступы</label>
              <label><input aria-label={`${clip.id} Spacer fill`} type="checkbox" checked={effect.parameters.fill??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,fill:event.currentTarget.checked}}))} />Залить фон</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Spacer ${channel}`} type="number" {min} {max} step="0.1"  value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), base))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Spacer ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Spacer ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Spacer ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Spacer ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Spacer ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей. Заливка делает фон непрозрачным.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'spacer', parameters: {} }))}>Сбросить Spacer</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Spacer</button></div>
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
