<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type FourColorGradient = Extract<WireVideoEffect, { kind: 'four_color_gradient' }>
  type Channel = 'c1R'|'c1G'|'c1B'|'c1A'|'c2R'|'c2G'|'c2B'|'c2A'|'c3R'|'c3G'|'c3B'|'c3A'|'c4R'|'c4G'|'c4B'|'c4A'|'blend'|'opacity'
  const defaults = {c1:[1,1,0,1],c2:[0,1,0,1],c3:[1,0,1,1],c4:[0,0,1,1]} as const
  const components = {"c1R": ["c1", 0], "c1G": ["c1", 1], "c1B": ["c1", 2], "c1A": ["c1", 3], "c2R": ["c2", 0], "c2G": ["c2", 1], "c2B": ["c2", 2], "c2A": ["c2", 3], "c3R": ["c3", 0], "c3G": ["c3", 1], "c3B": ["c3", 2], "c3A": ["c3", 3], "c4R": ["c4", 0], "c4G": ["c4", 1], "c4B": ["c4", 2], "c4A": ["c4", 3]} as const
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['c1R','Цвет 1 · R',-3.4028234663852886e38,3.4028234663852886e38,1],['c1G','Цвет 1 · G',-3.4028234663852886e38,3.4028234663852886e38,1],['c1B','Цвет 1 · B',-3.4028234663852886e38,3.4028234663852886e38,0],['c1A','Цвет 1 · A',-3.4028234663852886e38,3.4028234663852886e38,1],['c2R','Цвет 2 · R',-3.4028234663852886e38,3.4028234663852886e38,0],['c2G','Цвет 2 · G',-3.4028234663852886e38,3.4028234663852886e38,1],['c2B','Цвет 2 · B',-3.4028234663852886e38,3.4028234663852886e38,0],['c2A','Цвет 2 · A',-3.4028234663852886e38,3.4028234663852886e38,1],['c3R','Цвет 3 · R',-3.4028234663852886e38,3.4028234663852886e38,1],['c3G','Цвет 3 · G',-3.4028234663852886e38,3.4028234663852886e38,0],['c3B','Цвет 3 · B',-3.4028234663852886e38,3.4028234663852886e38,1],['c3A','Цвет 3 · A',-3.4028234663852886e38,3.4028234663852886e38,1],['c4R','Цвет 4 · R',-3.4028234663852886e38,3.4028234663852886e38,0],['c4G','Цвет 4 · G',-3.4028234663852886e38,3.4028234663852886e38,0],['c4B','Цвет 4 · B',-3.4028234663852886e38,3.4028234663852886e38,1],['c4A','Цвет 4 · A',-3.4028234663852886e38,3.4028234663852886e38,1],['blend','Blend',1,1000,100],['opacity','Непрозрачность, %',0,100,100]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: FourColorGradient) => FourColorGradient): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'four_color_gradient' ? change(effect) : effect))
  }
  function value(effect: FourColorGradient, channel: Channel, base: number): number {
    if(channel==='blend'||channel==='opacity')return effect.parameters[channel]??base
    const [field,index]=components[channel]
    return effect.parameters[field]?.[index]??base
  }
  function setValue(effect: FourColorGradient, channel: Channel, next: number): FourColorGradient {
    if(channel==='blend'||channel==='opacity')return {...effect,parameters:{...effect.parameters,[channel]:next}}
    const [field,index]=components[channel]
    const color:[number,number,number,number]=[...(effect.parameters[field]??defaults[field])]
    color[index]=next
    return {...effect,parameters:{...effect.parameters,[field]:color}}
  }
  function keyframe(effect: FourColorGradient, channel: Channel, base: number, start: number): FourColorGradient {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: FourColorGradient, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): FourColorGradient {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="4-Color Gradient">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'four_color_gradient', parameters: {} }])}>Добавить 4-Color Gradient</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'four_color_gradient'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>4-Color Gradient {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} FourColorGradient ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} FourColorGradient ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} FourColorGradient ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} FourColorGradient ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} FourColorGradient ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} FourColorGradient ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'four_color_gradient', parameters: {} }))}>Сбросить 4-Color Gradient</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить 4-Color Gradient</button></div>
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
