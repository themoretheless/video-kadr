<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type CornerPin = Extract<WireVideoEffect, { kind: 'corner_pin' }>
  type Point = 'upperLeft'|'upperRight'|'lowerLeft'|'lowerRight'
  type Channel = 'upperLeftX'|'upperLeftY'|'upperRightX'|'upperRightY'|'lowerLeftX'|'lowerLeftY'|'lowerRightX'|'lowerRightY'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['upperLeftX', 'upperLeftX, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['upperLeftY', 'upperLeftY, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['upperRightX', 'upperRightX, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['upperRightY', 'upperRightY, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['lowerLeftX', 'lowerLeftX, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['lowerLeftY', 'lowerLeftY, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['lowerRightX', 'lowerRightX, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0], ['lowerRightY', 'lowerRightY, px', -Number.MAX_VALUE, Number.MAX_VALUE, 0]]
  function location(channel:Channel):[Point,0|1] { return [channel.slice(0,-1) as Point,channel.endsWith('X')?0:1] }
  function sourceDefault(sourceId:string,channel:Channel):number {
    const source=nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if(!source)throw new Error('Не найдены размеры исходного кадра')
    const [point,axis]=location(channel)
    return axis===0 ? (point.endsWith('Right')?source.width:0) : (point.startsWith('lower')?source.height:0)
  }
  function value(effect:CornerPin,channel:Channel,_base:number,_sourceId:string):number|undefined {
    const [point,axis]=location(channel);return effect.parameters[point]?.[axis] ?? undefined
  }
  function setValue(effect:CornerPin,channel:Channel,number:number|null,_sourceId:string):CornerPin {
    const [point,axis]=location(channel)
    const coordinates:[number|null,number|null]=[...(effect.parameters[point]??[null,null])]
    coordinates[axis]=number
    return {...effect,parameters:{...effect.parameters,[point]:coordinates}}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: CornerPin) => CornerPin): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'corner_pin' ? change(effect) : effect))
  }
  function keyframe(effect: CornerPin, channel: Channel, base: number, start: number, sourceId: string): CornerPin {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceDefault(sourceId,channel) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: CornerPin, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): CornerPin {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="CornerPin">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'corner_pin', parameters: {} }])}>Добавить CornerPin</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'corner_pin'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>CornerPin {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} CornerPin ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} CornerPin ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} CornerPin ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} CornerPin ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} CornerPin ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} CornerPin ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, center: null } }))}>Автоматический центр CornerPin</button>
              <small>Пустая координата угла — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'corner_pin', parameters: {} }))}>Сбросить CornerPin</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить CornerPin</button></div>
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
