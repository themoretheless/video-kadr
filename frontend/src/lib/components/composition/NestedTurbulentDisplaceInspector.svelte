<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type TurbulentDisplace = Extract<WireVideoEffect, { kind: 'turbulent_displace' }>
  type Channel = 'amount'|'size'|'offsetX'|'offsetY'|'complexity'|'evolution'|'cycleRevs'|'seed'
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['amount','Смещение',-1000,1000,50],['size','Размер, px',2,1000,100],['offsetX','Центр X, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['offsetY','Центр Y, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['complexity','Сложность',1,10,1],['evolution','Эволюция, °',-3.4028234663852886e38,3.4028234663852886e38,0],['cycleRevs','Обороты цикла',1,100,1],['seed','Случайное зерно',0,10000,0]]
  function sourceCenter(sourceId: string): [number,number] {
    const source = nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if (!source) throw new Error('Не найдены размеры исходного кадра')
    return [source.width / 2, source.height / 2]
  }
  function value(effect: TurbulentDisplace, channel: Channel, base: number, sourceId: string): number | undefined {
    if (channel !== 'offsetX' && channel !== 'offsetY') return effect.parameters[channel] ?? base
    return effect.parameters.offset?.[channel === 'offsetX' ? 0 : 1] ?? undefined
  }
  function baseValue(channel: Channel): number { return channels.find(item => item[0]===channel)![4] }
  function setValue(effect: TurbulentDisplace, channel: Channel, number: number | null, sourceId: string): TurbulentDisplace {
    if (channel !== 'offsetX' && channel !== 'offsetY') return { ...effect, parameters: { ...effect.parameters, [channel]: number ?? baseValue(channel) } }
    const center: [number|null,number|null] = [...(effect.parameters.offset ?? [null,null])]
    center[channel === 'offsetX' ? 0 : 1] = number
    return { ...effect, parameters: { ...effect.parameters, offset: center } }
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: TurbulentDisplace) => TurbulentDisplace): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'turbulent_displace' ? change(effect) : effect))
  }
  function keyframe(effect: TurbulentDisplace, channel: Channel, base: number, start: number, sourceId: string): TurbulentDisplace {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceCenter(sourceId)[channel === 'offsetX' ? 0 : 1] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: TurbulentDisplace, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): TurbulentDisplace {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="TurbulentDisplace">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'turbulent_displace', parameters: {} }])}>Добавить Turbulent Displace</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'turbulent_displace'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>TurbulentDisplace {index + 1}</legend>
              <label>Режим<select aria-label={`${clip.id} TurbulentDisplace displacement`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,displacement:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.displacement??0)===0}>Turbulent</option><option value="1" selected={(effect.parameters.displacement??0)===1}>Bulge</option><option value="2" selected={(effect.parameters.displacement??0)===2}>Twist</option><option value="3" selected={(effect.parameters.displacement??0)===3}>Turbulent Smoother</option><option value="4" selected={(effect.parameters.displacement??0)===4}>Bulge Smoother</option><option value="5" selected={(effect.parameters.displacement??0)===5}>Twist Smoother</option><option value="6" selected={(effect.parameters.displacement??0)===6}>Vertical Displacement</option><option value="7" selected={(effect.parameters.displacement??0)===7}>Horizontal Displacement</option><option value="8" selected={(effect.parameters.displacement??0)===8}>Cross Displacement</option></select></label>
              <label>Закрепление краёв<select aria-label={`${clip.id} TurbulentDisplace pinning`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,pinning:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.pinning??1)===0}>None</option><option value="1" selected={(effect.parameters.pinning??1)===1}>Pin All</option><option value="2" selected={(effect.parameters.pinning??1)===2}>Pin Horizontal</option><option value="3" selected={(effect.parameters.pinning??1)===3}>Pin Vertical</option></select></label>
              <label>Сглаживание<select aria-label={`${clip.id} TurbulentDisplace antialias`} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,antialias:Number(event.currentTarget.value)}}))}><option value="0" selected={(effect.parameters.antialias??0)===0}>Low</option><option value="1" selected={(effect.parameters.antialias??0)===1}>High</option></select></label>
              <label><input type="checkbox" aria-label={`${clip.id} TurbulentDisplace cycle`} checked={effect.parameters.cycle??false} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,cycle:event.currentTarget.checked}}))} />Циклическая эволюция</label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} TurbulentDisplace ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} TurbulentDisplace ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} TurbulentDisplace ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} TurbulentDisplace ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} TurbulentDisplace ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} TurbulentDisplace ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, offset: null } }))}>Автоматический центр TurbulentDisplace</button>
              <small>Пустая координата центра — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'turbulent_displace', parameters: {} }))}>Сбросить Turbulent Displace</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Turbulent Displace</button></div>
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
