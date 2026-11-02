<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Transform = Extract<WireVideoEffect, { kind: 'transform' }>
  type Channel = 'scaleHeight'|'scaleWidth'|'skew'|'skewAxis'|'rotation'|'opacity'|'shutterAngle'|'anchorX'|'anchorY'|'positionX'|'positionY'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [["scaleHeight", "Масштаб по высоте, %", 0, 10000, 100], ["scaleWidth", "Масштаб по ширине, %", 0, 10000, 100], ["skew", "Наклон, °", -70, 70, 0], ["skewAxis", "Ось наклона, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 0], ["rotation", "Поворот, °", -3.4028234663852886e+38, 3.4028234663852886e+38, 0], ["opacity", "Непрозрачность, %", 0, 100, 100], ["shutterAngle", "Угол затвора, °", 0, 360, 0], ["anchorX", "Опорная точка X, px", -1.7976931348623157e+308, 1.7976931348623157e+308, 0], ["anchorY", "Опорная точка Y, px", -1.7976931348623157e+308, 1.7976931348623157e+308, 0], ["positionX", "Позиция X, px", -1.7976931348623157e+308, 1.7976931348623157e+308, 0], ["positionY", "Позиция Y, px", -1.7976931348623157e+308, 1.7976931348623157e+308, 0]]
  function pointChannel(channel:Channel):'anchor'|'position'|undefined{return channel.startsWith('anchor')?'anchor':channel.startsWith('position')?'position':undefined}
  function value(effect:Transform,channel:Channel,base:number):number|undefined{const point=pointChannel(channel);if(point)return effect.parameters[point]?.[channel.endsWith('X')?0:1]??undefined;return effect.parameters[channel as Exclude<Channel,'anchorX'|'anchorY'|'positionX'|'positionY'>]??base}
  function setValue(effect:Transform,channel:Channel,number:number|null):Transform{const point=pointChannel(channel);if(point){const coords:[number|null,number|null]=[...(effect.parameters[point]??[null,null])];coords[channel.endsWith('X')?0:1]=number;return {...effect,parameters:{...effect.parameters,[point]:coords}}}return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(item=>item[0]===channel)![4]}}}
  function automatic(sourceId:string,channel:Channel):number{const source=nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId];if(!source)throw new Error('Не найдены размеры исходного кадра');return (channel.endsWith('X')?source.width:source.height)/2}

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: Transform) => Transform): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'transform' ? change(effect) : effect))
  }
  function keyframe(effect: Transform, channel: Channel, base: number, start: number, sourceId:string): Transform {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) ?? automatic(sourceId,channel) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: Transform, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): Transform {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Transform">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'transform', parameters: {} }])}>Добавить Transform</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'transform'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Transform {index + 1}</legend>
<label><input aria-label={`${clip.id} Transform uniformScale`} type="checkbox" checked={effect.parameters.uniformScale??true} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,uniformScale:event.currentTarget.checked}}))}/>Единый масштаб</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Transform ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Transform ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick),clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Transform ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Transform ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Transform ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Transform ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'transform', parameters: {} }))}>Сбросить Transform</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Transform</button></div>
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
