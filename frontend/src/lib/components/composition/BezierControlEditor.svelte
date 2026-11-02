<script lang="ts">
  import type { CompositionBezierInterpolation } from '$lib/composition/types'
  import { sampleBezier } from '$lib/composition/bezier'
  let { value, onpreview, oncommit, oncancel, disabled = false }: {
    value: CompositionBezierInterpolation
    onpreview: (value: CompositionBezierInterpolation) => void
    oncommit: (value: CompositionBezierInterpolation) => void
    oncancel: () => void
    disabled?: boolean
  } = $props()
  let draft = $state<CompositionBezierInterpolation | null>(null)
  const shown = $derived(draft ?? value)
  let gestureRange = $state<{ low: number; high: number } | null>(null)
  const range = $derived(gestureRange ?? { low: Math.min(0, shown.p1[1], shown.p2[1]), high: Math.max(1, shown.p1[1], shown.p2[1]) })
  const y = (v: number) => 90 - (v - range.low) / (range.high - range.low) * 80
  const points = $derived(Array.from({ length: 81 }, (_, n) => `${10+n},${y(sampleBezier(shown,n/80))}`).join(' '))
  let drag: { pointerId: number; point: 'p1' | 'p2'; element: SVGElement; rect: DOMRect; low: number; high: number; origin: CompositionBezierInterpolation; x: number; y: number; active: boolean } | null = null
  function begin(event: PointerEvent, point: 'p1' | 'p2'): void {
    if (event.button !== 0 || disabled) return
    event.preventDefault(); event.stopPropagation()
    const element = event.currentTarget as SVGElement
    const rect = element.ownerSVGElement!.getBoundingClientRect()
    if (!rect.width || !rect.height) return
    gestureRange = { ...range }
    drag = { pointerId: event.pointerId, point, element, rect, low: range.low, high: range.high, origin: value, x: event.clientX, y: event.clientY, active: false }
    element.setPointerCapture?.(event.pointerId)
    onpreview(value)
  }
  function move(event: PointerEvent): void {
    if (!drag || drag.pointerId !== event.pointerId) return
    drag.active ||= Math.hypot(event.clientX-drag.x,event.clientY-drag.y) >= 3
    if (!drag.active) return
    const x = Math.max(0, Math.min(1, ((event.clientX-drag.rect.left)/drag.rect.width*100-10)/80))
    const y = drag.low + (90-(event.clientY-drag.rect.top)/drag.rect.height*100)/80*(drag.high-drag.low)
    if (!Number.isFinite(x) || !Number.isFinite(y)) return
    draft = { ...drag.origin, [drag.point]: [x,y] }
    onpreview(draft)
  }
  function finish(event?: PointerEvent, cancel = false): void {
    if (!drag || event && drag.pointerId !== event.pointerId) return
    if (event && !cancel) move(event)
    const press = drag
    const next = draft
    drag = null; draft = null; gestureRange = null
    if (press.element.hasPointerCapture?.(press.pointerId)) press.element.releasePointerCapture(press.pointerId)
    if (cancel || !next) oncancel()
    else oncommit(next)
  }
  function keyboard(event: KeyboardEvent, point: 'p1' | 'p2'): void {
    if (disabled) return
    if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish(undefined,true); return }
    if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return
    event.preventDefault(); event.stopPropagation()
    const p: [number,number] = [...value[point]]
    const step = event.shiftKey ? 0.1 : 0.01
    if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') p[0] = Math.max(0,Math.min(1,p[0]+(event.key === 'ArrowRight' ? step : -step)))
    else p[1] += event.key === 'ArrowUp' ? step : -step
    oncommit({ ...value, [point]: p })
  }
  $effect(() => { if (disabled) finish(undefined,true) })
  $effect(() => () => finish(undefined,true))
</script>

<svelte:window onkeydown={(event) => { if (drag && event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish(undefined,true) } }} />

<svg viewBox="0 0 100 100" preserveAspectRatio="none" aria-label="Контрольные точки Bézier" role="group" style="width:100%;height:120px;touch-action:none" onpointermove={move} onpointerup={(event) => finish(event)} onpointercancel={(event) => finish(event,true)}>
  <polyline points={points} fill="none" stroke="currentColor" stroke-width="1" />
  <line x1="10" y1={y(0)} x2={10+shown.p1[0]*80} y2={y(shown.p1[1])} stroke="currentColor" stroke-dasharray="2 2" />
  <line x1="90" y1={y(1)} x2={10+shown.p2[0]*80} y2={y(shown.p2[1])} stroke="currentColor" stroke-dasharray="2 2" />
  {#each ['p1','p2'] as point (point)}
    <circle role="button" tabindex={disabled ? -1 : 0} aria-disabled={disabled} aria-label={`Переместить Bézier ${point}`} cx={10+shown[point as 'p1' | 'p2'][0]*80} cy={y(shown[point as 'p1' | 'p2'][1])} r="4" fill="var(--panel-2)" stroke="currentColor" onpointerdown={(event) => begin(event,point as 'p1' | 'p2')} onlostpointercapture={() => finish(undefined,true)} onkeydown={(event) => keyboard(event,point as 'p1' | 'p2')} />
  {/each}
</svg>
