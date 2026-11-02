<script lang="ts">
  import { BEZIER_PRESETS, bezierPresetId } from '$lib/composition/easingPresets'
  import { resolvedKeyframeTangents } from '$lib/composition/tangents'
  import { setCompositionAutomationTangents } from '$lib/state/composition.svelte'
  import TimelineTangentHandles from './TimelineTangentHandles.svelte'
  import { tick as flushUi } from 'svelte'
  import { updateSelectedKeyframeInterpolation } from '$lib/composition/keyframes'
  import { keyframeValueRange, keyframeValueY, keyframeCurvePoints, keyframeTailPoints } from '$lib/composition/keyframeCurve'
  import { keyframeChannels, keyframeCompositionTick, type KeyframeChannel } from '$lib/composition/keyframeChannels'
  import { moveSelectedKeyframeTracks, scaleSelectedKeyframeTracks, type SelectedKeyframeTrack } from '$lib/composition/keyframeSelection'
  import { canvasTimeGrid } from '$lib/composition/timeGrid'
  import { clipDurationTicks, COMPOSITION_TIME_BASE, COMPOSITION_INTERPOLATIONS, type CompositionInterpolation, type Composition, type CompositionTrack } from '$lib/composition/types'
  import { compositionState, linkedScaleAutomationPatches, selectCompositionKeyframe, setCompositionPlayhead, resolveAutomation, compositionKeyframeSelectionExtent, moveCompositionKeyframeSelection, scaleCompositionKeyframeSelection, setCompositionKeyframeSelectionInterpolation, type CompositionKeyframeSubject } from '$lib/state/composition.svelte'
  import { shortcutCommandForEvent, shortcutState } from '$lib/state/shortcuts.svelte.js'
  import { executeCompositionCommand } from '$lib/composition/commandRegistry'
  let { track, width }: { track: CompositionTrack; width: number } = $props()
  let tangentMenu = $state<{ channel: KeyframeChannel; tick: number; x: number; y: number; continuity: 'smooth' | 'broken'; element: SVGElement } | null>(null)
  async function openTangentMenu(event: MouseEvent | KeyboardEvent, channel: KeyframeChannel, tick: number): Promise<void> {
    event.preventDefault(); event.stopPropagation()
    if (channel.locked) return
    closeMenu()
    const element = event.currentTarget as SVGElement
    const bounds = element.getBoundingClientRect()
    const index = channel.value.track.keyframes.findIndex(key => key.tick === tick)
    tangentMenu = { channel,tick,x: Math.max(8,Math.min(event instanceof MouseEvent ? event.clientX : bounds.x,window.innerWidth-180)),y: Math.max(8,Math.min(event instanceof MouseEvent ? event.clientY : bounds.bottom,window.innerHeight-90)),continuity: resolvedKeyframeTangents(channel.value.track.keyframes,index,channel.value.track.interpolation).continuity,element }
    await flushUi()
    document.querySelector<HTMLButtonElement>('.composition-tangent-menu button')?.focus()
  }
  function chooseContinuity(continuity: 'smooth' | 'broken'): void {
    if (!tangentMenu) return
    try { setCompositionAutomationTangents(tangentMenu.channel.target,tangentMenu.channel.property,[tangentMenu.tick],{ continuity },true) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    tangentMenu.element.focus(); tangentMenu = null
  }
  function tangentMenuKey(event: KeyboardEvent): void {
    const buttons = [...(event.currentTarget as HTMLElement).parentElement!.querySelectorAll<HTMLButtonElement>('button')]
    const index = buttons.indexOf(event.currentTarget as HTMLButtonElement)
    const direction = event.key === 'ArrowDown' || event.key === 'ArrowRight' ? 1 : event.key === 'ArrowUp' || event.key === 'ArrowLeft' ? -1 : 0
    if (direction || event.key === 'Home' || event.key === 'End') {
      event.preventDefault(); event.stopPropagation()
      buttons[event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length-1 : (index+direction+buttons.length)%buttons.length]?.focus()
    } else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault(); event.stopPropagation(); (event.currentTarget as HTMLButtonElement).click()
    } else if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault(); event.stopPropagation(); tangentMenu?.element.focus(); tangentMenu = null
    }
  }
  let expanded = $state(false)
  let frozenAxes = $state<Record<string, import('$lib/composition/keyframeCurve').KeyframeValueRange>>({})
  function freezeAxis(id: string, range: import('$lib/composition/keyframeCurve').KeyframeValueRange | null): void {
    if (range) frozenAxes[id] = { ...range }
    else delete frozenAxes[id]
  }
  const channelRange = (channel: KeyframeChannel) => frozenAxes[channel.id] ?? keyframeValueRange(channel)
  let menuElement: HTMLDivElement | undefined = $state()
  let menuReturnFocus: HTMLElement | null = null
  let menu = $state<{ x: number; y: number } | null>(null)
  let elasticAmplitude = $state(1)
  let elasticPeriod = $state(0.3)
  let bezierP1 = $state<[number, number]>([0.25, 0.1])
  let bezierP2 = $state<[number, number]>([0.25, 1])
  function bezierMode(): CompositionInterpolation { return { kind: 'bezier', p1: [...bezierP1], p2: [...bezierP2] } }
  const elasticDirections = ['in', 'out', 'in_out'] as const
  function elasticMode(dir: 'in' | 'out' | 'in_out'): CompositionInterpolation {
    return { kind: 'elastic', dir, amplitude: elasticAmplitude, period: elasticPeriod }
  }
  const menuInterpolation = $derived.by(() => {
    const subjects = compositionState.ui.selectedKeyframes
    if (subjects.reduce((count, subject) => count + subject.ticks.length, 0) !== 1) return null
    const subject = subjects[0]!
    const current = resolveAutomation(subject.target, subject.property)[1]
    return current?.mode === 'keyframes' ? current.track.keyframes.find((key) => key.tick === subject.ticks[0])?.interpolation ?? current.track.interpolation : null
  })
  function openMenu(event: MouseEvent, channel: KeyframeChannel, tick: number, seek = true): void {
    event.preventDefault()
    event.stopPropagation()
    if (channel.locked) return
    if (!selected(channel, tick)) {
      selectCompositionKeyframe(channel.target, channel.property, tick)
      if (seek) setCompositionPlayhead(keyframeCompositionTick(channel, tick))
    }
    menuReturnFocus = event.currentTarget instanceof HTMLElement ? event.currentTarget : document.activeElement instanceof HTMLElement ? document.activeElement : null
    elasticAmplitude = typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'elastic' ? menuInterpolation.amplitude : 1
    elasticPeriod = typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'elastic' ? menuInterpolation.period : 0.3
    bezierP1 = typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'bezier' ? [...menuInterpolation.p1] : [0.25, 0.1]
    bezierP2 = typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'bezier' ? [...menuInterpolation.p2] : [0.25, 1]
    menu = { x: Math.max(8, Math.min(event.clientX, window.innerWidth - 180)), y: Math.max(8, Math.min(event.clientY, window.innerHeight - 180)) }
  }
  type Previews = typeof compositionState.ui.keyframePreviews
  function clearOwnedPreviews(owned: Previews): void {
    if (owned.length) compositionState.ui.keyframePreviews = compositionState.ui.keyframePreviews.filter((current) => !owned.some((entry) => entry.value === current.value && entry.property === current.property && JSON.stringify(entry.target) === JSON.stringify(current.target)))
  }
  function publishPreviews(previews: Previews): Previews {
    previews = linkedScaleAutomationPatches(previews) as Previews
    const ids = previews.map((preview) => JSON.stringify([preview.target, preview.property]))
    compositionState.ui.keyframePreviews = [...compositionState.ui.keyframePreviews.filter((current) => !ids.includes(JSON.stringify([current.target, current.property]))), ...previews]
    return compositionState.ui.keyframePreviews.filter((current) => ids.includes(JSON.stringify([current.target, current.property])))
  }
  let ownedMenuPreviews: Previews = []
  function clearMenuPreview(): void { clearOwnedPreviews(ownedMenuPreviews); ownedMenuPreviews = [] }
  function closeMenu(restoreFocus = false): void {
    const opener = menuReturnFocus
    menu = null; clearMenuPreview(); menuReturnFocus = null
    if (restoreFocus && opener?.isConnected) opener.focus()
  }
  function menuKeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeMenu(true); return }
    if (event.key === 'Tab') {
      event.stopPropagation()
      const last = menuElement?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')
      if (!event.shiftKey && event.currentTarget === last?.[last.length - 1]) {
        event.preventDefault(); menuElement?.querySelector<HTMLInputElement>('input')?.focus(); return
      }
      closeMenu(true); return
    }
    if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return
    event.preventDefault(); event.stopPropagation()
    const items = [...(menuElement?.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]') ?? [])]
    const current = items.indexOf(event.currentTarget as HTMLButtonElement)
    const index = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length
    items[index]?.focus()
  }
  function previewEasing(interpolation: CompositionInterpolation): void {
    if (!menu) return
    try {
      const previews = compositionState.ui.selectedKeyframes.map((subject) => {
        const current = resolveAutomation(subject.target, subject.property)[1]
        if (!current) throw new Error('Selected keyframe no longer exists')
        return { ...subject, value: updateSelectedKeyframeInterpolation(current, subject.ticks, interpolation) }
      })
      clearMenuPreview()
      ownedMenuPreviews = publishPreviews(previews)
    } catch (error) { clearMenuPreview(); compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function commitElasticParameters(): void {
    if (!menu || typeof menuInterpolation !== 'object' || menuInterpolation?.kind !== 'elastic') return
    if (menuInterpolation.amplitude === elasticAmplitude && menuInterpolation.period === elasticPeriod) { clearMenuPreview(); return }
    try { setCompositionKeyframeSelectionInterpolation(elasticMode(menuInterpolation.dir)); clearMenuPreview() }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function elasticParameterKey(event: KeyboardEvent): void {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeMenu(true) }
    if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); commitElasticParameters() }
  }
  function previewBezierPoint(point: 'p1' | 'p2', axis: 0 | 1, value: number): void {
    if (point === 'p1') bezierP1[axis] = value
    else bezierP2[axis] = value
    previewEasing(bezierMode())
  }
  function commitBezierPoints(): void {
    if (!menu || typeof menuInterpolation !== 'object' || menuInterpolation?.kind !== 'bezier') return
    if (menuInterpolation.p1.every((n, i) => n === bezierP1[i]) && menuInterpolation.p2.every((n, i) => n === bezierP2[i])) { clearMenuPreview(); return }
    try { setCompositionKeyframeSelectionInterpolation(bezierMode()); clearMenuPreview() }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function bezierParameterKey(event: KeyboardEvent): void {
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeMenu(true) }
    if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); commitBezierPoints() }
  }
  function easeSelection(interpolation: CompositionInterpolation): void {
    try { setCompositionKeyframeSelectionInterpolation(interpolation); closeMenu(true) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  let drag: { pointerId: number; x: number; element: HTMLElement; document: Composition; subjects: readonly CompositionKeyframeSubject[]; groups: (SelectedKeyframeTrack & { timelineStartTicks: number })[]; anchor: number | null; hit: number; active: boolean; valid: boolean; previews: Previews; delta: number; factor: number } | null = null
  let ownerActive = $state(false)
  const channels = $derived(keyframeChannels(track).map((channel) => {
    const preview = compositionState.ui.keyframePreviews.find((item) => JSON.stringify([item.target, item.property]) === channel.id)
    return preview?.value.mode === 'keyframes' ? { ...channel, value: preview.value } : channel
  }))
  function selected(channel: KeyframeChannel, tick: number): boolean {
    const subject = compositionState.ui.keyframePreviews.find((item) => JSON.stringify([item.target, item.property]) === channel.id) ?? compositionState.ui.selectedKeyframes.find((item) => JSON.stringify([item.target, item.property]) === channel.id)
    return !!subject?.ticks.includes(tick)
  }
  function begin(event: PointerEvent, channel: KeyframeChannel, tick: number): void {
    if (event.button !== 0 || channel.locked) return
    event.stopPropagation()
    event.preventDefault()
    if (event.shiftKey) { selectCompositionKeyframe(channel.target, channel.property, tick, true); return }
    if (!selected(channel, tick)) selectCompositionKeyframe(channel.target, channel.property, tick)
    const subjects = compositionState.ui.selectedKeyframes.map((subject) => ({ ...subject, ticks: [...subject.ticks] }))
    const groups = subjects.map((subject) => {
      const resolved = resolveAutomation(subject.target, subject.property)
      return { id: JSON.stringify([subject.target, subject.property]), value: resolved[1]!, selectedTicks: subject.ticks, durationTicks: clipDurationTicks(resolved[0]), timelineStartTicks: resolved[0].timelineStartTicks }
    })
    const extent = compositionKeyframeSelectionExtent(subjects)
    const hit = keyframeCompositionTick(channel, tick)
    const anchor = event.altKey && extent && extent.distinct >= 2 ? hit === extent.first ? extent.last : hit === extent.last ? extent.first : null : null
    drag = { pointerId: event.pointerId, x: event.clientX, element: (event.currentTarget as HTMLElement).parentElement!, document: compositionState.document, subjects, groups, anchor, hit, active: false, valid: true, previews: [], delta: 0, factor: 1 }
    ownerActive = true
    compositionState.ui.keyframePointerReleasePending = true
    drag.element.setPointerCapture?.(event.pointerId)
  }
  function move(event: PointerEvent): void {
    if (!drag || drag.pointerId !== event.pointerId) return
    drag.active ||= Math.abs(event.clientX - drag.x) >= 3
    if (!drag.active) return
    const delta = Math.round((event.clientX - drag.x) * COMPOSITION_TIME_BASE / compositionState.ui.zoomPxPerSecond)
    try {
      const grid = canvasTimeGrid(compositionState.document.canvas)
      const result = drag.anchor === null ? moveSelectedKeyframeTracks(drag.groups, delta, grid) : scaleSelectedKeyframeTracks(drag.groups, drag.anchor, (drag.hit + delta - drag.anchor) / (drag.hit - drag.anchor), grid)
      drag.valid = true
      drag.delta = delta
      if ('factor' in result) drag.factor = result.factor
      clearOwnedPreviews(drag.previews)
      drag.previews = publishPreviews(drag.subjects.map((subject, index) => ({ ...subject, ticks: result.groups[index]!.selectedTicks, value: result.groups[index]!.value })))
    } catch (error) { drag.valid = false; clearOwnedPreviews(drag.previews); drag.previews = []; compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function end(event?: PointerEvent, cancel = false): void {
    if (!drag || event && drag.pointerId !== event.pointerId) return
    if (event && !cancel) move(event)
    const press = drag
    if (event) compositionState.ui.keyframePointerReleasePending = false
    drag = null
    ownerActive = false
    clearOwnedPreviews(press.previews)
    if (press.element.hasPointerCapture?.(press.pointerId)) press.element.releasePointerCapture(press.pointerId)
    if (!cancel && press.active && press.valid) try {
      if (compositionState.document !== press.document) throw new Error('Проект изменился во время переноса ключей')
      compositionState.ui.selectedKeyframes = [...(press.anchor === null ? moveCompositionKeyframeSelection(press.subjects, press.delta) : scaleCompositionKeyframeSelection(press.subjects, press.anchor, press.factor))]
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  $effect(() => () => { closeMenu(); end(undefined, true) })
  function keydown(event: KeyboardEvent): void {
    const boundCommand = shortcutState.capturingCommandId ? null : shortcutCommandForEvent('composition', event)
    if (boundCommand === 'composition.copy' || boundCommand === 'composition.paste') {
      if (executeCompositionCommand(boundCommand)) { event.preventDefault(); event.stopPropagation() }
      return
    }
    if (event.key === 'Escape') {
      if (menu) { event.stopPropagation(); closeMenu(true); return }
      if (drag) { event.stopPropagation(); end(undefined, true) }
      else compositionState.ui.selectedKeyframes = []
      return
    }
    const command = event.key === 'Delete' || event.key === 'Backspace' ? 'composition.delete' : event.key === 'ArrowLeft' ? 'composition.nudgePrevious' : event.key === 'ArrowRight' ? 'composition.nudgeNext' : null
    if (command) { event.preventDefault(); event.stopPropagation(); executeCompositionCommand(command) }
  }
</script>
<svelte:window onpointerdown={(event) => { if (!(event.target as Element).closest('.composition-tangent-menu')) tangentMenu = null; if (!(event.target as HTMLElement).closest('.composition-keyframe-menu')) closeMenu() }} onpointermove={move} onpointerup={(event) => end(event)} onpointercancel={(event) => end(event, true)} onkeydown={(event) => { if (event.key === 'Escape') { if (tangentMenu) { event.preventDefault(); tangentMenu.element.focus(); tangentMenu = null }; closeMenu(); if (ownerActive) end(undefined, true) } }} />
{#if channels.length}
  <div class="composition-track-row composition-keyframe-row" data-keyframe-track={track.id}>
    <div class="composition-track-head"><button class="btn ghost sm" type="button" aria-expanded={expanded} onclick={() => { expanded = !expanded }}>Ключи {expanded ? '▾' : '▸'}</button>
      {#if expanded}<div class="composition-keyframe-ranges">{#each channels as channel (channel.id)}{@const range = channelRange(channel)}<small title={channel.target.clipId}>{channel.property} {range.minimum.toFixed(2)}…{range.maximum.toFixed(2)}</small>{/each}</div>{/if}
    </div>
    <div class="composition-keyframe-lane" data-expanded={expanded} role="listbox" aria-multiselectable="true" aria-label={`Ключевые кадры дорожки ${track.name}`} tabindex="0" onkeydown={keydown} style:width={`${width}px`} style:height={`${expanded ? 120 : 24}px`} onlostpointercapture={() => end(undefined, true)} onpointerup={(event) => { if (drag) { event.stopPropagation(); end(event) } }}>
      {#each channels as channel (channel.id)}
        {@const range = channelRange(channel)}
        {#if expanded}
          <svg class="composition-keyframe-curve" data-keyframe-curve={channel.id} width={width} height="120" role="group" aria-label={`Кривая ${channel.property}`}>
            <polyline class="composition-keyframe-tail" points={keyframeTailPoints(channel, 'before', compositionState.ui.zoomPxPerSecond, 120, range)} />
            <polyline points={keyframeCurvePoints(channel, compositionState.ui.zoomPxPerSecond, 120, range)} />
            {#each channel.value.track.keyframes.slice(0, -1) as left, index (left.tick)}
              {@const segmentChannel = { ...channel, value: { ...channel.value, track: { ...channel.value.track, keyframes: [left, channel.value.track.keyframes[index + 1]!] } } }}
              <!-- Keyboard users open the same menu on the owner diamond. -->
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <polyline class="composition-keyframe-segment-hit" data-segment-tick={left.tick} points={keyframeCurvePoints(segmentChannel, compositionState.ui.zoomPxPerSecond, 120, range)} oncontextmenu={(event) => openMenu(event, channel, left.tick, false)} />
            {/each}
            <TimelineTangentHandles {channel} zoom={compositionState.ui.zoomPxPerSecond} {range} onaxis={freezeAxis} onmenu={openTangentMenu} />
            <polyline class="composition-keyframe-tail" points={keyframeTailPoints(channel, 'after', compositionState.ui.zoomPxPerSecond, 120, range)} />
          </svg>
        {/if}
        {#each channel.value.track.keyframes as key (key.tick)}
          <button class="composition-keyframe-diamond" data-keyframe-channel={channel.id} data-keyframe-tick={key.tick} class:selected={selected(channel, key.tick)} type="button" role="option" aria-selected={selected(channel, key.tick)} disabled={channel.locked} style:left={`${keyframeCompositionTick(channel, key.tick) / COMPOSITION_TIME_BASE * compositionState.ui.zoomPxPerSecond}px`} style:top={`${expanded ? keyframeValueY(key.value, range, 120) : 12}px`} aria-label={`Ключ ${channel.target.clipId} ${channel.property} ${key.tick}`} title={`${channel.property}: ${key.value}`} oncontextmenu={(event) => openMenu(event, channel, key.tick)} onpointerdown={(event) => begin(event, channel, key.tick)} onlostpointercapture={() => end(undefined, true)} onkeydown={(event) => {
            if (event.key === 'ContextMenu' || event.key === 'F10' && event.shiftKey) {
              event.preventDefault(); event.stopPropagation()
              const bounds = (event.currentTarget as HTMLElement).getBoundingClientRect()
              openMenu(new MouseEvent('contextmenu', { clientX: bounds.left + bounds.width / 2, clientY: bounds.bottom }), channel, key.tick)
              menuReturnFocus = event.currentTarget as HTMLElement
              void flushUi().then(() => { if (menu) menuElement?.querySelector<HTMLButtonElement>('[role="menuitemradio"]')?.focus() })
            } else keydown(event)
          }} onclick={(event) => { event.stopPropagation(); if (event.detail === 0) selectCompositionKeyframe(channel.target, channel.property, key.tick, event.shiftKey) }}>◆</button>
        {/each}
      {/each}
    </div>
  </div>
{/if}

{#if menu}
  <div bind:this={menuElement} class="composition-keyframe-menu" role="menu" aria-label="Сглаживание выбранных ключей" style:left={`${menu.x}px`} style:top={`${menu.y}px`} style:max-height={`${Math.max(96, window.innerHeight - menu.y - 8)}px`} style:overflow="auto">
    {#each COMPOSITION_INTERPOLATIONS as interpolation (interpolation)}
      <button class="btn ghost sm" type="button" role="menuitemradio" aria-checked={menuInterpolation === interpolation} onkeydown={menuKeydown} onpointerenter={() => previewEasing(interpolation)} onpointerleave={clearMenuPreview} onfocus={() => previewEasing(interpolation)} onblur={clearMenuPreview} onclick={() => easeSelection(interpolation)}>{interpolation}</button>
    {/each}
    <button class="btn ghost sm" type="button" role="menuitemradio" aria-checked={typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'bezier' && !bezierPresetId(menuInterpolation)} onkeydown={menuKeydown} onpointerenter={() => previewEasing(bezierMode())} onpointerleave={clearMenuPreview} onfocus={() => previewEasing(bezierMode())} onblur={clearMenuPreview} onclick={() => easeSelection(bezierMode())}>Bézier</button>
    {#each BEZIER_PRESETS as preset (preset.id)}
      <button class="btn ghost sm" type="button" role="menuitemradio" aria-label={`Пресет ${preset.id}`} aria-checked={typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'bezier' && bezierPresetId(menuInterpolation) === preset.id} onkeydown={menuKeydown} onpointerenter={() => previewEasing(preset.interpolation)} onpointerleave={clearMenuPreview} onfocus={() => previewEasing(preset.interpolation)} onblur={clearMenuPreview} onclick={() => easeSelection(preset.interpolation)}>{preset.id}</button>
    {/each}
    {#each elasticDirections as dir (dir)}
      <button class="btn ghost sm" type="button" role="menuitemradio" aria-checked={typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'elastic' && menuInterpolation.dir === dir} onkeydown={menuKeydown} onpointerenter={() => previewEasing(elasticMode(dir))} onpointerleave={clearMenuPreview} onfocus={() => previewEasing(elasticMode(dir))} onblur={clearMenuPreview} onclick={() => easeSelection(elasticMode(dir))}>Elastic {dir === 'in_out' ? 'InOut' : dir === 'in' ? 'In' : 'Out'}</button>
    {/each}
    {#if typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'bezier'}
      {#each ['p1', 'p2'] as point (point)}
        {#each [0, 1] as axis (axis)}
          <label>{point} {axis === 0 ? 'X' : 'Y'}<input aria-label={`Bézier ${point} ${axis === 0 ? 'X' : 'Y'} выбранных ключей`} type="number" min={axis === 0 ? 0 : undefined} max={axis === 0 ? 1 : undefined} step="0.01" value={point === 'p1' ? bezierP1[axis] : bezierP2[axis]} oninput={(event) => previewBezierPoint(point as 'p1' | 'p2', axis as 0 | 1, event.currentTarget.valueAsNumber)} onchange={commitBezierPoints} onkeydown={bezierParameterKey} /></label>
        {/each}
      {/each}
    {:else}
      <label>Амплитуда<input aria-label="Амплитуда Elastic выбранных ключей" type="number" min="1" step="0.1" value={elasticAmplitude} oninput={(event) => { elasticAmplitude = event.currentTarget.valueAsNumber; if (typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'elastic') previewEasing(elasticMode(menuInterpolation.dir)) }} onkeydown={elasticParameterKey} onchange={commitElasticParameters} /></label>
      <label>Период<input aria-label="Период Elastic выбранных ключей" type="number" min="0" step="0.01" value={elasticPeriod} oninput={(event) => { elasticPeriod = event.currentTarget.valueAsNumber; if (typeof menuInterpolation === 'object' && menuInterpolation?.kind === 'elastic') previewEasing(elasticMode(menuInterpolation.dir)) }} onkeydown={elasticParameterKey} onchange={commitElasticParameters} /></label>
    {/if}
  </div>
{/if}

{#if tangentMenu}
  <div class="composition-keyframe-menu composition-tangent-menu" role="menu" aria-label="Непрерывность касательной" style={`position:fixed;left:${tangentMenu.x}px;top:${tangentMenu.y}px;z-index:1000`}>
    {#each ['smooth','broken'] as continuity (continuity)}
      <button class="btn ghost sm" type="button" role="menuitemradio" aria-checked={tangentMenu.continuity === continuity} onkeydown={tangentMenuKey} onclick={() => chooseContinuity(continuity as 'smooth' | 'broken')}>{continuity === 'smooth' ? 'Smooth' : 'Broken'}</button>
    {/each}
  </div>
{/if}
