<script lang="ts">
  import { onDestroy, tick } from 'svelte'
  import { SvelteSet } from 'svelte/reactivity'
  import {
    cloneCurves,
    identityCurve,
    isIdentityCurve,
    MAX_CURVE_POINTS,
    sampleCurvePchip,
    sanitizeCurve,
  } from '$lib/domain/edit.js'
  import type { ColorCurves, CurvePoint } from '$lib/types.js'

  type CurveChannel = keyof ColorCurves
  type InteractionSource = 'pointer' | 'numeric'

  interface PointerDrag {
    channel: CurveChannel
    index: number
    pointerId: number
    points: CurvePoint[]
  }

  interface Props {
    value: ColorCurves
    onchange?: (value: ColorCurves) => void
    oninteractionstart?: () => void
    oninteractionend?: () => void
  }
  let { value, onchange, oninteractionstart, oninteractionend }: Props = $props()
  const channels: Array<{ key: CurveChannel; label: string; longLabel: string; color: string }> = [
    { key: 'master', label: 'Общая', longLabel: 'Общая', color: 'var(--text)' },
    { key: 'red', label: 'R', longLabel: 'Красная (R)', color: '#ff626c' },
    { key: 'green', label: 'G', longLabel: 'Зелёная (G)', color: '#3ecf8e' },
    { key: 'blue', label: 'B', longLabel: 'Синяя (B)', color: '#5d94ff' },
  ]
  const padding = 8
  const size = 240
  const ticks = [64, 128, 192]
  const generatedId = $props.id()
  const instanceId = generatedId.replace(/:/g, '')
  let tabList: HTMLElement
  let plot: SVGSVGElement
  let selectedXInput: HTMLInputElement
  let activeChannel = $state<CurveChannel>('master')
  let selectedIndex = $state(0)
  const interactionSources = new SvelteSet<InteractionSource>()
  let pointerDrag: PointerDrag | null = null
  let activeOption = $derived(channels.find((channel) => channel.key === activeChannel) ?? channels[0]!)
  let activePoints = $derived(sanitizeCurve(value[activeChannel]))
  let selectedPoint = $derived(activePoints[selectedIndex] ?? activePoints[0]!)
  let endpoint = $derived(selectedIndex === 0 || selectedIndex === activePoints.length - 1)
  let selectedXMin = $derived(activePoints[selectedIndex - 1]?.x + 1 || 0)
  let selectedXMax = $derived(activePoints[selectedIndex + 1]?.x - 1 || 255)
  let activeIdentity = $derived(isIdentityCurve(activePoints))
  let polyline = $derived(sampleCurvePchip(activePoints).map((point) => `${toX(point.x)},${toY(point.y)}`).join(' '))

  const clamp = (number: number, min: number, max: number) => Math.max(min, Math.min(number, max))
  const clampValue = (number: number) => Math.round(clamp(number, 0, 255))
  const toX = (input: number) => padding + input / 255 * size
  const toY = (output: number) => padding + (255 - output) / 255 * size

  const clonePoints = (points: CurvePoint[]) => points.map((point) => ({ ...point }))
  const tabId = (channel: CurveChannel) => `${instanceId}-curve-tab-${channel}`
  const panelId = (channel: CurveChannel) => `${instanceId}-curve-panel-${channel}`

  $effect(() => {
    const length = activePoints.length
    selectedIndex = clamp(selectedIndex, 0, Math.max(0, length - 1))
  })

  function beginInteraction(source: InteractionSource): void {
    if (interactionSources.has(source)) return
    if (interactionSources.size === 0) oninteractionstart?.()
    interactionSources.add(source)
  }

  function endInteraction(source: InteractionSource): void {
    if (!interactionSources.delete(source)) return
    if (interactionSources.size === 0) oninteractionend?.()
  }

  function endAllInteractions(): void {
    if (interactionSources.size === 0) return
    interactionSources.clear()
    oninteractionend?.()
  }

  function replaceChannel(channel: CurveChannel, points: CurvePoint[]): void {
    const next = cloneCurves(value)
    next[channel] = clonePoints(points)
    onchange?.(next)
  }

  function replace(points: CurvePoint[]): void {
    replaceChannel(activeChannel, sanitizeCurve(points))
  }

  function discrete(update: () => void): void {
    const wasIdle = interactionSources.size === 0
    if (wasIdle) oninteractionstart?.()
    update()
    if (wasIdle) oninteractionend?.()
  }

  function constrainPoint(
    points: CurvePoint[],
    index: number,
    requestedX: number,
    requestedY: number,
  ): CurvePoint {
    const current = points[index]
    if (!current) return { x: 0, y: 0 }
    const isEndpoint = index === 0 || index === points.length - 1
    const minX = points[index - 1] ? points[index - 1]!.x + 1 : current.x
    const maxX = points[index + 1] ? points[index + 1]!.x - 1 : current.x
    return {
      x: isEndpoint ? current.x : clamp(clampValue(requestedX), minX, maxX),
      y: clampValue(requestedY),
    }
  }

  function updatedPoints(
    points: CurvePoint[],
    index: number,
    requestedX: number,
    requestedY: number,
  ): CurvePoint[] {
    const next = clonePoints(points)
    next[index] = constrainPoint(next, index, requestedX, requestedY)
    return next
  }

  function pointFromPointer(event: PointerEvent): CurvePoint | null {
    if (!plot) return null
    const rect = plot.getBoundingClientRect()
    if (rect.width <= 0 || rect.height <= 0) return null
    const svgX = ((event.clientX - rect.left) / rect.width) * 256
    const svgY = ((event.clientY - rect.top) / rect.height) * 256
    return {
      x: clampValue(((svgX - padding) / size) * 255),
      y: clampValue(255 - ((svgY - padding) / size) * 255),
    }
  }

  function selectChannel(channel: CurveChannel): void {
    if (channel === activeChannel) return
    stopPointerDrag()
    endAllInteractions()
    activeChannel = channel
    selectedIndex = 0
  }

  function onTabKeydown(event: KeyboardEvent, index: number): void {
    let nextIndex: number | null = null
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') nextIndex = (index + 1) % channels.length
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') nextIndex = (index - 1 + channels.length) % channels.length
    else if (event.key === 'Home') nextIndex = 0
    else if (event.key === 'End') nextIndex = channels.length - 1
    if (nextIndex === null) return
    event.preventDefault()
    selectChannel(channels[nextIndex]!.key)
    void tick().then(() => tabList?.querySelectorAll<HTMLButtonElement>('[role="tab"]').item(nextIndex!).focus())
  }

  function startPointerDrag(
    index: number,
    event: PointerEvent,
    points: CurvePoint[] = clonePoints(activePoints),
  ): void {
    if (event.button !== 0) return
    stopPointerDrag()
    selectedIndex = index
    pointerDrag = {
      channel: activeChannel,
      index,
      pointerId: event.pointerId,
      points: clonePoints(points),
    }
    beginInteraction('pointer')
    plot?.setPointerCapture?.(event.pointerId)
    window.addEventListener('pointermove', onPointerMove)
    window.addEventListener('pointerup', onPointerUp)
    window.addEventListener('pointercancel', onPointerUp)
    event.preventDefault()
  }

  function onPlotPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return
    const point = pointFromPointer(event)
    if (!point) return
    const points = clonePoints(activePoints)
    let index = points.findIndex((candidate) => candidate.x === point.x)
    let added = false
    if (index < 0) {
      if (points.length >= MAX_CURVE_POINTS) return
      points.push(point)
      points.sort((left, right) => left.x - right.x)
      index = points.findIndex((candidate) => candidate.x === point.x)
      added = true
    }
    startPointerDrag(index, event, points)
    if (added) replaceChannel(activeChannel, points)
  }

  function onPointerMove(event: PointerEvent): void {
    const drag = pointerDrag
    if (!drag || event.pointerId !== drag.pointerId) return
    const requested = pointFromPointer(event)
    if (!requested) return
    const current = drag.points[drag.index]
    if (!current) return
    const points = updatedPoints(drag.points, drag.index, requested.x, requested.y)
    const next = points[drag.index]!
    if (current.x === next.x && current.y === next.y) return
    drag.points = points
    replaceChannel(drag.channel, points)
  }

  function onPointerUp(event: PointerEvent): void {
    if (!pointerDrag || event.pointerId !== pointerDrag.pointerId) return
    stopPointerDrag()
  }

  function stopPointerDrag(): void {
    const drag = pointerDrag
    if (!drag) return
    pointerDrag = null
    window.removeEventListener('pointermove', onPointerMove)
    window.removeEventListener('pointerup', onPointerUp)
    window.removeEventListener('pointercancel', onPointerUp)
    if (plot?.hasPointerCapture?.(drag.pointerId)) plot.releasePointerCapture(drag.pointerId)
    endInteraction('pointer')
  }

  function addPoint(): void {
    if (activePoints.length >= MAX_CURVE_POINTS) return
    const points = clonePoints(activePoints)
    let gapIndex = 0
    for (let i = 1; i < points.length - 1; i += 1) if (points[i + 1]!.x - points[i]!.x > points[gapIndex + 1]!.x - points[gapIndex]!.x) gapIndex = i
    const left = points[gapIndex]!
    const right = points[gapIndex + 1]!
    if (right.x - left.x <= 1) return
    const x = Math.round((left.x + right.x) / 2)
    const y = Math.round(left.y + (right.y - left.y) * ((x - left.x) / (right.x - left.x)))
    discrete(() => { points.splice(gapIndex + 1, 0, { x, y }); selectedIndex = gapIndex + 1; replace(points) })
    void tick().then(() => selectedXInput?.focus())
  }
  function deletePoint(): void {
    if (endpoint) return
    const points = clonePoints(activePoints)
    discrete(() => { points.splice(selectedIndex, 1); selectedIndex = Math.min(selectedIndex, points.length - 1); replace(points) })
  }
  function resetChannel(): void {
    if (activeIdentity) return
    discrete(() => { selectedIndex = 0; replace(identityCurve()) })
  }

  function onPointKeydown(index: number, event: KeyboardEvent): void {
    selectedIndex = index
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault()
      return
    }
    if (event.key === 'Delete' || event.key === 'Backspace') {
      if (index === 0 || index === activePoints.length - 1) return
      event.preventDefault()
      deletePoint()
    }
  }

  function updateCoordinate(axis: 'x' | 'y', event: Event): void {
    const number = (event.currentTarget as HTMLInputElement).valueAsNumber
    if (!Number.isFinite(number)) return
    const points = clonePoints(activePoints)
    const point = points[selectedIndex]
    if (!point) return
    const requestedX = axis === 'x' ? number : point.x
    const requestedY = axis === 'y' ? number : point.y
    replace(updatedPoints(points, selectedIndex, requestedX, requestedY))
  }

  function blurOnEnter(event: KeyboardEvent): void {
    if (event.key === 'Enter') (event.currentTarget as HTMLInputElement).blur()
  }

  onDestroy(() => {
    stopPointerDrag()
    endAllInteractions()
  })
</script>

<div class="curves-editor">
  <div bind:this={tabList} class="curve-tabs" role="tablist" aria-label="Канал кривой">
    {#each channels as channel, index (channel.key)}
      <button
        id={tabId(channel.key)}
        type="button"
        class={`curve-tab channel-${channel.key}`}
        role="tab"
        aria-controls={panelId(channel.key)}
        aria-selected={activeChannel === channel.key}
        aria-label={channel.longLabel}
        tabindex={activeChannel === channel.key ? 0 : -1}
        onclick={() => selectChannel(channel.key)}
        onkeydown={(event) => onTabKeydown(event, index)}
      >
        <span class="channel-swatch" aria-hidden="true"></span>{channel.label}
      </button>
    {/each}
  </div>
  <div id={panelId(activeChannel)} class="curve-panel" role="tabpanel" aria-labelledby={tabId(activeChannel)}>
    <p id={`${instanceId}-curve-help`} class="visually-hidden">
      Нажмите на график или кнопку «Добавить точку». Выберите точку и измените её координаты в
      полях «Вход» и «Выход». Клавиши Delete или Backspace удаляют внутреннюю точку.
    </p>
    <div class="curve-toolbar">
      <span class="point-count" aria-live="polite">{activePoints.length} / {MAX_CURVE_POINTS} точек</span>
      <button type="button" class="curve-action" disabled={activePoints.length >= MAX_CURVE_POINTS} onclick={addPoint}>Добавить точку</button>
      <button type="button" class="curve-action" disabled={endpoint} onclick={deletePoint}>Удалить точку</button>
      <button type="button" class="curve-action" disabled={activeIdentity} onclick={resetChannel}>Сбросить: {activeOption.label}</button>
    </div>
    <div class="curve-plot-frame">
      <svg
        bind:this={plot}
        class="curve-plot"
        style:color={activeOption.color}
        viewBox="0 0 256 256"
        role="group"
        aria-label={`${activeOption.longLabel} тоновая кривая`}
        aria-describedby={`${instanceId}-curve-help`}
        onpointerdown={onPlotPointerDown}
      >
        <rect class="plot-background" x={padding} y={padding} width={size} height={size} rx="2" />
        <g class="plot-guides" aria-hidden="true">
          {#each ticks as tick (tick)}<line x1={toX(tick)} x2={toX(tick)} y1={padding} y2={padding + size} /><line x1={padding} x2={padding + size} y1={toY(tick)} y2={toY(tick)} />{/each}
          <line class="identity-line" x1={toX(0)} y1={toY(0)} x2={toX(255)} y2={toY(255)} />
        </g>
        <polyline class="curve-line" points={polyline} aria-hidden="true" />
        {#each activePoints as point, index (`${point.x}-${index}`)}
          <g
            class:selected={selectedIndex === index}
            class="curve-point"
            transform={`translate(${toX(point.x)} ${toY(point.y)})`}
            role="button"
            tabindex="0"
            aria-label={`${activeOption.longLabel}: точка ${index + 1}, вход ${point.x}, выход ${point.y}`}
            onfocus={() => { selectedIndex = index }}
            onpointerdown={(event) => { event.stopPropagation(); startPointerDrag(index, event) }}
            onkeydown={(event) => onPointKeydown(index, event)}
          >
            <circle class="point-hit-area" r="12" /><circle class="point-focus-ring" r="8" /><circle class="point-dot" r="4.5" />
          </g>
        {/each}
      </svg>
    </div>
    <div class="point-controls" aria-label="Координаты выбранной точки кривой">
      <label>
        <span>Вход (X)</span>
        <input bind:this={selectedXInput} type="number" inputmode="numeric" step="1" min={selectedXMin} max={selectedXMax} value={selectedPoint.x} disabled={endpoint} onfocus={() => beginInteraction('numeric')} onchange={(event) => updateCoordinate('x', event)} onkeydown={blurOnEnter} onblur={() => endInteraction('numeric')} />
      </label>
      <label>
        <span>Выход (Y)</span>
        <input type="number" inputmode="numeric" step="1" min="0" max="255" value={selectedPoint.y} onfocus={() => beginInteraction('numeric')} onchange={(event) => updateCoordinate('y', event)} onkeydown={blurOnEnter} onblur={() => endInteraction('numeric')} />
      </label>
    </div>
  </div>
</div>
