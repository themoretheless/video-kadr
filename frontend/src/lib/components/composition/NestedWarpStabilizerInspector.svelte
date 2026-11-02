<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type WarpStabilizer = Extract<WireVideoEffect, { kind: 'warp_stabilizer' }>
  type Channel='smoothness'|'maxScale'|'actionSafe'|'additionalScale'|'cropLess'
  const channels:readonly (readonly [Channel,string,number,number,number])[]=[['smoothness','Сглаживание, %',0,1000,50],['maxScale','Максимальный масштаб, %',100,1000,150],['actionSafe','Безопасный отступ, %',0,100,0],['additionalScale','Дополнительный масштаб, %',50,400,100],['cropLess','Баланс обрезки, %',0,100,50]]
  function value(effect:WarpStabilizer,channel:Channel,base:number):number{return effect.parameters[channel]??base}
  function setValue(effect:WarpStabilizer,channel:Channel,number:number|null):WarpStabilizer{return {...effect,parameters:{...effect.parameters,[channel]:number??channels.find(([name])=>name===channel)![4]}}}
  const choices=[['result','Результат',0,['Smooth Motion','No Motion']],['method','Метод',1,['Position','Position, Scale, Rotation','Perspective','Subspace Warp']],['framing','Края',2,['Stabilize Only','Stabilize, Crop','Stabilize, Crop, Auto-scale','Stabilize, Synthesize Edges']]] as const

  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function edit(track: string, clip: string, index: number, change: (effect: WarpStabilizer) => WarpStabilizer): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'warp_stabilizer' ? change(effect) : effect))
  }
  function keyframe(effect: WarpStabilizer, channel: Channel, base: number, start: number): WarpStabilizer {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value(effect, channel, base) }].sort((a, b) => a.tick - b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } } } }
  }
  function editKey(effect: WarpStabilizer, channel: Channel, tick: number, patch: { tick?: number; value?: number } | null): WarpStabilizer {
    const previous = effect.animation?.[channel]
    if (previous?.mode !== 'keyframes') return effect
    const animation = { ...effect.animation }
    const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
    if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
    else delete animation[channel]
    return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
  }
</script>
<section aria-label="Warp Stabilizer">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'warp_stabilizer', parameters: {} }])}>Добавить Warp Stabilizer</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'warp_stabilizer'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Warp Stabilizer {index + 1}</legend>
              {#each channels as [channel, label, min, max, base]}
                {@const animation = effect.animation?.[channel]}
                <div class="row">
                  <label>{label}<input aria-label={`${clip.id} Warp Stabilizer ${channel}`} type="number" {min} {max} step="0.1" value={value(effect, channel, base)} onchange={event => edit(track.id, clip.id, index, current => setValue(current, channel, event.currentTarget.value === '' ? null : Number(event.currentTarget.value)))} /></label>
                  <button class="btn ghost sm" aria-label={`${clip.id} Warp Stabilizer ${channel} key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, current => keyframe(current, channel, base, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ у курсора</button>
                  {#if animation}<button class="btn ghost sm" aria-label={`${clip.id} Warp Stabilizer ${channel} clear`} onclick={() => edit(track.id, clip.id, index, current => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation: Object.keys(animation).length ? animation : undefined } })}>Очистить ключи</button>{/if}
                </div>
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{label} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Warp Stabilizer ${channel} ${key.tick} time`} type="number" min="0" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) }))} /></label>
                        <label>Значение<input aria-label={`${clip.id} Warp Stabilizer ${channel} ${key.tick} value`} type="number" {min} {max} step="0.01" value={key.value} onchange={event => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, { value: Number(event.currentTarget.value) }))} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Warp Stabilizer ${channel} ${key.tick} delete`} onclick={() => edit(track.id, clip.id, index, current => editKey(current, channel, key.tick, null))}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              {#each choices as [channel,label,base,options]}
                <label>{label}<select aria-label={`${clip.id} Warp Stabilizer ${channel}`} value={String(effect.parameters[channel]??base)} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:Number(event.currentTarget.value)}}))}>{#each options as option,i}<option value={String(i)}>{option}</option>{/each}</select></label>
              {/each}
              {#each [['preserveScale','Сохранять масштаб'],['detailed','Подробный анализ']] as [channel,label]}
                <label>{label}<input aria-label={`${clip.id} Warp Stabilizer ${channel}`} type="checkbox" checked={Boolean(effect.parameters[channel as 'preserveScale'|'detailed'])} onchange={event=>edit(track.id,clip.id,index,current=>({...current,parameters:{...current.parameters,[channel]:event.currentTarget.checked}}))}/></label>
              {/each}
              <div class="row"><button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, () => ({ kind: 'warp_stabilizer', parameters: {} }))}>Сбросить Warp Stabilizer</button><button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Warp Stabilizer</button></div>
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
