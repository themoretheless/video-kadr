<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type FocusBlur = Extract<WireVideoEffect, { kind: 'focus_blur' }>
  type Channel = 'amount'|'size'|'feather'|'angle'|'centerX'|'centerY'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['amount','Размытие',0,500,20],['size','Размер, px',0,4000,200],['feather','Растушёвка, px',0,4000,200],['angle','Угол, °',-3.4028234663852886e38,3.4028234663852886e38,0],['centerX','Центр X, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['centerY','Центр Y, px',-Number.MAX_VALUE,Number.MAX_VALUE,0]]
  function sourceCenter(sourceId: string): [number,number] {
    const source = nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if (!source) throw new Error('Не найдены размеры исходного кадра')
    return [source.width / 2, source.height / 2]
  }
  function value(effect: FocusBlur, channel: Channel, base: number, sourceId: string): number | undefined {
    if (channel !== 'centerX' && channel !== 'centerY') return effect.parameters[channel] ?? base
    return effect.parameters.center?.[channel === 'centerX' ? 0 : 1] ?? undefined
  }
  function baseValue(channel: Channel): number { return channels.find(item => item[0]===channel)![4] }
  function setValue(effect: FocusBlur, channel: Channel, number: number | null, sourceId: string): FocusBlur {
    if (channel !== 'centerX' && channel !== 'centerY') return { ...effect, parameters: { ...effect.parameters, [channel]: number ?? baseValue(channel) } }
    const center: [number|null,number|null] = [...(effect.parameters.center ?? [null,null])]
    center[channel === 'centerX' ? 0 : 1] = number
    return { ...effect, parameters: { ...effect.parameters, center: center } }
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: FocusBlur) => FocusBlur): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'focus_blur' ? change(effect) : effect))
  }
  function keyframe(effect: FocusBlur, channel: Channel, base: number, start: number, sourceId: string): FocusBlur {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceCenter(sourceId)[channel === 'centerX' ? 0 : 1] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: FocusBlur, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): FocusBlur {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="FocusBlur">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'focus_blur', parameters: {} }])}>Добавить Focus Blur</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'focus_blur'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>FocusBlur {index + 1}</legend>
              <label>Форма<select aria-label={`${clip.id} FocusBlur shape`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,shape:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.shape??0)===0}>Radial</option><option value="1" selected={(effect.parameters.shape??0)===1}>Linear</option></select></label>
              <label><input type="checkbox" aria-label={`${clip.id} FocusBlur show`} checked={effect.parameters.show??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,show:event.currentTarget.checked}}))} />Показать область фокуса</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} FocusBlur ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} FocusBlur ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} FocusBlur ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} FocusBlur ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} FocusBlur ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} FocusBlur ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, center: null } }))}>Автоматический центр FocusBlur</button>
              <small>Пустая координата центра — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'focus_blur', parameters: {} }))}>Сбросить Focus Blur</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Focus Blur</button></div>
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
