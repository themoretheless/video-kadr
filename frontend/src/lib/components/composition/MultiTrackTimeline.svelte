<script lang="ts">
  import { tick } from 'svelte'
  import type { ReviewThreadDto } from '$lib/api.js'
  import AudioWaveform from '$lib/audio/AudioWaveform.svelte'
  import type { LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import { clipDurationTicks, clipEndTicks, COMPOSITION_TIME_BASE, type CompositionClip, type CompositionTrack } from '$lib/composition/types.js'
  import { MAX_COMPOSITION_MARKERS } from '$lib/composition/markers.js'
  import { COMPOSITION_TOOLBAR_COMMANDS, compositionCommandState, executeCompositionCommand } from '$lib/composition/commandRegistry.js'
  import { captureScrollAnchor, restoreScrollAnchor, timelineViewport, visibleRulerMarks, visibleTimelineItems } from '$lib/composition/timelineUi.js'
  import { shortcutDefinition } from '$lib/shortcuts.js'
  import { shortcutAria, shortcutLabel } from '$lib/state/shortcuts.svelte.js'
  import TimelineSemanticList from './TimelineSemanticList.svelte'
  import {
    compositionDuration,
    compositionMarkerList,
    compositionState,
    clearCompositionExportRange,
    addCompositionTrack,
    addCompositionMarkerAtPlayhead,
    moveCompositionClip,
    magnetizeCompositionTrack,
    reorderCompositionTrack,
    selectCompositionClip,
    setCompositionPlayhead,
    setCompositionExportRangePoint,
    setCompositionZoom,
    slipCompositionClip,
    toggleCompositionSnapping,
    toggleCompositionTrackFlag,
    trimCompositionClip,
    removeCompositionMarker,
    rippleDeleteSelectedCompositionClip,
    seekCompositionMarker,
    updateCompositionMarker,
  } from '$lib/state/composition.svelte.js'

  type GestureMode = 'move' | 'slip' | 'trim-start' | 'trim-end'
  interface Props {
    waveformCache?: LocalWaveformCache
    reviewThreads?: ReviewThreadDto[]
  }
  interface Gesture {
    clipId: string
    trackId: string
    mode: GestureMode
    pointerStartX: number
    originalStart: number
    originalEnd: number
  }
  interface PreviewRange { clipId: string; start: number; end: number }

  let { waveformCache, reviewThreads = [] }: Props = $props()

  let gesture = $state<Gesture | null>(null)
  let previewRange = $state<PreviewRange | null>(null)
  let slipDelta = $state(0)
  let timelineScroll = $state<HTMLDivElement>()
  let scrollLeft = $state(0)
  let viewportWidth = $state(1_200)
  let semanticListOpen = $state(false)
  let commandPaletteOpen = $state(false)
  let toolbarFocusIndex = $state(0)
  let paletteFocusIndex = $state(0)
  const markers = $derived(compositionMarkerList())
  const reviewTicks = $derived(reviewThreads.map((thread) => thread.comments[0]?.timelineTick ?? 0))
  const timelineExtent = $derived(Math.max(compositionDuration(), markers.at(-1)?.tick ?? 0, ...reviewTicks))
  const contentWidth = $derived(
    Math.max(760, (timelineExtent / COMPOSITION_TIME_BASE) * compositionState.ui.zoomPxPerSecond + 160),
  )
  const viewport = $derived(timelineViewport(scrollLeft, viewportWidth, contentWidth))
  const rulerMarks = $derived(visibleRulerMarks(viewport, compositionState.ui.zoomPxPerSecond))
  const toolbarCommands = $derived(COMPOSITION_TOOLBAR_COMMANDS.map((id) => ({ id, ...compositionCommandState(id) })))

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
    const resolvedMode = mode === 'move' && event.altKey && (clip.kind === 'video' || clip.kind === 'audio') ? 'slip' : mode
    gesture = {
      clipId: clip.id,
      trackId,
      mode: resolvedMode,
      pointerStartX: event.clientX,
      originalStart: clip.timelineStartTicks,
      originalEnd: clipEndTicks(clip),
    }
    previewRange = { clipId: clip.id, start: clip.timelineStartTicks, end: clipEndTicks(clip) }
    slipDelta = 0
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function moveGesture(event: PointerEvent): void {
    if (!gesture || !previewRange) return
    const delta = pxToTicks(event.clientX - gesture.pointerStartX)
    const minimum = Math.max(1, Math.round(COMPOSITION_TIME_BASE / 100))
    if (gesture.mode === 'slip') {
      slipDelta = delta
    } else if (gesture.mode === 'move') {
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
      if (current.mode === 'slip') {
        slipCompositionClip(current.clipId, slipDelta)
      } else if (current.mode === 'move') {
        const underPointer = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-track-id]')
        const targetTrackId = underPointer?.dataset.trackId ?? current.trackId
        moveCompositionClip(current.clipId, targetTrackId, range.start)
      } else {
        trimCompositionClip(current.clipId, range.start, range.end)
      }
    })
    slipDelta = 0
    const target = event.currentTarget as HTMLElement
    if (target.hasPointerCapture?.(event.pointerId)) target.releasePointerCapture(event.pointerId)
  }

  function cancelGesture(event: PointerEvent): void {
    gesture = null
    previewRange = null
    slipDelta = 0
    const target = event.currentTarget as HTMLElement
    if (target.hasPointerCapture?.(event.pointerId)) target.releasePointerCapture(event.pointerId)
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

  function onExportRangeShortcut(event: KeyboardEvent): void {
    if (event.defaultPrevented || event.metaKey || event.ctrlKey || event.altKey) return
    const target = event.target as HTMLElement | null
    if (target?.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target?.tagName ?? '')) return
    const key = event.key.toLowerCase()
    if (key !== 'i' && key !== 'o') return
    event.preventDefault()
    run(() => setCompositionExportRangePoint(key === 'i' ? 'in' : 'out'))
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

  function onClipKey(event: KeyboardEvent, trackId: string, clip: CompositionClip): void {
    const clipId = clip.id
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId)
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId)
      executeCompositionCommand('composition.delete')
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId)
      const frame = Math.max(1, Math.round(COMPOSITION_TIME_BASE / (compositionState.document.canvas.fps || 30)))
      const delta = event.key === 'ArrowLeft' ? -frame : frame
      if (event.shiftKey) run(() => slipCompositionClip(clipId, delta))
      else run(() => moveCompositionClip(clipId, trackId, Math.max(0, clip.timelineStartTicks + delta), false))
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

  function visibleClips(track: CompositionTrack): readonly CompositionClip[] {
    return visibleTimelineItems(
      track.clips as readonly CompositionClip[],
      viewport,
      (clip) => ticksToPx(clip.timelineStartTicks),
      (clip) => ticksToPx(clipEndTicks(clip)),
    )
  }

  function updateViewport(event: Event): void {
    const element = event.currentTarget as HTMLDivElement
    scrollLeft = element.scrollLeft
    viewportWidth = element.clientWidth || viewportWidth
  }

  async function updateZoom(event: Event): Promise<void> {
    const anchor = captureScrollAnchor(scrollLeft, compositionState.ui.zoomPxPerSecond, COMPOSITION_TIME_BASE)
    setCompositionZoom(Number((event.currentTarget as HTMLInputElement).value))
    await tick()
    if (timelineScroll) {
      timelineScroll.scrollLeft = restoreScrollAnchor(anchor, compositionState.ui.zoomPxPerSecond, COMPOSITION_TIME_BASE)
      scrollLeft = timelineScroll.scrollLeft
    }
  }

  function onToolbarKeydown(event: KeyboardEvent): void {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return
    const toolbar = event.currentTarget as HTMLElement
    const buttons = [...toolbar.querySelectorAll<HTMLButtonElement>('[data-timeline-command]')]
    if (!buttons.length) return
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (Math.max(0, current) + (event.key === 'ArrowLeft' ? -1 : 1) + buttons.length) % buttons.length
    event.preventDefault()
    toolbarFocusIndex = next
    buttons[next]!.focus()
  }

  async function toggleCommandPalette(): Promise<void> {
    commandPaletteOpen = !commandPaletteOpen
    if (!commandPaletteOpen) return
    paletteFocusIndex = 0
    await tick()
    document.querySelector<HTMLButtonElement>('#timeline-command-palette [role="menuitem"]')?.focus()
  }

  function onPaletteKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') {
      event.preventDefault()
      commandPaletteOpen = false
      return
    }
    if (!['ArrowUp', 'ArrowDown', 'Home', 'End'].includes(event.key)) return
    const menu = event.currentTarget as HTMLElement
    const items = [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]')]
    if (!items.length) return
    const current = items.indexOf(document.activeElement as HTMLButtonElement)
    paletteFocusIndex = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (Math.max(0, current) + (event.key === 'ArrowUp' ? -1 : 1) + items.length) % items.length
    event.preventDefault()
    items[paletteFocusIndex]!.focus()
  }

  function executePaletteCommand(command: (typeof COMPOSITION_TOOLBAR_COMMANDS)[number]): void {
    executeCompositionCommand(command)
    commandPaletteOpen = false
  }
</script>

<svelte:window onkeydown={onExportRangeShortcut} />

<section class="composition-timeline card" aria-label="Многодорожечная монтажная линия">
  <div class="composition-timeline-toolbar" role="toolbar" tabindex="-1" aria-label="Команды монтажной линии" onkeydown={onToolbarKeydown}>
    <strong>Монтажная линия</strong>
    {#each toolbarCommands as command, index (command.id)}
      <button
        class:danger={command.id === 'composition.delete'}
        class="btn ghost sm"
        type="button"
        data-timeline-command={command.id}
        tabindex={index === toolbarFocusIndex ? 0 : -1}
        aria-disabled={!command.enabled}
        aria-label={shortcutDefinition(command.id).label}
        aria-keyshortcuts={shortcutAria(command.id)}
        aria-describedby={!command.enabled ? `timeline-command-reason-${index}` : undefined}
        title={`${command.disabledReason ?? shortcutDefinition(command.id).description} (${shortcutLabel(command.id)})`}
        onfocus={() => { toolbarFocusIndex = index }}
        onclick={() => command.enabled && executeCompositionCommand(command.id)}
      >{command.id === 'composition.undo' ? '↶' : command.id === 'composition.redo' ? '↷' : shortcutDefinition(command.id).label}</button>
      {#if !command.enabled}<span id={`timeline-command-reason-${index}`} class="visually-hidden">{command.disabledReason}</span>{/if}
    {/each}
    <button class="btn ghost sm danger" type="button" onclick={() => run(rippleDeleteSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} title="Удалить выбранный clip и сдвинуть только последующие clips этой дорожки">Ripple delete</button>
    <button class="btn ghost sm" type="button" onclick={() => run(addCompositionMarkerAtPlayhead)} disabled={markers.length >= MAX_COMPOSITION_MARKERS} title={`Добавить marker на playhead (${markers.length}/${MAX_COMPOSITION_MARKERS})`}>+ Marker</button>
    <button class="btn ghost sm" type="button" aria-keyshortcuts="I" onclick={() => run(() => setCompositionExportRangePoint('in'))}>In · I</button>
    <button class="btn ghost sm" type="button" aria-keyshortcuts="O" onclick={() => run(() => setCompositionExportRangePoint('out'))}>Out · O</button>
    <button class="btn ghost sm" type="button" disabled={compositionState.export.rangeInTicks === null && compositionState.export.rangeOutTicks === null} onclick={clearCompositionExportRange}>Очистить In/Out</button>
    <span class="composition-toolbar-separator" aria-hidden="true"></span>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('video'))}>+ Video track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('audio'))}>+ Audio track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('image'))}>+ Image track</button>
    <button class="btn ghost sm" type="button" onclick={() => run(() => addCompositionTrack('text'))}>+ Text track</button>
    <label class="composition-snap-toggle"><input type="checkbox" checked={compositionState.ui.snapEnabled} onchange={toggleCompositionSnapping} /> магнит</label>
    <button class="btn ghost sm" type="button" aria-expanded={semanticListOpen} aria-controls="timeline-semantic-list" onclick={() => { semanticListOpen = !semanticListOpen }}>Список для ассистивных технологий</button>
    <button class="btn ghost sm" type="button" aria-haspopup="menu" aria-expanded={commandPaletteOpen} aria-controls="timeline-command-palette" onclick={() => void toggleCommandPalette()}>Команды</button>
    <label class="composition-zoom">масштаб <input type="range" min="24" max="320" value={compositionState.ui.zoomPxPerSecond} oninput={(event) => void updateZoom(event)} /></label>
  </div>

  {#if commandPaletteOpen}
    <div id="timeline-command-palette" class="timeline-command-palette" role="menu" tabindex="-1" aria-label="Команды монтажной линии" onkeydown={onPaletteKeydown}>
      {#each toolbarCommands as command, index (command.id)}
        <button
          type="button"
          role="menuitem"
          tabindex={index === paletteFocusIndex ? 0 : -1}
          aria-disabled={!command.enabled}
          title={command.disabledReason}
          onfocus={() => { paletteFocusIndex = index }}
          onclick={() => command.enabled && executePaletteCommand(command.id)}
        >{shortcutDefinition(command.id).label}<span>{shortcutLabel(command.id)}</span></button>
      {/each}
    </div>
  {/if}

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

  {#if semanticListOpen}
    <div id="timeline-semantic-list">
      <TimelineSemanticList tracks={compositionState.document.tracks} selectedClipId={compositionState.ui.selectedClipId} clipLabel={clipName} onselect={selectCompositionClip} />
    </div>
  {/if}

  <div class="composition-timeline-scroll" bind:this={timelineScroll} onscroll={updateViewport}>
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
        {#if compositionState.export.rangeInTicks !== null && compositionState.export.rangeOutTicks !== null && compositionState.export.rangeInTicks < compositionState.export.rangeOutTicks}
          <div
            class="composition-export-range"
            style:left={`${ticksToPx(compositionState.export.rangeInTicks)}px`}
            style:width={`${ticksToPx(compositionState.export.rangeOutTicks - compositionState.export.rangeInTicks)}px`}
            aria-hidden="true"
          ></div>
        {/if}
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
        {#each reviewThreads as thread (thread.id)}
          {@const reviewTick = thread.comments[0]?.timelineTick ?? 0}
          <button
            class="composition-review-handle"
            class:resolved={thread.resolvedAt != null}
            type="button"
            style:left={`${ticksToPx(reviewTick)}px`}
            aria-label={`Review ${thread.resolvedAt == null ? 'открыто' : 'закрыто'}, ${formatShort(reviewTick)}`}
            title={`${thread.comments[0]?.body ?? 'Review'} · ${formatShort(reviewTick)}`}
            onclick={(event) => { event.stopPropagation(); setCompositionPlayhead(reviewTick) }}
          >◆</button>
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
          {#each visibleClips(track) as clip (clip.id)}
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
              onpointercancel={cancelGesture}
              onlostpointercapture={cancelGesture}
              onclick={(event) => event.stopPropagation()}
              onkeydown={(event) => onClipKey(event, track.id, clip)}
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
