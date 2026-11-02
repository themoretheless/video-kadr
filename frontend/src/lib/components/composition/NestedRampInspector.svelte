<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Ramp = Extract<WireVideoEffect, { kind: 'ramp' }>
  type PointChannel = "startX"|"startY"|"endX"|"endY"
  type Channel = 'startX'|'startY'|'endX'|'endY'|'startColorR'|'startColorG'|'startColorB'|'startColorA'|'endColorR'|'endColorG'|'endColorB'|'endColorA'|'blend'
  const points = {startX:["start",0],startY:["start",1],endX:["end",0],endY:["end",1]} as const
  const colors = {"startColorR": ["startColor", 0], "startColorG": ["startColor", 1], "startColorB": ["startColor", 2], "startColorA": ["startColor", 3], "endColorR": ["endColor", 0], "endColorG": ["endColor", 1], "endColorB": ["endColor", 2], "endColorA": ["endColor", 3]} as const
  const channels: readonly (readonly [Channel,string,number,number,number])[] = [['startX','startX, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['startY','startY, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['endX','endX, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['endY','endY, px',-Number.MAX_VALUE,Number.MAX_VALUE,0],['startColorR','startColor · R',-3.4028234663852886e38,3.4028234663852886e38,0],['startColorG','startColor · G',-3.4028234663852886e38,3.4028234663852886e38,0],['startColorB','startColor · B',-3.4028234663852886e38,3.4028234663852886e38,0],['startColorA','startColor · A',-3.4028234663852886e38,3.4028234663852886e38,1],['endColorR','endColor · R',-3.4028234663852886e38,3.4028234663852886e38,1],['endColorG','endColor · G',-3.4028234663852886e38,3.4028234663852886e38,1],['endColorB','endColor · B',-3.4028234663852886e38,3.4028234663852886e38,1],['endColorA','endColor · A',-3.4028234663852886e38,3.4028234663852886e38,1],['blend','Blend With Original, %',0,100,0]]
  function isPoint(channel: Channel): channel is PointChannel { return channel in points }
  function sourceCenter(sourceId: string): [number,number] {
    const source = nestedComposition.document?.compositions[compositionId]?.content.sources[sourceId]
    if (!source) throw new Error('Не найдены размеры исходного кадра')
    return [source.width / 2, source.height / 2]
  }
  function value(effect: Ramp, channel: Channel, base: number, _sourceId: string): number | undefined {
    if(channel==='blend')return effect.parameters.blend??base
    if(isPoint(channel)) {
      const [field,index]=points[channel]
      const point=effect.parameters[field]??(field==='start'?[null,0]:[null,null])
      return point[index]??undefined
    }
    const [field,index]=colors[channel]
    return effect.parameters[field]?.[index]??base
  }
  function setValue(effect: Ramp, channel: Channel, number: number | null, _sourceId: string): Ramp {
    if(channel==='blend')return {...effect,parameters:{...effect.parameters,blend:number??0}}
    if(isPoint(channel)) {
      const [field,index]=points[channel]
      const point:[number|null,number|null]=[...(effect.parameters[field]??(field==='start'?[null,0]:[null,null]))]
      point[index]=number
      return {...effect,parameters:{...effect.parameters,[field]:point}}
    }
    const [field,index]=colors[channel]
    const color:[number,number,number,number]=[...(effect.parameters[field]??(field==='startColor'?[0,0,0,1]:[1,1,1,1]))]
    color[index]=number??(field==='endColor'||index===3?1:0)
    return {...effect,parameters:{...effect.parameters,[field]:color}}
  }

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: Ramp) => Ramp): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'ramp' ? change(effect) : effect))
  }
  function keyframe(effect: Ramp, channel: Channel, base: number, start: number, sourceId: string): Ramp {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base, sourceId) ?? sourceCenter(sourceId)[isPoint(channel)?points[channel][1]:0] }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: Ramp, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): Ramp {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Ramp">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'ramp', parameters: {} }])}>Добавить Ramp</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'ramp'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Ramp {index + 1}</legend>
              <label>Форма<select aria-label={`${clip.id} Ramp shape`} value={effect.parameters.shape??'linear'} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,shape:event.currentTarget.value==='radial'?'radial':'linear'}}))}><option value="linear">Linear Ramp</option><option value="radial">Radial Ramp</option></select></label>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Ramp ${channel}`} type="number" {min} {max} step="0.1" placeholder="Авто" value={value(effect, channel, base, clip.sourceId)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value), clip.sourceId))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Ramp ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick), clip.sourceId))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Ramp ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Ramp ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Ramp ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Ramp ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, current => ({ ...current, parameters: { ...current.parameters, center: null } }))}>Автоматический центр Ramp</button>
              <small>Пустая координата центра — автоматическая для этой оси. Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'ramp', parameters: {} }))}>Сбросить Ramp</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Ramp</button></div>
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
