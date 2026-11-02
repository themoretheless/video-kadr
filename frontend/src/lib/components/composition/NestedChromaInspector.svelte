<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import type { WireVideoEffect } from '$lib/composition/types'
  let { disabled = false }: { disabled?: boolean } = $props()
  type Matte = Extract<WireVideoEffect, { kind: 'chroma_matte' }>
  const defaults: Matte = { kind: 'chroma_matte', parameters: { key: [0, 1, 0], balance: 0.5, clipBlack: 0, clipWhite: 1, despill: 1, feather: 0, shrink: 0, viewMatte: false } }
  const channels = ['keyR', 'keyG', 'keyB', 'balance', 'clipBlack', 'clipWhite', 'despill', 'feather', 'shrink', 'viewMatte'] as const
  type Channel = typeof channels[number]
  const labels: Record<Channel, string> = { keyR: 'Ключ R', keyG: 'Ключ G', keyB: 'Ключ B', balance: 'Баланс экрана', clipBlack: 'Уровень чёрного', clipWhite: 'Уровень белого', despill: 'Подавление ореола', feather: 'Растушёвка', shrink: 'Расширение маски', viewMatte: 'Просмотр маски' }
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function value(effect: Matte, channel: Channel): number {
    if (channel === 'keyR') return effect.parameters.key[0]
    if (channel === 'keyG') return effect.parameters.key[1]
    if (channel === 'keyB') return effect.parameters.key[2]
    if (channel === 'viewMatte') return Number(effect.parameters.viewMatte)
    return effect.parameters[channel]
  }
  function edit(track: string, clip: string, index: number, change: (effect: Matte) => Matte): void {
    void editNestedVideoEffects(compositionId, track, clip, (effects) => effects.map((effect, i) => i === index && effect.kind === 'chroma_matte' ? change(effect) : effect))
  }
  function baseline(effect: Matte, channel: Channel, number: number): Matte {
    const parameters = { ...effect.parameters }
    if (channel.startsWith('key')) {
      const key: [number, number, number] = [...parameters.key]
      key[channels.indexOf(channel)] = number
      parameters.key = key
    } else if (channel === 'viewMatte') parameters.viewMatte = number > 0.5
    else parameters[channel as Exclude<Channel, 'keyR' | 'keyG' | 'keyB' | 'viewMatte'>] = number
    return { ...effect, parameters }
  }
  function keyframe(effect: Matte, channel: Channel, start: number): Matte {
    const tick = nestedComposition.timeUs - start
    if (!Number.isSafeInteger(tick) || tick < 0) throw new Error('Курсор должен находиться после начала клипа')
    const previous = effect.animation?.[channel]
    const keyframes = previous?.mode === 'keyframes' ? previous.track.keyframes : []
    if (previous?.mode === 'keyframes' && previous.track.timeBase !== 1000000) throw new Error('Шкала канала должна быть в микросекундах')
    const points = [...keyframes.filter((point) => point.tick !== tick), { tick, value: value(effect, channel) }].sort((a,b) => a.tick-b.tick)
    return { ...effect, animation: { ...effect.animation, [channel]: { mode: 'keyframes', track: { timeBase: 1000000, interpolation: previous?.mode === 'keyframes' ? previous.track.interpolation : 'linear', keyframes: points } } } }
  }
</script>

<section aria-label="Chroma matte">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="clip-row">
          <span>{track.name} · {clip.id}</span>
          <button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, (effects) => [...effects, defaults])}>Добавить chroma matte</button>
        </div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'chroma_matte'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>Chroma matte {index + 1}</legend>
              {#each channels as channel}
                <div class="channel-row">
                  <label>{labels[channel]} · база {#if channel === 'viewMatte'}<input aria-label={`${clip.id} ${labels[channel]} база`} type="checkbox" checked={effect.parameters.viewMatte} onchange={(event) => edit(track.id, clip.id, index, (current) => baseline(current, channel, Number(event.currentTarget.checked)))} />{:else}<input aria-label={`${clip.id} ${labels[channel]} база`} type="number" min={channel === 'shrink' ? -5 : 0} max={channel === 'feather' ? 10 : channel === 'shrink' ? 5 : 1} step="0.01" value={value(effect, channel)} onchange={(event) => edit(track.id, clip.id, index, (current) => baseline(current, channel, Number(event.currentTarget.value)))} />{/if}</label>
                  <button class="btn ghost sm" disabled={nestedComposition.timeUs < ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)} onclick={() => edit(track.id, clip.id, index, (current) => keyframe(current, channel, ('placement' in clip ? clip.placement.timelineStartTick : clip.timelineStartTick)))}>Ключ по курсору</button>
                  {#if effect.animation?.[channel]}<button class="btn ghost sm" onclick={() => edit(track.id, clip.id, index, (current) => { const animation = { ...current.animation }; delete animation[channel]; return { ...current, animation } })}>Удалить анимацию</button>{/if}
                </div>
                {#if effect.animation?.[channel]?.mode === 'keyframes'}
                  <small>{effect.animation[channel].track.keyframes.map((point) => `${point.tick}: ${point.value}`).join(' · ')}</small>
                {/if}
              {/each}
              <button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, (effects) => effects.filter((_,i) => i !== index))}>Удалить эффект</button>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  .clip-row, .channel-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  fieldset { border: 0; padding: 0.5rem 0; margin: 0; }
  input { width: 5rem; }
  small { display: block; }
</style>
