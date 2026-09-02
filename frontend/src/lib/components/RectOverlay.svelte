<script lang="ts">
  import { sceneLayerAttributes } from '$lib/features/canvas/scene.js'
  import { CanvasToolMachine } from '$lib/features/canvas/toolMachine.js'
  import { state } from '$lib/state/store.svelte.js'
  import { eventListener, listenMany } from '../../composables/globalListeners.js'

  interface OverlayRect { x: number; y: number; w: number; h: number }
  interface Props {
    rect: OverlayRect
    color?: string
    mode?: 'crop' | 'mask'
    onrectchange?: (rect: OverlayRect) => void
    oninteractionstart?: () => void
    oninteractionend?: () => void
  }

  let {
    rect,
    color = 'var(--accent)',
    mode = 'crop',
    onrectchange,
    oninteractionstart,
    oninteractionend,
  }: Props = $props()
  let root: HTMLElement
  const MIN = 16
  type DragMode = 'move' | 'nw' | 'ne' | 'sw' | 'se'
  const toolMachine = new CanvasToolMachine<DragMode>()
  const overlayLayerAttributes = sceneLayerAttributes('overlays', true)
  const handleLayerAttributes = sceneLayerAttributes('handles', true)
  let dragMode: DragMode | null = null
  let startX = 0
  let startY = 0
  let original: OverlayRect = { x: 0, y: 0, w: 0, h: 0 }
  let disposeDragListeners = () => {}

  const clamp = (value: number, min: number, max: number) => Math.max(min, Math.min(value, max))
  const dims = () => ({ width: state.video?.width || 1, height: state.video?.height || 1 })
  function normalize(value: OverlayRect): OverlayRect {
    const { width, height } = dims()
    const minWidth = Math.min(MIN, width)
    const minHeight = Math.min(MIN, height)
    const finite = (input: number, fallback: number) => Number.isFinite(input) ? input : fallback
    const w = clamp(Math.round(finite(value.w, minWidth)), minWidth, width)
    const h = clamp(Math.round(finite(value.h, minHeight)), minHeight, height)
    return {
      x: clamp(Math.round(finite(value.x, 0)), 0, width - w),
      y: clamp(Math.round(finite(value.y, 0)), 0, height - h),
      w,
      h,
    }
  }
  let normalized = $derived(normalize(rect))
  let rectStyle = $derived([
    `left:${normalized.x / dims().width * 100}%`,
    `top:${normalized.y / dims().height * 100}%`,
    `width:${normalized.w / dims().width * 100}%`,
    `height:${normalized.h / dims().height * 100}%`,
    `border-color:${color}`,
    `box-shadow:${mode === 'crop' ? '0 0 0 9999px rgba(0, 0, 0, 0.45)' : 'none'}`,
    `background:${mode === 'mask' ? 'rgba(255, 80, 80, 0.32)' : 'transparent'}`,
  ].join(';'))

  function sourceDelta(dx: number, dy: number): { dx: number; dy: number } | null {
    if (!root) return null
    const bounds = root.getBoundingClientRect()
    if (bounds.width <= 0 || bounds.height <= 0) return null
    const { width, height } = dims()
    return { dx: dx * width / bounds.width, dy: dy * height / bounds.height }
  }
  function begin(nextMode: DragMode, event: PointerEvent): void {
    const target = event.currentTarget as HTMLElement
    const before = toolMachine.snapshot()
    toolMachine.begin(nextMode, event.pointerId, target)
    if (before.pointerId === event.pointerId && before.mode === nextMode && before.state !== 'idle') return
    dragMode = nextMode
    startX = event.clientX
    startY = event.clientY
    original = normalize(rect)
    oninteractionstart?.()
    disposeDragListeners()
    disposeDragListeners = listenMany([
      [window, 'pointermove', eventListener(onMove)],
      [window, 'pointerup', eventListener((event: PointerEvent) => stopDrag(event))],
      [window, 'pointercancel', eventListener((event: PointerEvent) => cancelDrag(event))],
      [window, 'keydown', eventListener(onWindowKeydown)],
    ])
    event.preventDefault()
    event.stopPropagation()
  }
  function onMove(event: PointerEvent): void {
    if (!dragMode) return
    const snapshot = toolMachine.move(event.pointerId)
    if (snapshot.pointerId !== event.pointerId) return
    const delta = sourceDelta(event.clientX - startX, event.clientY - startY)
    if (!delta) return stopDrag()
    const { width, height } = dims()
    const minWidth = Math.min(MIN, width)
    const minHeight = Math.min(MIN, height)
    if (dragMode === 'move') {
      onrectchange?.(normalize({ ...original, x: original.x + delta.dx, y: original.y + delta.dy }))
      return
    }
    let x1 = original.x
    let y1 = original.y
    let x2 = original.x + original.w
    let y2 = original.y + original.h
    if (dragMode.includes('w')) x1 = clamp(original.x + delta.dx, 0, x2 - minWidth)
    if (dragMode.includes('e')) x2 = clamp(original.x + original.w + delta.dx, x1 + minWidth, width)
    if (dragMode.includes('n')) y1 = clamp(original.y + delta.dy, 0, y2 - minHeight)
    if (dragMode.includes('s')) y2 = clamp(original.y + original.h + delta.dy, y1 + minHeight, height)
    onrectchange?.(normalize({ x: x1, y: y1, w: x2 - x1, h: y2 - y1 }))
  }
  function removeDragListeners(): void {
    disposeDragListeners()
    disposeDragListeners = () => {}
  }
  function stopDrag(event?: PointerEvent): void {
    const activePointer = toolMachine.snapshot().pointerId
    if (event && event.pointerId !== activePointer) return
    const wasDragging = dragMode !== null
    toolMachine.finish(activePointer)
    dragMode = null
    removeDragListeners()
    if (wasDragging) oninteractionend?.()
  }
  function cancelDrag(event?: PointerEvent): void {
    const activePointer = toolMachine.snapshot().pointerId
    if (event && event.pointerId !== activePointer) return
    const wasDragging = dragMode !== null
    toolMachine.cancel(activePointer)
    toolMachine.finish(activePointer)
    dragMode = null
    removeDragListeners()
    if (wasDragging) oninteractionend?.()
  }
  function onWindowKeydown(event: KeyboardEvent): void {
    if (event.key !== 'Escape') return
    event.preventDefault()
    cancelDrag()
  }
  function keyboardAdjust(mode: DragMode, event: KeyboardEvent): void {
    const direction = event.key
    if (!['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown'].includes(direction)) return
    event.preventDefault()
    event.stopPropagation()
    const step = event.shiftKey ? 10 : 1
    const dx = direction === 'ArrowLeft' ? -step : direction === 'ArrowRight' ? step : 0
    const dy = direction === 'ArrowUp' ? -step : direction === 'ArrowDown' ? step : 0
    const current = normalize(rect)
    let next = current
    if (mode === 'move') next = normalize({ ...current, x: current.x + dx, y: current.y + dy })
    else {
      const west = mode.includes('w')
      const north = mode.includes('n')
      const x1 = west ? current.x + dx : current.x
      const y1 = north ? current.y + dy : current.y
      const x2 = west ? current.x + current.w : current.x + current.w + dx
      const y2 = north ? current.y + current.h : current.y + current.h + dy
      next = normalize({ x: x1, y: y1, w: x2 - x1, h: y2 - y1 })
    }
    oninteractionstart?.()
    onrectchange?.(next)
    oninteractionend?.()
  }
  $effect(() => () => stopDrag())
</script>

<div bind:this={root} class="crop-overlay" data-scene-layer="guides" data-hit-test="passthrough">
  <div
    {...overlayLayerAttributes}
    class="crop-rect"
    style={rectStyle}
    role="button"
    tabindex="0"
    aria-label={`Переместить область кадра. X ${normalized.x}, Y ${normalized.y}, ширина ${normalized.w}, высота ${normalized.h}`}
    onpointerdown={(event) => begin('move', event)}
    onkeydown={(event) => keyboardAdjust('move', event)}
  >
    {#each ['nw', 'ne', 'sw', 'se'] as handle (handle)}
      <span
        {...handleLayerAttributes}
        class={`crop-handle ${handle}`}
        style:border-color={color}
        role="button"
        tabindex="0"
        aria-label={`Изменить размер области: ${handle}`}
        onpointerdown={(event) => begin(handle as DragMode, event)}
        onkeydown={(event) => keyboardAdjust(handle as DragMode, event)}
      ></span>
    {/each}
  </div>
</div>
