<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type AutoReframe = Extract<WireVideoEffect, { kind: 'auto_reframe' }>
  type Channel='zoom'|'offsetX'|'offsetY'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[['zoom','Масштаб, %',100,400,100],['offsetX','Смещение X',-Number.MAX_VALUE,Number.MAX_VALUE,0],['offsetY','Смещение Y',-Number.MAX_VALUE,Number.MAX_VALUE,0]]
  function value(effect:AutoReframe,channel:Channel,base:number):number{return channel==='zoom'?effect.parameters.zoom??base:effect.parameters.offset?.[channel==='offsetX'?0:1]??base}
  function setValue(effect:AutoReframe,channel:Channel,number:number|null):AutoReframe{if(channel==='zoom')return {...effect,parameters:{...effect.parameters,zoom:number??100}};const offset:[number,number]=[effect.parameters.offset?.[0]??0,effect.parameters.offset?.[1]??0];offset[channel==='offsetX'?0:1]=number??0;return {...effect,parameters:{...effect.parameters,offset}}}

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: AutoReframe) => AutoReframe): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'auto_reframe' ? change(effect) : effect))
  }
  function keyframe(effect: AutoReframe, channel: Channel, base: number, start: number): AutoReframe {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: AutoReframe, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): AutoReframe {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Auto Reframe">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'auto_reframe', parameters: {} }])}>Добавить Auto Reframe</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'auto_reframe'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Auto Reframe {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Auto Reframe ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Auto Reframe ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Auto Reframe ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Auto Reframe ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Auto Reframe ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Auto Reframe ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <label>Движение<select aria-label={`${clip.id} Auto Reframe preset`} value={String(effect.parameters.preset??1)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,preset:Number(event.currentTarget.value)}}))}>
                {#each ['Slower Motion','Default','Faster Motion'] as label,preset}<option value={String(preset)}>{label}</option>{/each}
              </select></label>
              <label>Пропорции<select aria-label={`${clip.id} Auto Reframe aspect`} value={String(effect.parameters.aspect??0)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,aspect:Number(event.currentTarget.value)}}))}>
                {#each ['Sequence','Vertical 9:16','Square 1:1','Vertical 4:5','Horizontal 16:9'] as label,aspect}<option value={String(aspect)}>{label}</option>{/each}
              </select></label>
              <small>Поля задают базовые значения и значения новых ключей.</small>
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'auto_reframe', parameters: {} }))}>Сбросить Auto Reframe</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Auto Reframe</button></div>
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
