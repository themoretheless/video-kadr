<script lang="ts">
  import TrimSlider from './TrimSlider.svelte'
  import type { TimelineSegment } from '$lib/types.js'
  import {
    activateTimeline,
    beginEditTransaction,
    deleteTimelineSegment,
    duplicateTimelineSegment,
    endEditTransaction,
    MAX_TIMELINE_SEGMENTS,
    moveTimelineSegment,
    seekTo,
    splitTimelineSegment,
    state as appState,
    supportsTimelineFormat,
    TIMELINE_MIN_SEGMENT_DURATION,
    totalTimelineDuration,
    updateTimelineSegmentRange,
  } from '$lib/state/store.svelte.js'

  let selectedId = $state('')
  let duration = $derived(appState.video?.duration ?? 0)
  let segments = $derived(appState.edit.timelineSegments)
  let selectedIndex = $derived(segments.findIndex((segment) => segment.id === selectedId))
  let selected = $derived(segments[selectedIndex] ?? null)
  let totalDuration = $derived(totalTimelineDuration(segments))
  let playbackDuration = $derived(appState.edit.speed > 0 ? totalDuration / appState.edit.speed : totalDuration)
  let formatSupported = $derived(supportsTimelineFormat(appState.edit.format))
  let canSplit = $derived.by(() => {
    const segment = selected
    return Boolean(
      segment &&
      segments.length < MAX_TIMELINE_SEGMENTS &&
      Number.isFinite(appState.playerTime) &&
      appState.playerTime - segment.start >= TIMELINE_MIN_SEGMENT_DURATION &&
      segment.end - appState.playerTime >= TIMELINE_MIN_SEGMENT_DURATION,
    )
  })

  $effect(() => {
    const ids = appState.edit.timelineSegments.map((segment) => segment.id).join('|')
    if (!appState.edit.timelineEnabled) selectedId = ''
    else if (!ids.split('|').includes(selectedId)) selectedId = appState.edit.timelineSegments[0]?.id ?? ''
  })

  function fmt(value: number): string {
    if (!Number.isFinite(value)) return '0:00.000'
    const minutes = Math.floor(value / 60)
    return `${minutes}:${(value - minutes * 60).toFixed(3).padStart(6, '0')}`
  }
  function selectSegment(id: string): void {
    selectedId = id
    const segment = segments.find((candidate) => candidate.id === id)
    if (segment) seekTo(segment.start, segment.id)
  }
  function activate(): void {
    const id = activateTimeline()
    if (id) selectSegment(id)
  }
  function splitSelected(): void {
    if (!selected) return
    const id = splitTimelineSegment(selected.id)
    if (id) selectSegment(id)
  }
  function duplicateSelected(): void {
    if (!selected) return
    const id = duplicateTimelineSegment(selected.id)
    if (id) selectSegment(id)
  }
  function deleteSelected(): void {
    if (!selected) return
    const id = deleteTimelineSegment(selected.id)
    if (id) selectSegment(id)
  }
  function moveSelected(direction: -1 | 1): void {
    if (selected) moveTimelineSegment(selected.id, direction)
  }
  function commitInput(field: 'start' | 'end', event: Event): void {
    if (!selected) return
    const input = event.currentTarget as HTMLInputElement
    const value = input.value.trim() ? Number(input.value) : Number.NaN
    if (Number.isFinite(value)) updateTimelineSegmentRange(selected.id, { [field]: value })
    input.value = selected[field].toFixed(3)
  }
  function updateRangeDuringDrag(patch: Partial<Pick<TimelineSegment, 'start' | 'end'>>): void {
    if (!selected) return
    const id = selected.id
    appState.edit.timelineSegments = appState.edit.timelineSegments.map((segment) =>
      segment.id === id ? { ...segment, ...patch } : segment,
    )
  }
</script>

<div class:enabled={appState.edit.timelineEnabled} class="timeline-editor">
  {#if !appState.edit.timelineEnabled}
    <div class="timeline-activation">
      <div>
        <strong>Монтажная линия</strong>
        <p>Разделяй, дублируй и переставляй фрагменты одного исходника.</p>
      </div>
      <button class="btn primary sm" type="button" disabled={duration <= 0} onclick={activate}>Активировать</button>
    </div>
  {:else}
    <div class="timeline-head">
      <strong>Монтажная линия</strong>
      <span>{segments.length} фрагм. · итог {fmt(playbackDuration)}</span>
    </div>
    <div class="timeline-track" aria-label="Фрагменты монтажной линии">
      {#each segments as segment, index (segment.id)}
        <button
          class:selected={segment.id === selectedId}
          class="timeline-segment"
          style:flex-grow={Math.max(segment.end - segment.start, 0.01)}
          type="button"
          aria-label={`Фрагмент ${index + 1}: ${fmt(segment.start)} — ${fmt(segment.end)}`}
          aria-pressed={segment.id === selectedId}
          title={`Фрагмент ${index + 1}: ${fmt(segment.start)} — ${fmt(segment.end)}`}
          onclick={() => selectSegment(segment.id)}
        >
          <span>{index + 1}</span><small>{fmt(segment.end - segment.start)}</small>
        </button>
      {/each}
    </div>
    <div class="timeline-actions" aria-label="Операции с выбранным фрагментом">
      <button class="btn ghost sm" type="button" aria-label="Переместить выбранный фрагмент левее" title="Переместить левее" disabled={selectedIndex <= 0} onclick={() => moveSelected(-1)}>←</button>
      <button class="btn ghost sm" type="button" aria-label="Переместить выбранный фрагмент правее" title="Переместить правее" disabled={selectedIndex < 0 || selectedIndex >= segments.length - 1} onclick={() => moveSelected(1)}>→</button>
      <button
        class="btn ghost sm timeline-split"
        type="button"
        disabled={!canSplit}
        title={segments.length >= MAX_TIMELINE_SEGMENTS ? `Достигнут лимит ${MAX_TIMELINE_SEGMENTS} фрагментов` : `Разделить на позиции плеера ${fmt(appState.playerTime)}`}
        onclick={splitSelected}
      >Разделить здесь</button>
      <button
        class="btn ghost sm"
        type="button"
        disabled={!selected || segments.length >= MAX_TIMELINE_SEGMENTS}
        title={segments.length >= MAX_TIMELINE_SEGMENTS ? `Достигнут лимит ${MAX_TIMELINE_SEGMENTS} фрагментов` : 'Дублировать выбранный фрагмент'}
        onclick={duplicateSelected}
      >Дубль</button>
      <button class="btn ghost sm timeline-delete" type="button" disabled={segments.length <= 1} title="Удалить выбранный фрагмент" onclick={deleteSelected}>Удалить</button>
    </div>
    {#if selected}
      <div class="timeline-range-editor">
        <TrimSlider
          min={0}
          max={duration}
          start={selected.start}
          end={selected.end}
          step={0.01}
          gap={TIMELINE_MIN_SEGMENT_DURATION}
          oninteractionstart={() => beginEditTransaction('timeline-range-drag')}
          oninteractionend={endEditTransaction}
          onstartchange={(start) => updateRangeDuringDrag({ start })}
          onendchange={(end) => updateRangeDuringDrag({ end })}
        />
        <div class="timeline-inputs">
          <label><span>Начало, с</span><input type="number" min="0" max={selected.end - TIMELINE_MIN_SEGMENT_DURATION} step="0.001" value={selected.start} onchange={(event) => commitInput('start', event)} /></label>
          <span class="timeline-range-duration">{fmt(selected.end - selected.start)}</span>
          <label><span>Конец, с</span><input type="number" min={selected.start + TIMELINE_MIN_SEGMENT_DURATION} max={duration} step="0.001" value={selected.end} onchange={(event) => commitInput('end', event)} /></label>
        </div>
      </div>
    {/if}
    {#if !formatSupported}
      <p class="timeline-warning" role="status">Порядок фрагментов экспортируется в MP4, WebM, AV1 и ProRes. Выбери один из этих форматов.</p>
    {:else}
      <p class="timeline-hint">Порядок блоков — порядок воспроизведения. Разделение использует текущую позицию плеера.</p>
    {/if}
  {/if}
</div>
