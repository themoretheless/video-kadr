<script lang="ts">
  import { nestedComposition, nestedHistory, editNestedVideoEffects } from '$lib/state/nestedComposition.svelte'
  import ToneCurveChannel from './ToneCurveChannel.svelte'
  let { disabled = false }: { disabled?: boolean } = $props()
  let channel = $state<'master' | 'red' | 'green' | 'blue' | 'hueVsSat' | 'hueVsHue' | 'hueVsLuma' | 'lumaVsSat' | 'satVsSat'>('master')
  const emptyPoints: readonly (readonly [number, number])[] = []
  const labels = { master: 'Общая', red: 'Красный', green: 'Зелёный', blue: 'Синий', hueVsSat: 'Оттенок → насыщенность', hueVsHue: 'Оттенок → оттенок', hueVsLuma: 'Оттенок → светлота', lumaVsSat: 'Светлота → насыщенность', satVsSat: 'Насыщенность → насыщенность' }
  let compositionId = $derived(nestedComposition.activeCompositionId ?? nestedComposition.document?.root ?? '')
  let tracks = $derived(nestedComposition.document?.compositions[compositionId]?.content.tracks ?? [])
  function change(track: string, clip: string, index: number, points: readonly (readonly [number, number])[]): void {
    const currentChannel = channel
    void editNestedVideoEffects(compositionId, track, clip, (effects) => effects.map((effect, i) => i === index && effect.kind === 'tone_curves' ? { ...effect, curves: { ...effect.curves, [currentChannel]: points } } : effect))
  }
</script>
<section aria-label="RGB-кривые">
  {#each tracks as track (track.id)}
    {#if track.kind === 'video' || track.kind === 'image'}
      {#each track.clips as clip (clip.id)}
        <div class="clip-row"><span>{track.name} · {clip.id}</span><button class="btn ghost sm" disabled={disabled || nestedHistory.busy || track.locked} onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, (effects) => [...effects, { kind: 'tone_curves', curves: {} }])}>Добавить RGB-кривые</button></div>
        {#each (clip.effects ?? []) as effect, index}
          {#if effect.kind === 'tone_curves'}
            <fieldset disabled={disabled || nestedHistory.busy || track.locked}>
              <legend>RGB-кривые {index + 1}</legend>
              <label>Канал <select aria-label={`${clip.id} канал кривой`} value={channel} onchange={(event) => { const next = event.currentTarget.value; if (next === 'master' || next === 'red' || next === 'green' || next === 'blue' || next === 'hueVsSat' || next === 'hueVsHue' || next === 'hueVsLuma' || next === 'lumaVsSat' || next === 'satVsSat') channel = next }}>{#each Object.entries(labels) as [key, label]}<option value={key}>{label}</option>{/each}</select></label>
              <ToneCurveChannel periodic={!["master", "red", "green", "blue"].includes(channel)} points={effect.curves[channel] ?? emptyPoints} label={labels[channel]} disabled={disabled || nestedHistory.busy || track.locked} onchange={(points) => change(track.id, clip.id, index, points)} />
              <button class="btn ghost sm" onclick={() => void editNestedVideoEffects(compositionId, track.id, clip.id, (effects) => effects.filter((_, i) => i !== index))}>Удалить RGB-кривые</button>
            </fieldset>
          {/if}
        {/each}
      {/each}
    {/if}
  {/each}
</section>
<style>
  .clip-row { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  fieldset { border: 0; padding: 0.5rem 0; margin: 0; }
</style>
