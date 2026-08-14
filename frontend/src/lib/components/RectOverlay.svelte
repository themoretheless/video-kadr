<script lang="ts">
  import { onDestroy } from 'svelte'
  import { state } from '$lib/state/store.svelte.js'

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
  let dragMode: DragMode | null = null
  let startX = 0
  let startY = 0
  let original: OverlayRect = { x: 0, y: 0, w: 0, h: 0 }

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
    dragMode = nextMode
    startX = event.clientX
    startY = event.clientY
    original = normalize(rect)
    oninteractionstart?.()
    ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', stopDrag)
    event.preventDefault()
    event.stopPropagation()
  }
  function onMove(event: PointerEvent): void {
    if (!dragMode) return
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
  function stopDrag(): void {
    const wasDragging = dragMode !== null
    dragMode = null
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', stopDrag)
    if (wasDragging) oninteractionend?.()
  }
  onDestroy(stopDrag)
</script>

<div bind:this={root} class="crop-overlay">
  <div class="crop-rect" style={rectStyle} role="group" aria-label="Редактируемая область кадра" onpointerdown={(event) => begin('move', event)}>
    {#each ['nw', 'ne', 'sw', 'se'] as handle (handle)}
      <span
        class={`crop-handle ${handle}`}
        style:border-color={color}
        role="button"
        tabindex="0"
        aria-label={`Изменить размер области: ${handle}`}
        onpointerdown={(event) => begin(handle as DragMode, event)}
        onkeydown={(event) => { if (event.key === 'Enter' || event.key === ' ') event.preventDefault() }}
      ></span>
    {/each}
  </div>
</div>
