<script lang="ts">
  import {
    clipDurationTicks,
    COMPOSITION_TIME_BASE,
    MAX_SPEED_RAMP_POINTS,
    type AudioClip,
    type CompositionSpeedRamp,
    type VideoClip,
  } from '$lib/composition/types.js'
  import { speedAtSourceProgress, speedRampSourceProgressAtTimelineTick } from '$lib/composition/speedRamp.js'
  import { compositionState, updateCompositionClipSpeed, updateCompositionSpeedRamp } from '$lib/state/composition.svelte.js'
  import { state as legacyState } from '$lib/state/store.svelte.js'

  let { clip }: { clip: VideoClip | AudioClip } = $props()
  const ramp = $derived(clip.speedRamp)
  const span = $derived(clip.sourceOutTicks - clip.sourceInTicks)
  const graph = $derived.by(() => {
    if (!ramp) return ''
    const speeds = ramp.points.map((point) => point.speed)
    const low = Math.min(...speeds)
    const range = Math.max(0.01, Math.max(...speeds) - low)
    return ramp.points.map((point) =>
      `${((point.sourceProgressTick / span) * 100).toFixed(2)},${(36 - ((point.speed - low) / range) * 28).toFixed(2)}`,
    ).join(' ')
  })
  const capabilityReason = $derived.by(() => {
    const capabilities = legacyState.capabilities
    if (!capabilities) return 'Проверяем server capability speed-ramp…'
    const feature = capabilities.features?.find((option) => option.id === 'speed-ramp')
    return !feature
      ? 'Сервер не объявил capability speed-ramp.'
      : feature.available ? null : feature.reason?.trim() || 'Speed ramp недоступна на сервере.'
  })

  function run(action: () => void): void {
    try {
      compositionState.ui.message = ''
      action()
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    }
  }

  function commit(value: CompositionSpeedRamp): void {
    updateCompositionSpeedRamp(clip.id, { ...value, points: value.points.map((point) => ({ ...point })) })
  }

  function enable(): void {
    commit({
      interpolation: 'linear',
      points: [{ sourceProgressTick: 0, speed: clip.speed ?? 1 }, { sourceProgressTick: span, speed: clip.speed ?? 1 }],
      audioPolicy: 'preserve_pitch',
    })
  }

  function changePoint(index: number, field: 'sourceProgressTick' | 'speed', value: number): void {
    if (!ramp) return
    if (!index && field === 'speed') return updateCompositionClipSpeed(clip.id, value)
    commit({
      ...ramp,
      points: ramp.points.map((point, candidate) => candidate === index ? { ...point, [field]: value } : point),
    })
  }

  function addPoint(): void {
    if (!ramp || ramp.points.length >= MAX_SPEED_RAMP_POINTS) return
    const local = Math.round(compositionState.transport.playheadTicks - clip.timelineStartTicks)
    if (local <= 0 || local >= clipDurationTicks(clip)) throw new Error('Поставьте playhead внутрь clip')
    const progress = speedRampSourceProgressAtTimelineTick(span, clip.speed ?? 1, ramp, local)
    const index = ramp.points.findIndex((point) => point.sourceProgressTick > progress)
    const before = ramp.points[index - 1]?.sourceProgressTick ?? progress
    const after = ramp.points[index]?.sourceProgressTick ?? progress
    if (progress - before < 1_000 || after - progress < 1_000) throw new Error('Между speed points нужно минимум 1000 ticks')
    const point = { sourceProgressTick: progress, speed: speedAtSourceProgress(span, clip.speed ?? 1, ramp, progress) }
    commit({ ...ramp, points: [...ramp.points.slice(0, index), point, ...ramp.points.slice(index)] })
  }

  const seconds = (ticks: number): number => Number((ticks / COMPOSITION_TIME_BASE).toFixed(6))
</script>

<fieldset class="composition-fieldset composition-speed-ramp-editor">
  <legend>Speed ramp</legend>
  {#if !ramp}
    <button class="btn ghost sm" type="button" disabled={Boolean(capabilityReason)} onclick={() => run(enable)}>Включить speed ramp</button>
    {#if capabilityReason}<p class="composition-inline-error" role="status">{capabilityReason}</p>{/if}
  {:else}
    <div class="composition-form-grid">
      <label>Интерполяция
        <select aria-label="Интерполяция speed ramp" value={ramp.interpolation} onchange={(event) => run(() => commit({ ...ramp, interpolation: event.currentTarget.value as CompositionSpeedRamp['interpolation'] }))}>
          <option value="hold">Hold</option><option value="linear">Linear</option>
        </select>
      </label>
      <label>Политика аудио
        <select aria-label="Политика аудио speed ramp" value={ramp.audioPolicy ?? 'preserve_pitch'} onchange={(event) => run(() => commit({ ...ramp, audioPolicy: event.currentTarget.value as CompositionSpeedRamp['audioPolicy'] }))}>
          <option value="preserve_pitch">Preserve pitch</option><option value="mute">Mute</option>
        </select>
      </label>
    </div>
    <svg class="composition-keyframe-graph" viewBox="0 0 100 44" role="img" aria-label="Кривая speed ramp">
      <line x1="0" y1="40" x2="100" y2="40"></line><polyline points={graph}></polyline>
    </svg>
    <div class="composition-keyframe-table-wrap">
      <table class="composition-keyframe-table">
        <caption>Points в presentation-order source progress</caption>
        <thead><tr><th scope="col">Source, с</th><th scope="col">Speed</th><th scope="col">Действие</th></tr></thead>
        <tbody>
          {#each ramp.points as point, index (index)}
            <tr>
              <td><input aria-label={`Speed point ${index + 1} source`} type="number" step="0.001" value={seconds(point.sourceProgressTick)} disabled={!index || index === ramp.points.length - 1} onchange={(event) => run(() => changePoint(index, 'sourceProgressTick', Math.round(Number(event.currentTarget.value) * COMPOSITION_TIME_BASE)))} /></td>
              <td><input aria-label={`Speed point ${index + 1} speed`} type="number" min="0.05" max="16" step="0.05" value={point.speed} onchange={(event) => run(() => changePoint(index, 'speed', Number(event.currentTarget.value)))} /></td>
              <td><button class="btn ghost sm danger" type="button" aria-label={`Удалить speed point ${index + 1}`} disabled={!index || index === ramp.points.length - 1} onclick={() => run(() => commit({ ...ramp, points: ramp.points.filter((_, candidate) => candidate !== index) }))}>×</button></td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="composition-inspector-actions">
      <button class="btn ghost sm" type="button" disabled={ramp.points.length >= MAX_SPEED_RAMP_POINTS} onclick={() => run(addPoint)}>+ Point at playhead</button>
      <button class="btn ghost sm danger" type="button" onclick={() => run(() => updateCompositionSpeedRamp(clip.id, undefined))}>Убрать ramp</button>
    </div>
    <p class="composition-help">{ramp.points.length}/{MAX_SPEED_RAMP_POINTS} points · source-time и duration точны; browser playbackRate — приближённый preview. Reverse использует presentation-order ramp.</p>
    {#if capabilityReason}<p class="composition-inline-error" role="status">Экспорт: {capabilityReason}</p>{/if}
  {/if}
</fieldset>
