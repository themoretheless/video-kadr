<script lang="ts">
  import TrimSlider from './TrimSlider.svelte'
  import { detectAudibleRanges, DEFAULT_SILENCE_REMOVAL } from '$lib/audio/silence.js'
  import { localWaveformCache, type LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import type { TimelineSegment } from '$lib/types.js'
  import { shortcutAria, shortcutLabel } from '$lib/state/shortcuts.svelte.js'
  import {
    activateTimeline,
    applyAudibleTimelineRanges,
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

  interface Props { waveformCache?: Pick<LocalWaveformCache, 'load'> }
  let { waveformCache = localWaveformCache }: Props = $props()

  let duration = $derived(appState.video?.duration ?? 0)
  let segments = $derived(appState.edit.timelineSegments)
  let selectedId = $derived(appState.timelineSelectedSegmentId ?? '')
  let selectedIndex = $derived(segments.findIndex((segment) => segment.id === selectedId))
  let selected = $derived(segments[selectedIndex] ?? null)
  let totalDuration = $derived(totalTimelineDuration(segments))
  let playbackDuration = $derived(appState.edit.speed > 0 ? totalDuration / appState.edit.speed : totalDuration)
  let formatSupported = $derived(supportsTimelineFormat(appState.edit.format))
  let silenceThresholdDb = $state(DEFAULT_SILENCE_REMOVAL.thresholdDb)
  let minimumSilenceSeconds = $state(DEFAULT_SILENCE_REMOVAL.minimumSilenceSeconds)
  let silencePaddingSeconds = $state(DEFAULT_SILENCE_REMOVAL.paddingSeconds)
  let detectingSilence = $state(false)
  let silenceMessage = $state('')
  let silenceResultSignature = $state('')
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
    if (!appState.edit.timelineEnabled) appState.timelineSelectedSegmentId = null
    else if (!ids.split('|').includes(selectedId)) appState.timelineSelectedSegmentId = appState.edit.timelineSegments[0]?.id ?? null
  })

  $effect(() => {
    const signature = appState.edit.timelineSegments.map(({ start, end }) => `${start}:${end}`).join('|')
    if (silenceMessage && silenceResultSignature && signature !== silenceResultSignature) {
      silenceMessage = ''
      silenceResultSignature = ''
    }
  })

  function fmt(value: number): string {
    if (!Number.isFinite(value)) return '0:00.000'
    const minutes = Math.floor(value / 60)
    return `${minutes}:${(value - minutes * 60).toFixed(3).padStart(6, '0')}`
  }
  function selectSegment(id: string): void {
    appState.timelineSelectedSegmentId = id
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
  async function removeSilence(): Promise<void> {
    const video = appState.video
    if (!video || detectingSilence) return
    detectingSilence = true
    silenceMessage = ''
    try {
      const summary = await waveformCache.load(video.url)
      const audible = detectAudibleRanges(summary, {
        thresholdDb: silenceThresholdDb,
        minimumSilenceSeconds,
        paddingSeconds: silencePaddingSeconds,
      })
      const before = totalTimelineDuration(appState.edit.timelineSegments)
      const count = applyAudibleTimelineRanges(audible)
      const removed = Math.max(0, before - totalTimelineDuration(appState.edit.timelineSegments))
      silenceResultSignature = appState.edit.timelineSegments.map(({ start, end }) => `${start}:${end}`).join('|')
      silenceMessage = removed >= TIMELINE_MIN_SEGMENT_DURATION
        ? `Удалено ${fmt(removed)} тишины · ${count} фрагм.`
        : 'Подходящая тишина не найдена'
    } catch (error) {
      silenceMessage = error instanceof Error ? error.message : 'Не удалось проанализировать тишину'
    } finally {
      detectingSilence = false
    }
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
      <button class="btn ghost sm" type="button" aria-label="Переместить выбранный фрагмент левее" aria-keyshortcuts={shortcutAria('legacy.timelineMovePrevious')} title={`Переместить левее (${shortcutLabel('legacy.timelineMovePrevious')})`} disabled={selectedIndex <= 0} onclick={() => moveSelected(-1)}>←</button>
      <button class="btn ghost sm" type="button" aria-label="Переместить выбранный фрагмент правее" aria-keyshortcuts={shortcutAria('legacy.timelineMoveNext')} title={`Переместить правее (${shortcutLabel('legacy.timelineMoveNext')})`} disabled={selectedIndex < 0 || selectedIndex >= segments.length - 1} onclick={() => moveSelected(1)}>→</button>
      <button
        class="btn ghost sm timeline-split"
        type="button"
        disabled={!canSplit}
        aria-keyshortcuts={shortcutAria('legacy.timelineSplit')}
        title={segments.length >= MAX_TIMELINE_SEGMENTS ? `Достигнут лимит ${MAX_TIMELINE_SEGMENTS} фрагментов` : `Разделить на позиции плеера ${fmt(appState.playerTime)} (${shortcutLabel('legacy.timelineSplit')})`}
        onclick={splitSelected}
      >Разделить здесь</button>
      <button
        class="btn ghost sm"
        type="button"
        disabled={!selected || segments.length >= MAX_TIMELINE_SEGMENTS}
        aria-keyshortcuts={shortcutAria('legacy.timelineDuplicate')}
        title={segments.length >= MAX_TIMELINE_SEGMENTS ? `Достигнут лимит ${MAX_TIMELINE_SEGMENTS} фрагментов` : `Дублировать выбранный фрагмент (${shortcutLabel('legacy.timelineDuplicate')})`}
        onclick={duplicateSelected}
      >Дубль</button>
      <button class="btn ghost sm timeline-delete" type="button" disabled={segments.length <= 1} aria-keyshortcuts={shortcutAria('legacy.timelineDelete')} title={`Удалить выбранный фрагмент (${shortcutLabel('legacy.timelineDelete')})`} onclick={deleteSelected}>Удалить</button>
    </div>
    <div class="timeline-silence-tools" aria-label="Локальное удаление тишины">
      <label><span>Порог, dB</span><input type="number" min="-80" max="-6" step="1" bind:value={silenceThresholdDb} /></label>
      <label><span>Мин. пауза, с</span><input type="number" min="0.1" max="10" step="0.05" bind:value={minimumSilenceSeconds} /></label>
      <label><span>Запас, с</span><input type="number" min="0" max="2" step="0.01" bind:value={silencePaddingSeconds} /></label>
      <button class="btn ghost sm" type="button" disabled={detectingSilence || appState.video?.acodec === null} onclick={removeSilence}>
        {detectingSilence ? 'Анализ…' : 'Убрать тишину'}
      </button>
    </div>
    {#if silenceMessage}<p class="timeline-hint" role="status">{silenceMessage}</p>{/if}
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
