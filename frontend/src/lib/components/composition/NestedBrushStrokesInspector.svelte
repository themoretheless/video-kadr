<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type BrushStrokes = Extract<WireVideoEffect, { kind: 'brush_strokes' }>
  type Channel = 'angle'|'size'|'length'|'density'|'randomness'|'blend'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['angle','Угол мазка',-3.4028234663852886e38,3.4028234663852886e38,135],['size','Размер кисти',0,5,2],['length','Длина мазка',0,40,10],['density','Плотность',0,2,1],['randomness','Случайность',0,2,1],['blend','Смешивание, %',0,100,0]]
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: BrushStrokes) => BrushStrokes): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'brush_strokes' ? change(effect) : effect))
  }
  function keyframe(effect: BrushStrokes, channel: Channel, base: number, start: number): BrushStrokes {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: effect.parameters[channel] ?? base }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: BrushStrokes, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): BrushStrokes {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="BrushStrokes">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'brush_strokes', parameters: {} }])}>Добавить BrushStrokes</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'brush_strokes'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>BrushStrokes {index + 1}</legend>
              <label>Поверхность<select aria-label={`${clip.id} BrushStrokes surface`} value={effect.parameters.surface??0} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,surface:Number(event.currentTarget.value)}}))}>{#each ['На исходном изображении','На прозрачном фоне','На белом фоне','На чёрном фоне'] as name,value}<option {value}>{name}</option>{/each}</select></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} BrushStrokes ${channel}`} type="number" {min} {max} step="0.1" value={effect.parameters[channel] ?? base} onchange={event => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, [channel]: Number(event.currentTarget.value) } }))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} BrushStrokes ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} BrushStrokes ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} BrushStrokes ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} BrushStrokes ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} BrushStrokes ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'brush_strokes', parameters: {} }))}>Сбросить BrushStrokes</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить BrushStrokes</button></div>
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
