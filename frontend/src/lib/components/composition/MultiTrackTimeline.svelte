<script lang="ts">
  import AudioWaveform from '$lib/audio/AudioWaveform.svelte'
  import type { LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import { clipDurationTicks, clipEndTicks, COMPOSITION_TIME_BASE, type CompositionClip } from '$lib/composition/types.js'
  import { MAX_COMPOSITION_MARKERS } from '$lib/composition/markers.js'
  import { shortcutAria, shortcutLabel } from '$lib/state/shortcuts.svelte.js'
  import {
    compositionDuration,
    compositionMarkerList,
    compositionState,
    addCompositionTrack,
    addCompositionMarkerAtPlayhead,
    deleteSelectedCompositionClip,
    duplicateSelectedCompositionClip,
    moveCompositionClip,
    magnetizeCompositionTrack,
    reorderCompositionTrack,
    selectCompositionClip,
    setCompositionPlayhead,
    setCompositionZoom,
    splitSelectedCompositionClip,
    toggleCompositionSnapping,
    toggleCompositionTrackFlag,
    trimCompositionClip,
    undoComposition,
    redoComposition,
    removeCompositionMarker,
    rippleDeleteSelectedCompositionClip,
    seekCompositionMarker,
    updateCompositionMarker,
  } from '$lib/state/composition.svelte.js'

  type GestureMode = 'move' | 'trim-start' | 'trim-end'
  interface Props { waveformCache?: LocalWaveformCache }
  interface Gesture {
    clipId: string
    trackId: string
    mode: GestureMode
    pointerStartX: number
    originalStart: number
    originalEnd: number
  }
  interface PreviewRange { clipId: string; start: number; end: number }

  let { waveformCache }: Props = $props()

  let gesture = $state<Gesture | null>(null)
  let previewRange = $state<PreviewRange | null>(null)
  const markers = $derived(compositionMarkerList())
  const timelineExtent = $derived(Math.max(compositionDuration(), markers.at(-1)?.tick ?? 0))
  const contentWidth = $derived(
    Math.max(760, (timelineExtent / COMPOSITION_TIME_BASE) * compositionState.ui.zoomPxPerSecond + 160),
  )
  const rulerMarks = $derived(
    Array.from({ length: Math.ceil(contentWidth / compositionState.ui.zoomPxPerSecond) + 1 }, (_, index) => index),
  )

  function run(action: () => void): void {
    try {
      compositionState.ui.message = ''
      action()
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
    }
  }

  function rangeFor(clip: CompositionClip): { start: number; end: number } {
    return previewRange?.clipId === clip.id
      ? previewRange
      : { start: clip.timelineStartTicks, end: clipEndTicks(clip) }
  }

  function leftPx(clip: CompositionClip): number {
    return ticksToPx(rangeFor(clip).start)
  }

  function widthPx(clip: CompositionClip): number {
    const range = rangeFor(clip)
    return Math.max(12, ticksToPx(range.end - range.start))
  }

  function ticksToPx(ticks: number): number {
    return (ticks / COMPOSITION_TIME_BASE) * compositionState.ui.zoomPxPerSecond
  }

  function pxToTicks(px: number): number {
    return Math.round((px / compositionState.ui.zoomPxPerSecond) * COMPOSITION_TIME_BASE)
  }

  function beginGesture(event: PointerEvent, clip: CompositionClip, trackId: string, mode: GestureMode): void {
    event.preventDefault()
    event.stopPropagation()
    const track = compositionState.document.tracks.find((candidate) => candidate.id === trackId)
    if (track?.locked) return
    selectCompositionClip(trackId, clip.id)
    gesture = {
      clipId: clip.id,
      trackId,
      mode,
      pointerStartX: event.clientX,
      originalStart: clip.timelineStartTicks,
      originalEnd: clipEndTicks(clip),
    }
    previewRange = { clipId: clip.id, start: clip.timelineStartTicks, end: clipEndTicks(clip) }
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function moveGesture(event: PointerEvent): void {
    if (!gesture || !previewRange) return
    const delta = pxToTicks(event.clientX - gesture.pointerStartX)
    const minimum = Math.max(1, Math.round(COMPOSITION_TIME_BASE / 100))
    if (gesture.mode === 'move') {
      const duration = gesture.originalEnd - gesture.originalStart
      const start = Math.max(0, gesture.originalStart + delta)
      previewRange = { ...previewRange, start, end: start + duration }
    } else if (gesture.mode === 'trim-start') {
      previewRange = {
        ...previewRange,
        start: Math.max(0, Math.min(gesture.originalEnd - minimum, gesture.originalStart + delta)),
      }
    } else {
      previewRange = {
        ...previewRange,
        end: Math.max(gesture.originalStart + minimum, gesture.originalEnd + delta),
      }
    }
  }

  function endGesture(event: PointerEvent): void {
    if (!gesture || !previewRange) return
    const current = gesture
    const range = previewRange
    gesture = null
    previewRange = null
    run(() => {
      if (current.mode === 'move') {
        const underPointer = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-track-id]')
        const targetTrackId = underPointer?.dataset.trackId ?? current.trackId
        moveCompositionClip(current.clipId, targetTrackId, range.start)
      } else {
        trimCompositionClip(current.clipId, range.start, range.end)
      }
    })
  }

  function seekOnLane(event: MouseEvent): void {
    const lane = event.currentTarget as HTMLElement
    const bounds = lane.getBoundingClientRect()
    setCompositionPlayhead(pxToTicks(Math.max(0, event.clientX - bounds.left)))
  }

  function seekWithKeyboard(event: KeyboardEvent): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    const direction = event.key === 'ArrowLeft' ? -1 : 1
    setCompositionPlayhead(compositionState.transport.playheadTicks + direction * COMPOSITION_TIME_BASE / 10)
  }

  function clipName(clip: CompositionClip): string {
    if (clip.kind === 'text') return clip.text
    return compositionState.media[clip.sourceId]?.filename ?? clip.sourceId
  }

  function formatShort(ticks: number): string {
    return `${(ticks / COMPOSITION_TIME_BASE).toFixed(1)}s`
  }

  function markerSeconds(event: Event): number {
    return Math.max(0, Math.round(Number((event.currentTarget as HTMLInputElement).value) * COMPOSITION_TIME_BASE))
  }

  function onClipKey(event: KeyboardEvent, trackId: string, clipId: string): void {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId)
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId)
      run(deleteSelectedCompositionClip)
    }
  }

  function onTrimKey(event: KeyboardEvent, trackId: string, clip: CompositionClip, edge: 'start' | 'end'): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    event.stopPropagation()
    selectCompositionClip(trackId, clip.id)
    const frame = Math.max(1, Math.round(COMPOSITION_TIME_BASE / (compositionState.document.canvas.fps || 30)))
    const delta = event.key === 'ArrowLeft' ? -frame : frame
    run(() => trimCompositionClip(
      clip.id,
      edge === 'start' ? clip.timelineStartTicks + delta : clip.timelineStartTicks,
      edge === 'end' ? clipEndTicks(clip) + delta : clipEndTicks(clip),
      false,
    ))
  }
</script>

<section class="composition-timeline card" aria-label="Многодорожечная монтажная линия">
  <div class="composition-timeline-toolbar">
    <strong>Монтажная линия</strong>
    <button class="btn ghost sm" type="button" onclick={undoComposition} disabled={!compositionState.history.past.length} aria-label="Отменить" aria-keyshortcuts={shortcutAria('composition.undo')} title={`Отменить (${shortcutLabel('composition.undo')})`}>↶</button>
    <button class="btn ghost sm" type="button" onclick={redoComposition} disabled={!compositionState.history.future.length} aria-label="Повторить" aria-keyshortcuts={shortcutAria('composition.redo')} title={`Повторить (${shortcutLabel('composition.redo')})`}>↷</button>
    <button class="btn ghost sm" type="button" onclick={() => run(splitSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} aria-keyshortcuts={shortcutAria('composition.split')} title={`Разрезать (${shortcutLabel('composition.split')})`}>Разрезать</button>
    <button class="btn ghost sm" type="button" onclick={() => run(duplicateSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} aria-keyshortcuts={shortcutAria('composition.duplicate')} title={`Дублировать (${shortcutLabel('composition.duplicate')})`}>Дубликат</button>
    <button class="btn ghost sm danger" type="button" onclick={() => run(deleteSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} aria-keyshortcuts={shortcutAria('composition.delete')} title={`Удалить (${shortcutLabel('composition.delete')})`}>Удалить</button>
    <button class="btn ghost sm danger" type="button" onclick={() => run(rippleDeleteSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} title="Удалить выбранный clip и сдвинуть только последующие clips этой дорожки">Ripple delete</button>
    <button class="btn ghost sm" type="button" onclick={() => run(addCompositionMarkerAtPlayhead)} disabled={markers.length >= MAX_COMPOSITION_MARKERS} title={`Добавить marker на playhead (${markers.length}/${MAX_COMPOSITION_MARKERS})`}>+ Marker</button>
    <span class="composition-toolbar-separator" aria-hidden="true"></span>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('video'))}>+ Video track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('audio'))}>+ Audio track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('image'))}>+ Image track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('text'))}>+ Text track</button>
    <label class="composition-snap-toggle"><input type="checkbox" checked={compositionState.ui.snapEnabled} onchange={toggleCompositionSnapping} /> магнит</label>
    <label class="composition-zoom">масштаб <input type="range" min="24" max="320" value={compositionState.ui.zoomPxPerSecond} oninput={(event) => setCompositionZoom(Number(event.currentTarget.value))} /></label>
  </div>

  {#if compositionState.ui.message}<p class="composition-inline-error" role="alert">{compositionState.ui.message}</p>{/if}

  {#if markers.length}
    <div class="composition-markers-panel" aria-label="Маркеры композиции">
      <table class="composition-markers-table">
        <caption>Markers {markers.length} / {MAX_COMPOSITION_MARKERS}</caption>
        <thead><tr><th scope="col">Цвет</th><th scope="col">Метка</th><th scope="col">Время, с</th><th scope="col"><span class="visually-hidden">Действия</span></th></tr></thead>
        <tbody>
          {#each markers as marker (marker.id)}
            <tr class:selected={compositionState.ui.selectedMarkerId === marker.id}>
              <td><input aria-label={`Цвет marker ${marker.label}`} type="color" value={marker.color ?? '#f59e0b'} onchange={(event) => run(() => updateCompositionMarker(marker.id, { color: event.currentTarget.value }))} /></td>
              <td><input aria-label={`Метка marker ${formatShort(marker.tick)}`} type="text" maxlength="128" value={marker.label} onchange={(event) => run(() => updateCompositionMarker(marker.id, { label: event.currentTarget.value }))} /></td>
              <td><input aria-label={`Время marker ${marker.label}`} type="number" min="0" max="86400" step="0.001" value={marker.tick / COMPOSITION_TIME_BASE} onchange={(event) => run(() => updateCompositionMarker(marker.id, { tick: markerSeconds(event) }))} /></td>
              <td class="composition-marker-actions">
                <button class="btn ghost sm" type="button" onclick={() => run(() => seekCompositionMarker(marker.id))}>Перейти</button>
                <button class="btn ghost sm danger" type="button" aria-label={`Удалить marker ${marker.label}`} onclick={() => run(() => removeCompositionMarker(marker.id))}>×</button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}

  <div class="composition-timeline-scroll">
    <div class="composition-ruler-row">
      <div class="composition-track-spacer"></div>
      <div
        class="composition-ruler"
        style:width={`${contentWidth}px`}
        role="slider"
        tabindex="0"
        aria-label="Шкала времени"
        aria-valuemin="0"
        aria-valuemax={timelineExtent}
        aria-valuenow={compositionState.transport.playheadTicks}
        onclick={seekOnLane}
        onkeydown={seekWithKeyboard}
      >
        {#each rulerMarks as second (second)}
          <span style:left={`${second * compositionState.ui.zoomPxPerSecond}px`}>{second}s</span>
        {/each}
        {#each markers as marker (marker.id)}
          <button
            class="composition-marker-handle"
            class:selected={compositionState.ui.selectedMarkerId === marker.id}
            type="button"
            style:left={`${ticksToPx(marker.tick)}px`}
            style:--marker-color={marker.color ?? '#f59e0b'}
            aria-label={`Marker ${marker.label}, ${formatShort(marker.tick)}`}
            title={`${marker.label} · ${formatShort(marker.tick)}`}
            onclick={(event) => { event.stopPropagation(); run(() => seekCompositionMarker(marker.id)) }}
          ></button>
        {/each}
      </div>
    </div>

    {#each compositionState.document.tracks as track, index (track.id)}
      <div class="composition-track-row" class:locked={track.locked} data-track-id={track.id}>
        <div class="composition-track-head">
          <div class="composition-track-title"><span class={`composition-kind ${track.kind}`}>{track.kind}</span><strong>{track.name}</strong></div>
          <div class="composition-track-actions">
            <button class="mini-icon" type="button" disabled={index === 0} onclick={() => run(() => reorderCompositionTrack(track.id, index - 1))} aria-label={`Переместить дорожку «${track.name}» выше`} aria-keyshortcuts={compositionState.ui.selectedTrackId === track.id ? shortcutAria('composition.trackUp') : undefined} title={`Выше (${shortcutLabel('composition.trackUp')})`}>↑</button>
            <button class="mini-icon" type="button" disabled={index === compositionState.document.tracks.length - 1} onclick={() => run(() => reorderCompositionTrack(track.id, index + 1))} aria-label={`Переместить дорожку «${track.name}» ниже`} aria-keyshortcuts={compositionState.ui.selectedTrackId === track.id ? shortcutAria('composition.trackDown') : undefined} title={`Ниже (${shortcutLabel('composition.trackDown')})`}>↓</button>
            <button class="mini-icon" type="button" disabled={track.locked || track.clips.length < 2} onclick={() => run(() => magnetizeCompositionTrack(track.id))} aria-label={`Собрать gaps дорожки «${track.name}»`} title={track.locked ? 'Сначала разблокируйте дорожку' : 'Однократно собрать clips от начала дорожки'}>🧲</button>
            {#if 'muted' in track}<button class:active={track.muted} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'muted'))} aria-label={`${track.muted ? 'Включить' : 'Выключить'} звук дорожки «${track.name}»`} title="Звук">{track.muted ? '🔇' : '🔊'}</button>{/if}
            {#if track.kind === 'audio'}<button class:active={track.solo ?? false} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'solo'))} aria-label={`${track.solo ? 'Отключить solo' : 'Включить solo'} дорожки «${track.name}»`} title="Solo">S</button>{/if}
            {#if 'hidden' in track}<button class:active={track.hidden} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'hidden'))} aria-label={`${track.hidden ? 'Показать' : 'Скрыть'} дорожку «${track.name}»`} title="Видимость">{track.hidden ? '◌' : '◉'}</button>{/if}
            <button class:active={track.locked} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'locked'))} aria-label={`${track.locked ? 'Разблокировать' : 'Заблокировать'} дорожку «${track.name}»`} title="Блокировка">{track.locked ? '🔒' : '🔓'}</button>
          </div>
        </div>
        <div
          class="composition-track-lane"
          style:width={`${contentWidth}px`}
          role="slider"
          tabindex="0"
          aria-label={`Плейхед на дорожке ${track.name}`}
          aria-valuemin="0"
          aria-valuemax={compositionDuration()}
          aria-valuenow={compositionState.transport.playheadTicks}
          onclick={seekOnLane}
          onkeydown={seekWithKeyboard}
        >
          {#each track.clips as clip (clip.id)}
            <div
              class={`composition-clip ${clip.kind}`}
              class:selected={compositionState.ui.selectedClipId === clip.id}
              class:dragging={gesture?.clipId === clip.id}
              style:left={`${leftPx(clip)}px`}
              style:width={`${widthPx(clip)}px`}
              role="button"
              tabindex="0"
              aria-label={`${clipName(clip)}, ${formatShort(clipDurationTicks(clip))}`}
              onpointerdown={(event) => beginGesture(event, clip, track.id, 'move')}
              onpointermove={moveGesture}
              onpointerup={endGesture}
              onpointercancel={() => { gesture = null; previewRange = null }}
              onclick={(event) => event.stopPropagation()}
              onkeydown={(event) => onClipKey(event, track.id, clip.id)}
            >
              <button type="button" class="composition-trim-handle left" aria-label={`Подрезать начало «${clipName(clip)}»`} title="Стрелки изменяют границу на один кадр" onpointerdown={(event) => beginGesture(event, clip, track.id, 'trim-start')} onkeydown={(event) => onTrimKey(event, track.id, clip, 'start')}></button>
              {#if clip.kind === 'audio' && compositionState.media[clip.sourceId]}
                <span class="composition-clip-waveform">
                  <AudioWaveform
                    url={compositionState.media[clip.sourceId]!.url}
                    sourceInSeconds={clip.sourceInTicks / COMPOSITION_TIME_BASE}
                    sourceOutSeconds={clip.sourceOutTicks / COMPOSITION_TIME_BASE}
                    label={`Форма волны ${clipName(clip)}`}
                    cache={waveformCache}
                  />
                </span>
              {/if}
              <span class="composition-clip-label">{clipName(clip)}</span>
              <small>{formatShort(clipDurationTicks(clip))}</small>
              <button type="button" class="composition-trim-handle right" aria-label={`Подрезать конец «${clipName(clip)}»`} title="Стрелки изменяют границу на один кадр" onpointerdown={(event) => beginGesture(event, clip, track.id, 'trim-end')} onkeydown={(event) => onTrimKey(event, track.id, clip, 'end')}></button>
            </div>
          {/each}
          {#if track.kind === 'video'}
            {#each track.transitions ?? [] as transition (transition.id)}
              {@const incoming = track.clips.find((clip) => clip.id === transition.toClipId)}
              {#if incoming}
                <button
                  type="button"
                  class="composition-transition-handle"
                  style:left={`${ticksToPx(incoming.timelineStartTicks - Math.floor(transition.durationTicks / 2))}px`}
                  style:width={`${Math.max(18, ticksToPx(transition.durationTicks))}px`}
                  aria-label={`Переход ${transition.kind}, ${formatShort(transition.durationTicks)}, к ${clipName(incoming)}`}
                  title={`${transition.kind} · ${formatShort(transition.durationTicks)} · source handles`}
                  onclick={(event) => { event.stopPropagation(); selectCompositionClip(track.id, incoming.id) }}
                >⇄</button>
              {/if}
            {/each}
          {/if}
          <div class="composition-playhead" style:left={`${ticksToPx(compositionState.transport.playheadTicks)}px`}></div>
        </div>
      </div>
    {:else}
      <div class="composition-no-tracks">Добавьте медиа или текст — дорожка создастся автоматически.</div>
    {/each}
  </div>
</section>
