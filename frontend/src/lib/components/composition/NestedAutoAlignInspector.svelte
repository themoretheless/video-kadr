<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type AutoAlign = Extract<WireVideoEffect, { kind: 'auto_align' }>
  type Channel = 'marginX'|'marginY'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['marginX','Горизонтальный отступ, px',0,4000,400],['marginY','Вертикальный отступ, px',0,4000,400]]
  function value(effect: AutoAlign, channel: Channel, base: number): number { return effect.parameters[channel] ?? base }
  function setValue(effect: AutoAlign, channel: Channel, number: number | null, base: number): AutoAlign {
    return { ...effect, parameters: { ...effect.parameters, [channel]: number ?? base } }
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: AutoAlign) => AutoAlign): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'auto_align' ? change(effect) : effect))
  }
  function keyframe(effect: AutoAlign, channel: Channel, base: number, start: number): AutoAlign {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: AutoAlign, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): AutoAlign {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="AutoAlign">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'auto_align', parameters: {} }])}>Добавить Auto Align</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'auto_align'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>AutoAlign {index + 1}</legend>
              <label>Горизонталь<select aria-label={`${clip.id} AutoAlign horizontal`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,horizontal:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.horizontal??2)===0}>Без выравнивания</option><option value="1" selected={(effect.parameters.horizontal??2)===1}>Слева</option><option value="2" selected={(effect.parameters.horizontal??2)===2}>По центру</option><option value="3" selected={(effect.parameters.horizontal??2)===3}>Справа</option></select></label>
              <label>Вертикаль<select aria-label={`${clip.id} AutoAlign vertical`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,vertical:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.vertical??2)===0}>Без выравнивания</option><option value="1" selected={(effect.parameters.vertical??2)===1}>Сверху</option><option value="2" selected={(effect.parameters.vertical??2)===2}>По центру</option><option value="3" selected={(effect.parameters.vertical??2)===3}>Снизу</option></select></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} AutoAlign ${channel}`} type="number" {min} {max} step="0.1"  value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), base))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} AutoAlign ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} AutoAlign ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} AutoAlign ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} AutoAlign ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} AutoAlign ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей. Отступы действуют при выравнивании по краям.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'auto_align', parameters: {} }))}>Сбросить Auto Align</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Auto Align</button></div>
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
