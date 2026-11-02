<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoLimiter } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  const choices: readonly { key: 'clipLevel' | 'compression' | 'axis'; label: string; base: number; values: readonly string[] }[] = [
    { key: 'clipLevel', label: 'Уровень', base: 0, values: Array.from({ length: 10 }, (_, i) => `${100 + i} IRE`) },
    { key: 'compression', label: 'Сжатие перед ограничением', base: 1, values: ['Нет', '3%', '5%', '10%', '20%'] },
    { key: 'axis', label: 'Ограничивать', base: 3, values: ['Яркость', 'Цветность', 'Яркость и цветность', 'Smart Limit'] },
  ]
  function edit(track: string, clip: string, index: number, patch: Partial<WireVideoLimiter>): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => i === index && effect.kind === 'video_limiter' ? { ...effect, parameters: { ...effect.parameters, ...patch } } : effect))
  }
  const colorChannels = ['red', 'green', 'blue', 'alpha'] as const
  function keyframe(track: string, clip: string, index: number, start: number): void {
    const time = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(time) || time < 0) return
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => {
      if (i !== index || effect.kind !== 'video_limiter') return effect
      const animation = { ...effect.animation }
      const channels = ['red', 'green', 'blue', 'alpha'] as const
      const value = effect.parameters.warningColor ?? [1, 0, 0, 1]
      channels.forEach((channel, component) => {
        const previous = animation[channel]
        if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала цвета должна быть в микросекундах')
        const keys = previous?.mode === 'keyframes' ? previous.track.keyframes : []
        const keyframes = [...keys.filter(key => key.tick !== time), { ...keys.find(key => key.tick === time), tick: time, value: value[component]! }].sort((a, b) => a.tick - b.tick)
        animation[channel] = { mode: 'keyframes', track: previous?.mode === 'keyframes' ? { ...previous.track, keyframes } : { timeBase: 1000000, interpolation: 'linear', keyframes } }
      })
      return { ...effect, animation }
    }))
  }
  function editKey(track: string, clip: string, index: number, channel: typeof colorChannels[number], tick: number, patch: { tick?: number; value?: number } | null): void {
    void editNestedVideoEffects(compositionId, track, clip, effects => effects.map((effect, i) => {
      if (i !== index || effect.kind !== 'video_limiter') return effect
      const previous = effect.animation?.[channel]
      if (previous?.mode !== 'keyframes') return effect
      const animation = { ...effect.animation }
      const keyframes = previous.track.keyframes.flatMap(key => key.tick === tick ? patch ? [{ ...key, ...patch }] : [] : [key]).sort((a, b) => a.tick - b.tick)
      if (keyframes.length) animation[channel] = { ...previous, track: { ...previous.track, keyframes } }
      else delete animation[channel]
      return { ...effect, animation: Object.keys(animation).length ? animation : undefined }
    }))
  }
  function hex(color: WireVideoLimiter['warningColor']): string {
    return '#' + (color ?? [1, 0, 0, 1]).slice(0, 3).map(value => Math.round(value * 255).toString(16).padStart(2, '0')).join('')
  }
  function color(value: string, alpha: number): readonly [number, number, number, number] {
    return [parseInt(value.slice(1, 3), 16) / 255, parseInt(value.slice(3, 5), 16) / 255, parseInt(value.slice(5, 7), 16) / 255, alpha]
  }
</script>
<section aria-label="Video Limiter">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => [...effects, { kind: 'video_limiter', parameters: {} }])}>Добавить Video Limiter</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'video_limiter'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Video Limiter {index + 1}</legend>
              {#each choices as choice}
                <label>{choice.label}<select aria-label={`${clip.id} Video Limiter ${choice.key}`} value={effect.parameters[choice.key] ?? choice.base} onchange={event => edit(track.id, clip.id, index, { [choice.key]: Number(event.currentTarget.value) })}>
                  {#each choice.values as label, value}<option {value}>{label}</option>{/each}
                </select></label>
              {/each}
              <label class="toggle"><input aria-label={`${clip.id} Video Limiter warning`} type="checkbox" checked={effect.parameters.gamutWarning ?? false} onchange={event => edit(track.id, clip.id, index, { gamutWarning: event.currentTarget.checked })} />Предупреждение о цветовом диапазоне</label>
              <label>Цвет предупреждения<input aria-label={`${clip.id} Video Limiter color`} type="color" value={hex(effect.parameters.warningColor)} onchange={event => edit(track.id, clip.id, index, { warningColor: color(event.currentTarget.value, effect.parameters.warningColor?.[3] ?? 1) })} /></label>
              <div class="row"><button class="btn ghost sm" aria-label={`${clip.id} Video Limiter color key`} disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => keyframe(track.id, clip.id, index, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick))}>Ключ цвета у курсора</button>
                <button class="btn ghost sm" disabled={!effect.animation} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.map((current, i) => i === index && current.kind === 'video_limiter' ? { ...current, animation: undefined } : current))}>Очистить ключи цвета</button></div>
              {#if effect.animation}<small>Ключи RGBA: {Object.entries(effect.animation).map(([channel, value]) => `${channel}: ${value?.mode === 'keyframes' ? value.track.keyframes.length : 1}`).join(' · ')}. Поле цвета задаёт значение нового ключа.</small>{/if}
              {#each colorChannels as channel}
                {@const animation = effect.animation?.[channel]}
                {#if animation?.mode === 'keyframes'}
                  <details><summary>{channel} · {animation.track.keyframes.length} ключей</summary>
                    {#each animation.track.keyframes as key (key.tick)}
                      <div class="row">
                        <label>Время, с<input aria-label={`${clip.id} Video Limiter ${channel} ${key.tick} time`} type="number" step="0.001" value={key.tick / animation.track.timeBase} onchange={event => editKey(track.id, clip.id, index, channel, key.tick, { tick: Math.round(Number(event.currentTarget.value) * animation.track.timeBase) })} /></label>
                        <label>Значение<input aria-label={`${clip.id} Video Limiter ${channel} ${key.tick} value`} type="number" min="0" max="1" step="0.01" value={key.value} onchange={event => editKey(track.id, clip.id, index, channel, key.tick, { value: Number(event.currentTarget.value) })} /></label>
                        <button class="btn ghost sm" aria-label={`${clip.id} Video Limiter ${channel} ${key.tick} delete`} onclick={() => editKey(track.id, clip.id, index, channel, key.tick, null)}>Удалить ключ</button>
                      </div>
                    {/each}
                  </details>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, effects => effects.filter((_, i) => i !== index))}>Удалить Video Limiter</button>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  section { display: grid; gap: 6px; }
  fieldset { border: 0; padding: 0; margin: 0; display: grid; gap: 6px; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .toggle { justify-content: flex-start; }
  fieldset > button { justify-self: start; }
  label { display: flex; gap: 6px; align-items: center; justify-content: space-between; }
</style>
