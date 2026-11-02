<script lang="ts">
  import {transitionSourceSpan} from '$lib/composition/transitionTimeline'
  import {activityReveal} from '$lib/agent/activityReveal.svelte'
  import { tick } from 'svelte'
  import { marqueeRect, marqueeScrollStep, selectMarqueeClips, selectMarqueeKeyframes, type MarqueeKeyframeBounds, type MarqueeClipBounds, type MarqueeRect } from '$lib/composition/marquee.js'
  import { canvasTimeGrid, stepTimeGridTicks } from '$lib/composition/timeGrid.js'
  import { findClipLocation, pasteClips, moveClipsToNewTrack } from '$lib/composition/commands.js'
  import type { ReviewThreadDto } from '$lib/api.js'
  import AudioWaveform from '$lib/audio/AudioWaveform.svelte'
  import type { LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import { clipDurationTicks, clipEndTicks, COMPOSITION_TIME_BASE, type CompositionClip, type CompositionTrack } from '$lib/composition/types.js'
  import { MAX_COMPOSITION_MARKERS } from '$lib/composition/markers.js'
  import { rippleDeleteClips, rippleDeleteGap } from '$lib/composition/timelineProductivity.js'
  import { compositionLinkAccent, compositionLinkToggleState } from '$lib/composition/links.js'
  import { COMPOSITION_TOOLBAR_COMMANDS, compositionCommandState, executeCompositionCommand } from '$lib/composition/commandRegistry.js'
  import { captureScrollAnchor, restoreScrollAnchor, timelineViewport, visibleRulerMarks, visibleTimelineItems } from '$lib/composition/timelineUi.js'
  import { shortcutDefinition } from '$lib/shortcuts.js'
  import { shortcutAria, shortcutLabel } from '$lib/state/shortcuts.svelte.js'
  import { keyframeChannels } from '$lib/composition/keyframeChannels'
  import TimelineKeyframes from './TimelineKeyframes.svelte'
  import TimelineColorKeys from './TimelineColorKeys.svelte'
  import TimelineSemanticList from './TimelineSemanticList.svelte'
  import {
    compositionDuration,
    compositionMarkerList,
    compositionState,
    deleteSelectedCompositionClip,
    toggleSelectedCompositionClipsEnabled,
    clearCompositionExportRange,
    addCompositionTrack,
    addCompositionMarkerAtPlayhead,
    moveCompositionClip,
    planCompositionClipMove,
    duplicateSelectedCompositionClip,
    raiseSelectedCompositionClips,
    closeCompositionGapAtPlayhead,
    closeSelectedCompositionGap,
    selectCompositionGap,
    selectAdjacentCompositionGap,
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

  type GestureMode = 'move' | 'duplicate' | 'slip' | 'trim-start' | 'trim-end'
  interface Props {
    waveformCache?: LocalWaveformCache
    reviewThreads?: ReviewThreadDto[]
  }
  interface Gesture {
    clipId: string
    trackId: string
    mode: GestureMode
    pointerStartX: number
    pointerStartY: number
    targetTrackId: string
    newTrack: boolean
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
  let clipMenu = $state<{ clipId: string | null; gap?: { trackId: string; start: number; end: number }; x: number; y: number } | null>(null)
  let clipMenuOrigin: HTMLElement | null = null
  let clipMenuElement = $state<HTMLDivElement>()
  let commandPaletteOpen = $state(false)
  let toolbarFocusIndex = $state(0)
  let paletteFocusIndex = $state(0)
  let marquee = $state<{ kind: 'clips' | 'keys'; x: number; y: number; clientX: number; clientY: number; pointerId: number; active: boolean; previous: { ids: string[]; primaryId: string | null; trackId: string | null; gap: typeof compositionState.ui.selectedGap; keys: typeof compositionState.ui.selectedKeyframes } } | null>(null)
  let marqueeFrame: number | null = null
  let suppressMarqueeClick = false
  let marqueeBox = $state<MarqueeRect | null>(null)
  $effect(() => () => endMarquee(true))

  function onTimelineBackgroundClick(event: MouseEvent): void {
    if (event.button !== 0 || marquee?.active) return
    if (compositionState.ui.keyframePointerReleasePending) { compositionState.ui.keyframePointerReleasePending = false; return }
    const hit = event.target as HTMLElement
    if (hit.closest('.composition-keyframe-lane') && !hit.closest('button, input')) {
      if (suppressMarqueeClick) { suppressMarqueeClick = false; return }
      compositionState.ui.selectedKeyframes = []
      return
    }
    if ((event.target as HTMLElement).closest('.composition-keyframe-row, .composition-track-lane, .composition-clip, .composition-ruler, .composition-track-head, button, input')) return
    if (suppressMarqueeClick) { suppressMarqueeClick = false; return }
    selectCompositionClip(null, null)
  }

  function beginMarquee(event: PointerEvent): void {
    suppressMarqueeClick = false
    compositionState.ui.keyframePointerReleasePending = false
    const hit = event.target as HTMLElement
    if (event.button !== 0 || hit.closest('.composition-clip, .composition-ruler, .composition-track-head, button, input') || gesture || !timelineScroll) return
    marquee = { kind: hit.closest('.composition-keyframe-lane') ? 'keys' : 'clips', x: event.clientX + timelineScroll.scrollLeft, y: event.clientY + timelineScroll.scrollTop, clientX: event.clientX, clientY: event.clientY, pointerId: event.pointerId, active: false, previous: { ids: [...compositionState.ui.selectedClipIds], primaryId: compositionState.ui.selectedClipId, trackId: compositionState.ui.selectedTrackId, gap: compositionState.ui.selectedGap ? { ...compositionState.ui.selectedGap } : null, keys: compositionState.ui.selectedKeyframes.map((subject) => ({ ...subject, ticks: [...subject.ticks] })) } }
  }

  function updateMarquee(): void {
    if (!marquee || !marquee.active || !timelineScroll) return
    const rect = marqueeRect(marquee.x - timelineScroll.scrollLeft, marquee.y - timelineScroll.scrollTop, marquee.clientX, marquee.clientY)
    if (marquee.kind === 'keys') {
      const channels = compositionState.document.tracks.flatMap(keyframeChannels)
      const bounds: MarqueeKeyframeBounds[] = []
      for (const diamond of timelineScroll.querySelectorAll<HTMLButtonElement>('.composition-keyframe-diamond')) {
        const lane = diamond.closest<HTMLElement>('.composition-keyframe-lane')
        if (!lane) continue
        const box = diamond.getBoundingClientRect()
        const row = lane.getBoundingClientRect()
        bounds.push({ channelId: diamond.dataset.keyframeChannel!, tick: Number(diamond.dataset.keyframeTick), x: box.left + box.width / 2, y: box.top + box.height / 2, laneTop: row.top, laneBottom: row.bottom, expanded: lane.dataset.expanded === 'true', locked: diamond.disabled })
      }
      compositionState.ui.selectedKeyframes = selectMarqueeKeyframes(rect, bounds).flatMap((selection) => {
        const channel = channels.find((channel) => channel.id === selection.channelId)
        return channel ? [{ target: channel.target, property: channel.property, ticks: selection.ticks }] : []
      })
    } else {
      const bounds: MarqueeClipBounds[] = []
      for (const lane of timelineScroll.querySelectorAll<HTMLElement>('.composition-track-lane')) {
        const trackId = lane.closest<HTMLElement>('[data-track-id]')?.dataset.trackId
        const track = compositionState.document.tracks.find((track) => track.id === trackId)
        if (!track) continue
        const box = lane.getBoundingClientRect()
        for (const clip of track.clips) {
          const left = box.left + ticksToPx(clip.timelineStartTicks)
          bounds.push({ clipId: clip.id, trackId: track.id, left, right: left + Math.max(12, ticksToPx(clipDurationTicks(clip))), top: box.top + 7, bottom: box.top + 57 })
        }
      }
      const selection = selectMarqueeClips(compositionState.document, rect, bounds, marquee.previous.primaryId, !compositionState.ui.linkOverride)
      compositionState.ui.selectedKeyframes = []
      compositionState.ui.selectedClipIds = selection.ids
      compositionState.ui.selectedClipId = selection.primaryId
      compositionState.ui.selectedTrackId = selection.trackId
      compositionState.ui.selectedGap = null
    }
    const viewport = timelineScroll.getBoundingClientRect()
    marqueeBox = { left: rect.left - viewport.left + timelineScroll.scrollLeft, right: rect.right - viewport.left + timelineScroll.scrollLeft, top: rect.top - viewport.top + timelineScroll.scrollTop, bottom: rect.bottom - viewport.top + timelineScroll.scrollTop }
  }

  function scrollMarquee(): void {
    marqueeFrame = null
    if (!marquee?.active || !timelineScroll) return
    const viewport = timelineScroll.getBoundingClientRect()
    const step = marqueeScrollStep(marquee.clientX, viewport.left, viewport.right)
    const old = timelineScroll.scrollLeft
    timelineScroll.scrollLeft += step
    if (timelineScroll.scrollLeft !== old) updateMarquee()
    marqueeFrame = requestAnimationFrame(scrollMarquee)
  }

  function moveMarquee(event: PointerEvent): void {
    if (!marquee || marquee.pointerId !== event.pointerId || !timelineScroll) return
    marquee.clientX = event.clientX
    marquee.clientY = event.clientY
    if (!marquee.active && Math.hypot(event.clientX - (marquee.x - timelineScroll.scrollLeft), event.clientY - (marquee.y - timelineScroll.scrollTop)) >= 3) {
      marquee.active = true
      timelineScroll.setPointerCapture?.(event.pointerId)
      marqueeFrame = requestAnimationFrame(scrollMarquee)
    }
    if (marquee.active) event.preventDefault()
    updateMarquee()
  }

  function endMarquee(cancel = false): void {
    if (!marquee) return
    if (cancel && marquee.active) {
      compositionState.ui.selectedClipIds = marquee.previous.ids
      compositionState.ui.selectedClipId = marquee.previous.primaryId
      compositionState.ui.selectedTrackId = marquee.previous.trackId
      compositionState.ui.selectedGap = marquee.previous.gap
      compositionState.ui.selectedKeyframes = marquee.previous.keys
    }
    suppressMarqueeClick = marquee.active
    const pointerId = marquee.pointerId
    marquee = null
    marqueeBox = null
    if (marqueeFrame !== null) cancelAnimationFrame(marqueeFrame)
    marqueeFrame = null
    if (timelineScroll?.hasPointerCapture?.(pointerId)) timelineScroll.releasePointerCapture(pointerId)
  }
  const copyPreview = $derived.by(() => {
    if (gesture?.mode !== 'duplicate' || !previewRange) return { ghosts: [], error: '' }
    const ids = [gesture.clipId, ...compositionState.ui.selectedClipIds.filter((id) => id !== gesture!.clipId)]
    if (ids.some((id) => !compositionState.document.tracks.some((track) => track.clips.some((clip) => clip.id === id)))) return { ghosts: [], error: 'Клип больше не существует' }
    const locations = ids.map((id) => findClipLocation(compositionState.document, id))
    const delta = Math.max(-Math.min(...locations.map(({ clip }) => clip.timelineStartTicks)), previewRange.start - gesture.originalStart)
    let ghosts = locations.map(({ clip, track }, index) => ({ clipId: clip.id, trackId: index === 0 ? gesture!.targetTrackId : track.id, start: clip.timelineStartTicks + delta, end: clipEndTicks(clip) + delta }))
    let prefix = 'drag-preview-'
    while (compositionState.document.tracks.some((track) => track.clips.some((clip) => clip.id.startsWith(prefix)))) prefix += '_'
    let error = ''
    try {
      const predicted = pasteClips(compositionState.document, ids.map((clipId, index) => ({ clipId, id: `${prefix}${index}` })), { targetTrackId: gesture.targetTrackId, timelineStartTicks: previewRange.start })
      ghosts = ids.map((clipId, index) => {
        const { clip, track } = findClipLocation(predicted, `${prefix}${index}`)
        return { clipId, trackId: track.id, start: clip.timelineStartTicks, end: clipEndTicks(clip) }
      })
    }
    catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    return { ghosts, error }
  })
  const raisePreview = $derived.by(() => {
    if (gesture?.mode !== 'move' || !gesture.newTrack || !previewRange) return { clips: [] as CompositionClip[], error: '' }
    let id = 'raise-drop-preview'
    while (compositionState.document.tracks.some((track) => track.id === id)) id += '-new'
    try {
      const next = moveClipsToNewTrack(compositionState.document, compositionState.ui.selectedClipIds, id, { clipId: gesture.clipId, startTicks: previewRange.start })
      return { clips: next.tracks[0]!.clips as readonly CompositionClip[], error: '' }
    } catch (error) { return { clips: [] as CompositionClip[], error: error instanceof Error ? error.message : String(error) } }
  })
  const movePreview = $derived.by(() => {
    if (gesture?.mode !== 'move' || gesture.newTrack || !previewRange) return { ghosts: [], error: '' }
    const ids = [gesture.clipId, ...compositionState.ui.selectedClipIds.filter((id) => id !== gesture!.clipId)]
    if (ids.some((id) => !compositionState.document.tracks.some((track) => track.clips.some((clip) => clip.id === id)))) return { ghosts: [], error: 'Клип больше не существует' }
    const locations = ids.map((id) => findClipLocation(compositionState.document, id))
    const delta = Math.max(-Math.min(...locations.map(({ clip }) => clip.timelineStartTicks)), previewRange.start - gesture.originalStart)
    let ghosts = locations.map(({ clip, track }, index) => ({ clipId: clip.id, trackId: index === 0 ? gesture!.targetTrackId : track.id, start: clip.timelineStartTicks + delta, end: clipEndTicks(clip) + delta }))
    let error = ''
    try {
      const predicted = planCompositionClipMove(gesture.clipId, gesture.targetTrackId, previewRange.start)
      ghosts = ids.map((id) => {
        const { clip, track } = findClipLocation(predicted, id)
        return { clipId: id, trackId: track.id, start: clip.timelineStartTicks, end: clipEndTicks(clip) }
      })
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
    return { ghosts, error }
  })

  const transitionPreview = $derived.by(() => {
    if (gesture?.mode !== 'move') return []
    const members = gesture.newTrack
      ? raisePreview.clips.map((clip) => ({ clipId: clip.id, trackId: 'new-track-drop', start: clip.timelineStartTicks }))
      : movePreview.ghosts
    if (gesture.newTrack ? raisePreview.error : movePreview.error) return []
    return compositionState.document.tracks.flatMap((track) => track.kind === 'video' ? (track.transitions ?? []).flatMap((transition) => {
      if (transition.timeline) return [] // Original move_items removes touched transitions.
      const from = members.find((member) => member.clipId === transition.fromClipId)
      const to = members.find((member) => member.clipId === transition.toClipId)
      return from && to && from.trackId === to.trackId ? [{ id: transition.id, trackId: to.trackId, start: to.start - Math.floor(transition.durationTicks / 2), duration: transition.durationTicks }] : []
    }) : [])
  })

  const enabledSelection = $derived.by(() => {
    const ids = compositionState.ui.selectedClipIds
    const selected = compositionState.document.tracks.flatMap((track) => track.clips.filter((clip) => ids.includes(clip.id)).map((clip) => ({ clip, track })))
    return { count: selected.length, enable: selected.some(({ clip }) => clip.enabled === false), locked: selected.some(({ track, clip }) => track.locked || clip.locked) }
  })
  const clipRippleRefusal = $derived.by(() => {
    if (!clipMenu?.clipId) return ''
    try { rippleDeleteClips(compositionState.document, compositionState.ui.selectedClipIds); return '' }
    catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const raiseRefusal = $derived.by(() => {
    if (!clipMenu?.clipId) return ''
    let id = 'raise-preflight'
    while (compositionState.document.tracks.some((track) => track.id === id)) id += '-new'
    try { moveClipsToNewTrack(compositionState.document, compositionState.ui.selectedClipIds, id); return '' }
    catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  $effect(() => {
    if (clipMenu?.clipId && !compositionState.document.tracks.some((track) => track.clips.some((clip) => clip.id === clipMenu?.clipId))) clipMenu = null
    if (clipMenu?.gap && JSON.stringify(clipMenu.gap) !== JSON.stringify(compositionState.ui.selectedGap)) clipMenu = null
  })

  async function openClipMenu(event: MouseEvent | KeyboardEvent, trackId: string, clipId: string): Promise<void> {
    event.preventDefault()
    event.stopPropagation()
    if (!compositionState.ui.selectedClipIds.includes(clipId)) selectCompositionClip(trackId, clipId, { single: event.altKey })
    clipMenuOrigin = event.currentTarget as HTMLElement
    const bounds = clipMenuOrigin.getBoundingClientRect()
    clipMenu = { clipId, x: Math.max(0, Math.min(event instanceof MouseEvent ? event.clientX : bounds.left, window.innerWidth - 220)), y: Math.max(0, Math.min(event instanceof MouseEvent ? event.clientY : bounds.bottom, window.innerHeight - 150)) }
    await tick()
    clipMenuElement?.querySelector<HTMLButtonElement>('button')?.focus()
  }

  async function openGapMenu(event: MouseEvent | KeyboardEvent, trackId: string): Promise<void> {
    event.preventDefault()
    event.stopPropagation()
    const origin = event.currentTarget as HTMLElement
    const bounds = origin.getBoundingClientRect()
    if (event instanceof MouseEvent) selectCompositionGap(trackId, pxToTicks(Math.max(0, event.clientX - bounds.left)))
    const gap = compositionState.ui.selectedGap
    clipMenu = null
    if (!gap || gap.trackId !== trackId) return
    clipMenuOrigin = origin
    clipMenu = { clipId: null, gap: { ...gap }, x: Math.max(0, Math.min(event instanceof MouseEvent ? event.clientX : bounds.left, window.innerWidth - 220)), y: Math.max(0, Math.min(event instanceof MouseEvent ? event.clientY : bounds.bottom, window.innerHeight - 150)) }
    await tick()
    clipMenuElement?.querySelector<HTMLButtonElement>('button')?.focus()
  }

  function executeClipMenu(action: () => void): void {
    clipMenu = null
    run(action)
  }

  function onClipMenuKey(event: KeyboardEvent): void {
    if (event.key === 'Escape') { event.preventDefault(); clipMenu = null; if (clipMenuOrigin?.isConnected) clipMenuOrigin.focus(); return }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
    event.preventDefault()
    const buttons = [...(clipMenuElement?.querySelectorAll<HTMLButtonElement>('button') ?? [])]
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement)
    const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + buttons.length) % buttons.length
    buttons[next]?.focus()
  }

  const gapRefusal = $derived.by(() => {
    const gap = compositionState.ui.selectedGap
    if (!gap) return ''
    try { rippleDeleteGap(compositionState.document, gap.trackId, gap.start, gap.end); return '' }
    catch (error) { return error instanceof Error ? error.message : String(error) }
  })
  const markers = $derived(compositionMarkerList())
  const reviewTicks = $derived(reviewThreads.map((thread) => thread.comments[0]?.timelineTick ?? 0))
  const timelineExtent = $derived(Math.max(compositionDuration(), markers.at(-1)?.tick ?? 0, ...reviewTicks))
  const contentWidth = $derived(
    Math.max(760, (timelineExtent / COMPOSITION_TIME_BASE) * compositionState.ui.zoomPxPerSecond + 160),
  )
  const viewport = $derived(timelineViewport(scrollLeft, viewportWidth, contentWidth))
  let revealedRequest=''
  $effect(()=>{
    const request=activityReveal.request,scroll=timelineScroll
    if(!request||request.mode!=='ordinary'||request.id===revealedRequest||!scroll)return
    revealedRequest=request.id
    void tick().then(async()=>{
      if(activityReveal.request!==request||!scroll.isConnected)return
      if(request.clipId){
        const clip=compositionState.document.tracks.find(track=>track.id===request.trackId)?.clips.find(clip=>clip.id===request.clipId)
        if(!clip)return
        scrollLeft=Math.max(0,ticksToPx(clip.timelineStartTicks)-32);scroll.scrollLeft=scrollLeft
        viewportWidth=scroll.clientWidth||viewportWidth
        await tick()
        if(activityReveal.request!==request||!scroll.isConnected)return
      }
      const target=request.clipId?[...scroll.querySelectorAll<HTMLElement>('.composition-clip[data-clip-id]')].find(row=>row.dataset.clipId===request.clipId):[...scroll.querySelectorAll<HTMLElement>('[data-track-id]')].find(row=>row.dataset.trackId===request.trackId)
      target?.scrollIntoView({block:'nearest',inline:'nearest'});target?.focus({preventScroll:true})
    })
  })
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
    return gesture?.mode !== 'duplicate' && gesture?.mode !== 'move' && previewRange?.clipId === clip.id
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
    if (event.button !== 0) return
    event.preventDefault()
    event.stopPropagation()
    const track = compositionState.document.tracks.find((candidate) => candidate.id === trackId)
    const copying = mode === 'move' && event.altKey
    const preserveSubjects = !event.shiftKey && (copying || mode === 'move') && compositionState.ui.selectedClipIds.includes(clip.id)
    if (!preserveSubjects && !selectCompositionClip(trackId, clip.id, { single: event.altKey && !copying, toggle: event.shiftKey })) return
    if (copying || mode === 'move') { compositionState.ui.selectedClipId = clip.id; compositionState.ui.selectedTrackId = trackId }
    // Pointer default is suppressed for dragging; restore focus for keyboard commands.
    ;(event.currentTarget as HTMLElement).focus({ preventScroll: true })
    if (track?.locked || clip.locked) return
    const resolvedMode = copying ? 'duplicate' : mode
    gesture = {
      clipId: clip.id,
      trackId,
      targetTrackId: trackId,
      newTrack: false,
      mode: resolvedMode,
      pointerStartX: event.clientX,
      pointerStartY: event.clientY,
      originalStart: clip.timelineStartTicks,
      originalEnd: clipEndTicks(clip),
    }
    previewRange = { clipId: clip.id, start: clip.timelineStartTicks, end: clipEndTicks(clip) }
    slipDelta = 0
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function moveGesture(event: PointerEvent): void {
    if (!gesture || !previewRange) return
    gesture.newTrack = gesture.mode === 'move' && !!document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-new-track-drop]')
    if (gesture.mode === 'duplicate' || gesture.mode === 'move') gesture.targetTrackId = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-track-id]')?.dataset.trackId ?? gesture.trackId
    const delta = pxToTicks(event.clientX - gesture.pointerStartX)
    const minimum = Math.max(1, Math.round(COMPOSITION_TIME_BASE / 100))
    if (gesture.mode === 'slip') {
      slipDelta = delta
    } else if (gesture.mode === 'move' || gesture.mode === 'duplicate') {
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
      } else if (current.mode === 'duplicate' && Math.hypot(event.clientX - current.pointerStartX, event.clientY - current.pointerStartY) < 3) {
        selectCompositionClip(current.trackId, current.clipId, { single: true })
      } else if (current.mode === 'move' || current.mode === 'duplicate') {
        if (current.mode === 'move' && document.elementFromPoint(event.clientX, event.clientY)?.closest('[data-new-track-drop]')) {
          const startTicks = Math.max(0, current.originalStart + pxToTicks(event.clientX - current.pointerStartX))
          raiseSelectedCompositionClips({ clipId: current.clipId, startTicks })
          return
        }
        const underPointer = document.elementFromPoint(event.clientX, event.clientY)?.closest<HTMLElement>('[data-track-id]')
        const targetTrackId = underPointer?.dataset.trackId ?? current.trackId
        if (current.mode === 'duplicate') duplicateSelectedCompositionClip({ trackId: targetTrackId, timelineStartTicks: range.start })
        else moveCompositionClip(current.clipId, targetTrackId, range.start)
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
    if (clip.kind === 'graphic') return 'Графика'
    if (clip.kind === 'text') return clip.text
    if (clip.kind === 'color') return `Цвет ${clip.color}`
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
    if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
      void openClipMenu(event, trackId, clipId)
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      selectCompositionClip(trackId, clipId, { single: event.altKey, toggle: event.shiftKey })
    } else if (event.key === 'Delete' || event.key === 'Backspace') {
      event.preventDefault()
      if (!compositionState.ui.selectedClipIds.includes(clipId)) selectCompositionClip(trackId, clipId, { single: event.altKey })
      if (event.shiftKey) run(rippleDeleteSelectedCompositionClip)
      else executeCompositionCommand('composition.delete')
    } else if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
      event.preventDefault()
      if (event.altKey || !compositionState.ui.selectedClipIds.includes(clipId)) selectCompositionClip(trackId, clipId, { single: event.altKey })
      const frame = Math.max(1, Math.round(COMPOSITION_TIME_BASE / (compositionState.document.canvas.fps || 30)))
      const delta = event.key === 'ArrowLeft' ? -frame : frame
      if (event.shiftKey) run(() => slipCompositionClip(clipId, delta))
      else run(() => moveCompositionClip(clipId, trackId, stepTimeGridTicks(clip.timelineStartTicks, event.key === 'ArrowLeft' ? -1 : 1, canvasTimeGrid(compositionState.document.canvas)), false))
    }
  }

  function onTrimKey(event: KeyboardEvent, trackId: string, clip: CompositionClip, edge: 'start' | 'end'): void {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    event.stopPropagation()
    if (event.altKey || !compositionState.ui.selectedClipIds.includes(clip.id)) selectCompositionClip(trackId, clip.id, { single: event.altKey })
    const rate = canvasTimeGrid(compositionState.document.canvas)
    const direction = event.key === 'ArrowLeft' ? -1 : 1
    run(() => trimCompositionClip(
      clip.id,
      edge === 'start' ? stepTimeGridTicks(clip.timelineStartTicks, direction, rate) : clip.timelineStartTicks,
      edge === 'end' ? stepTimeGridTicks(clipEndTicks(clip), direction, rate) : clipEndTicks(clip),
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

<svelte:window onkeydown={(event) => { if (event.key === 'Escape' && marquee) { event.preventDefault(); endMarquee(true) } else onExportRangeShortcut(event) }} onpointermove={moveMarquee} onpointerup={(event) => { if (marquee?.pointerId === event.pointerId) endMarquee() }} onpointercancel={(event) => { if (marquee?.pointerId === event.pointerId) endMarquee(true) }} onpointerdown={(event) => { if (clipMenu && !(event.target as HTMLElement)?.closest('.composition-clip-menu')) clipMenu = null }} />
{#if clipMenu}
  <div class="composition-clip-menu" role="menu" tabindex="-1" aria-label={clipMenu.gap ? 'Действия с выбранным пробелом' : 'Действия с выделенными клипами'} bind:this={clipMenuElement} style:left={`${clipMenu.x}px`} style:top={`${clipMenu.y}px`} onkeydown={onClipMenuKey}>
    {#if clipMenu.gap}
      <button role="menuitem" type="button" aria-disabled={!!gapRefusal} title={gapRefusal || undefined} onclick={() => { if (!gapRefusal) executeClipMenu(closeSelectedCompositionGap) }}>Удалить пробел со сдвигом</button>
    {:else}
    <button role="menuitem" type="button" aria-disabled={enabledSelection.locked} title={enabledSelection.locked ? 'Выделение содержит заблокированную дорожку' : undefined} onclick={() => { if (!enabledSelection.locked) executeClipMenu(toggleSelectedCompositionClipsEnabled) }}>{enabledSelection.enable ? 'Включить' : 'Выключить'} клипы ({enabledSelection.count})</button>
    <button role="menuitem" type="button" aria-disabled={enabledSelection.locked} onclick={() => { if (!enabledSelection.locked) executeClipMenu(deleteSelectedCompositionClip) }}>Удалить клипы ({enabledSelection.count})</button>
    <button role="menuitem" type="button" aria-disabled={!!clipRippleRefusal} title={clipRippleRefusal || undefined} onclick={() => { if (!clipRippleRefusal) executeClipMenu(rippleDeleteSelectedCompositionClip) }}>Ripple delete ({enabledSelection.count})</button>
    <button role="menuitem" type="button" aria-disabled={!!raiseRefusal} title={raiseRefusal || undefined} onclick={() => { if (!raiseRefusal) executeClipMenu(raiseSelectedCompositionClips) }}>На новую дорожку</button>
    {/if}
  </div>
{/if}

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
      >{command.id === 'composition.undo' ? '↶' : command.id === 'composition.redo' ? '↷' : command.id === 'composition.linkSelection' ? (compositionLinkToggleState(compositionState.document, compositionState.ui.selectedClipIds).mode === 'remove' ? 'Разорвать связь' : 'Связать клипы') : shortcutDefinition(command.id).label}</button>
      {#if !command.enabled}<span id={`timeline-command-reason-${index}`} class="visually-hidden">{command.disabledReason}</span>{/if}
    {/each}
    <button class="btn ghost sm danger" type="button" onclick={() => run(rippleDeleteSelectedCompositionClip)} disabled={!compositionState.ui.selectedClipId} title="Удалить выделенные клипы и закрыть их интервалы на всех дорожках">Ripple delete</button>
    <button class="btn ghost sm" type="button" disabled={!enabledSelection.count || enabledSelection.locked} title={enabledSelection.locked ? 'Выделение содержит заблокированную дорожку' : 'Изменить включение точного выделения в предпросмотре и экспорте'} onclick={() => run(toggleSelectedCompositionClipsEnabled)}>{enabledSelection.enable ? 'Включить' : 'Выключить'} клипы ({enabledSelection.count})</button>
    {#if compositionState.ui.selectedGap}
      <button class="btn ghost sm" type="button" aria-disabled={!!gapRefusal} title={gapRefusal || 'Закрыть выбранный пробел на всех дорожках'} onclick={() => { if (!gapRefusal) run(closeSelectedCompositionGap) }}>Закрыть пробел</button>
      <span role="status">{gapRefusal || `Пробел ${formatShort(compositionState.ui.selectedGap.start)} — ${formatShort(compositionState.ui.selectedGap.end)}`}</span>
    {/if}
    {#if compositionState.ui.linkOverride}<span role="status">Связи отключены</span>{/if}
    {#if compositionState.ui.selectedClipIds.length > 1}<span>Выбрано: {compositionState.ui.selectedClipIds.length}</span>{/if}
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
      <TimelineSemanticList tracks={compositionState.document.tracks} selectedClipId={compositionState.ui.selectedClipId} selectedClipIds={compositionState.ui.selectedClipIds} clipLabel={clipName} onselect={selectCompositionClip} />
    </div>
  {/if}

  <div class="composition-timeline-scroll" bind:this={timelineScroll} onscroll={updateViewport} onpointerup={onTimelineBackgroundClick} onpointerdown={beginMarquee} onlostpointercapture={() => endMarquee(true)} role="region" aria-label="Область выделения клипов" tabindex="-1">
    {#if marqueeBox}
      <div class="composition-marquee" style:left={`${marqueeBox.left}px`} style:top={`${marqueeBox.top}px`} style:width={`${marqueeBox.right - marqueeBox.left}px`} style:height={`${marqueeBox.bottom - marqueeBox.top}px`} aria-hidden="true"></div>
    {/if}
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

    {#if compositionState.document.tracks.length}
      <div class="composition-track-row composition-new-track-drop" class:active={gesture?.mode === 'move'} class:invalid={!!raisePreview.error} data-new-track-drop role="status" aria-label="Перенести выделение на новую дорожку" title={raisePreview.error || 'Перенести выделение на новую дорожку'}>
        <div class="composition-track-head">Новая дорожка</div>
        <div class="composition-new-track-lane" style:width={`${contentWidth}px`}>
          {#each raisePreview.clips as clip (clip.id)}
            <div class="composition-copy-ghost" style:left={`${ticksToPx(clip.timelineStartTicks)}px`} style:width={`${Math.max(12, ticksToPx(clipDurationTicks(clip)))}px`} aria-hidden="true"></div>
          {/each}
          {#each transitionPreview.filter((preview) => preview.trackId === 'new-track-drop') as preview (preview.id)}
            <div class="composition-transition-handle composition-transition-ghost" style:left={`${ticksToPx(preview.start)}px`} style:width={`${Math.max(18, ticksToPx(preview.duration))}px`} aria-hidden="true">⇄</div>
          {/each}
        </div>
      </div>
    {/if}
    {#each compositionState.document.tracks as track, index (track.id)}
      <div class="composition-track-row" class:locked={track.locked} tabindex="-1" data-track-id={track.id}>
        <div class="composition-track-head">
          <div class="composition-track-title"><span class={`composition-kind ${track.kind}`}>{track.kind}</span><strong>{track.name}</strong></div>
          <div class="composition-track-actions">
            <button class="mini-icon" type="button" disabled={index === 0} onclick={() => run(() => reorderCompositionTrack(track.id, index - 1))} aria-label={`Переместить дорожку «${track.name}» выше`} aria-keyshortcuts={compositionState.ui.selectedTrackId === track.id ? shortcutAria('composition.trackUp') : undefined} title={`Выше (${shortcutLabel('composition.trackUp')})`}>↑</button>
            <button class="mini-icon" type="button" disabled={index === compositionState.document.tracks.length - 1} onclick={() => run(() => reorderCompositionTrack(track.id, index + 1))} aria-label={`Переместить дорожку «${track.name}» ниже`} aria-keyshortcuts={compositionState.ui.selectedTrackId === track.id ? shortcutAria('composition.trackDown') : undefined} title={`Ниже (${shortcutLabel('composition.trackDown')})`}>↓</button>
            <button class="mini-icon" type="button" onclick={() => run(() => closeCompositionGapAtPlayhead(track.id))} aria-label={`Закрыть пробел у плейхеда на дорожке «${track.name}»`} title="Закрыть пробел у плейхеда на всех дорожках">↤</button>
            <button class="mini-icon" type="button" disabled={track.locked || track.clips.length < 2} onclick={() => run(() => magnetizeCompositionTrack(track.id))} aria-label={`Собрать gaps дорожки «${track.name}»`} title={track.locked ? 'Сначала разблокируйте дорожку' : 'Однократно собрать clips от начала дорожки'}>🧲</button>
            {#if 'muted' in track}<button class:active={track.muted} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'muted'))} aria-label={`${track.muted ? 'Включить' : 'Выключить'} звук дорожки «${track.name}»`} title="Звук">{track.muted ? '🔇' : '🔊'}</button>{/if}
            {#if track.kind === 'audio'}<button class:active={track.solo ?? false} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'solo'))} aria-label={`${track.solo ? 'Отключить solo' : 'Включить solo'} дорожки «${track.name}»`} title="Solo">S</button>{/if}
            {#if 'hidden' in track}<button class:active={track.hidden} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'hidden'))} aria-label={`${track.hidden ? 'Показать' : 'Скрыть'} дорожку «${track.name}»`} title="Видимость">{track.hidden ? '◌' : '◉'}</button>{/if}
            <button class:active={track.syncLock ?? true} class="mini-icon" type="button" aria-pressed={track.syncLock ?? true} onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'syncLock'))} aria-label={`${track.syncLock === false ? 'Включить' : 'Выключить'} sync-lock дорожки «${track.name}»`} title="Sync-lock при вставке">⇄</button>
            <button class:active={track.locked} class="mini-icon" type="button" onclick={() => run(() => toggleCompositionTrackFlag(track.id, 'locked'))} aria-label={`${track.locked ? 'Разблокировать' : 'Заблокировать'} дорожку «${track.name}»`} title="Блокировка">{track.locked ? '🔒' : '🔓'}</button>
          </div>
        </div>
        <div
          class="composition-track-lane"
          style:width={`${contentWidth}px`}
          role="slider"
          tabindex="0"
          aria-label={`Плейхед на дорожке ${track.name}`}
          title="Alt+стрелки выбирают пробелы; Delete закрывает выбранный пробел; Escape снимает выделение"
          aria-valuemin="0"
          aria-valuemax={compositionDuration()}
          aria-valuenow={compositionState.transport.playheadTicks}
          onclick={(event) => { if (suppressMarqueeClick) { suppressMarqueeClick = false; return } seekOnLane(event); selectCompositionGap(track.id, compositionState.transport.playheadTicks) }}
          oncontextmenu={(event) => void openGapMenu(event, track.id)}
          onkeydown={(event) => {
            if ((event.shiftKey && event.key === 'F10') || event.key === 'ContextMenu') void openGapMenu(event, track.id)
            else if (event.altKey && (event.key === 'ArrowLeft' || event.key === 'ArrowRight')) {
              event.preventDefault(); selectAdjacentCompositionGap(track.id, event.key === 'ArrowLeft' ? -1 : 1)
            } else if (event.key === 'Delete' || event.key === 'Backspace') {
              event.preventDefault(); run(closeSelectedCompositionGap)
            } else if (event.key === 'Escape') selectCompositionClip(null, null)
            else seekWithKeyboard(event)
          }}
        >
          {#if compositionState.ui.selectedGap?.trackId === track.id}
            <div class="composition-selected-gap" title={`Пробел ${formatShort(compositionState.ui.selectedGap.start)} — ${formatShort(compositionState.ui.selectedGap.end)}` } style:left={`${ticksToPx(compositionState.ui.selectedGap.start)}px`} style:width={`${ticksToPx(compositionState.ui.selectedGap.end - compositionState.ui.selectedGap.start)}px`} aria-hidden="true"></div>
          {/if}
          {#each visibleClips(track) as clip (clip.id)}
            {@const link = compositionState.document.links?.find((link) => link.memberIds.includes(clip.id))}
            <div
              class={`composition-clip ${clip.kind}`}
              data-clip-id={clip.id}
              class:selected={compositionState.ui.selectedClipIds.includes(clip.id) || compositionState.ui.selectedClipId === clip.id}
              class:dragging={gesture?.clipId === clip.id}
              class:disabled-clip={clip.enabled === false}
              style:left={`${leftPx(clip)}px`}
              style:width={`${widthPx(clip)}px`}
              style:border-left={link ? `2px solid ${compositionLinkAccent(link.id, compositionState.ui.linkOverride)}` : undefined}
              role="button"
              tabindex="0"
              aria-label={`${clipName(clip)}, ${formatShort(clipDurationTicks(clip))}`}
              onpointerdown={(event) => beginGesture(event, clip, track.id, 'move')}
              onpointermove={moveGesture}
              onpointerup={endGesture}
              onpointercancel={cancelGesture}
              onlostpointercapture={cancelGesture}
              oncontextmenu={(event) => void openClipMenu(event, track.id, clip.id)}
              onclick={(event) => event.stopPropagation()}
              onkeydown={(event) => onClipKey(event, track.id, clip)}
            >
              {#if clip.locked || track.locked}<span class="composition-clip-lock" title={clip.locked ? 'Клип заблокирован' : 'Дорожка заблокирована'} aria-label="Клип защищён">🔒</span>{/if}
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
              {@const incoming = track.clips.find((clip) => clip.id === (transition.toClipId ?? transition.fromClipId))}
              {@const span = transition.timeline ? transitionSourceSpan(transition.timeline).map(value=>Number(value)/254016) : null}
              {@const transitionStart = span ? span[0]! : (incoming?.timelineStartTicks ?? 0) - Math.floor(transition.durationTicks / 2)}
              {@const transitionDuration = span ? span[1]! : transition.durationTicks}
              {#if incoming && !transitionPreview.some((preview) => preview.id === transition.id)}
                <button
                  type="button"
                  class="composition-transition-handle"
                  style:left={`${ticksToPx(transitionStart)}px`}
                  style:width={`${Math.max(18, ticksToPx(transitionDuration))}px`}
                  aria-label={`Переход ${transition.concatOverlap?.legacy ?? transition.kind}, ${formatShort(transitionDuration)}, к ${clipName(incoming)}`}
                  title={`${transition.concatOverlap?.legacy ?? transition.kind} · ${formatShort(transitionDuration)} · source handles`}
                  onclick={(event) => { event.stopPropagation(); selectCompositionClip(track.id, incoming.id) }}
                >⇄</button>
              {/if}
            {/each}
          {/if}
          {#each copyPreview.ghosts.filter((ghost) => ghost.trackId === track.id) as ghost (ghost.clipId)}
            <div class="composition-copy-ghost" class:invalid={!!copyPreview.error} style:left={`${ticksToPx(ghost.start)}px`} style:width={`${Math.max(12, ticksToPx(ghost.end - ghost.start))}px`} title={copyPreview.error || 'Копия клипа'} aria-hidden="true"></div>
          {/each}
          {#each movePreview.ghosts.filter((ghost) => ghost.trackId === track.id) as ghost (ghost.clipId)}
            <div class="composition-copy-ghost composition-move-ghost" class:invalid={!!movePreview.error} style:left={`${ticksToPx(ghost.start)}px`} style:width={`${Math.max(12, ticksToPx(ghost.end - ghost.start))}px`} title={movePreview.error || 'Новое положение клипа'} aria-hidden="true"></div>
          {/each}
          {#each transitionPreview.filter((preview) => preview.trackId === track.id) as preview (preview.id)}
            <div class="composition-transition-handle composition-transition-ghost" style:left={`${ticksToPx(preview.start)}px`} style:width={`${Math.max(18, ticksToPx(preview.duration))}px`} aria-hidden="true">⇄</div>
          {/each}
          <div class="composition-playhead" style:left={`${ticksToPx(compositionState.transport.playheadTicks)}px`}></div>
        </div>
      </div>
      <TimelineKeyframes {track} width={contentWidth} />
      <TimelineColorKeys {track} width={contentWidth} />
    {:else}
      <div class="composition-no-tracks">Добавьте медиа или текст — дорожка создастся автоматически.</div>
    {/each}
  </div>
</section>
