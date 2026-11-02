<script lang="ts">
  import { keyframeTangentPoints, keyframeTangentAtPoint, type KeyframeValueRange } from '$lib/composition/keyframeCurve'
  import { editKeyframeTangent, resolvedKeyframeTangents } from '$lib/composition/tangents'
  import type { KeyframeChannel } from '$lib/composition/keyframeChannels'
  import { compositionState, linkedScaleAutomationPatches, editCompositionAutomationTangent } from '$lib/state/composition.svelte'
  let { channel, zoom, range, onaxis, onmenu }: { channel: KeyframeChannel; zoom: number; range: KeyframeValueRange; onaxis: (id: string, range: KeyframeValueRange | null) => void; onmenu: (event: MouseEvent | KeyboardEvent, channel: KeyframeChannel, tick: number) => void } = $props()
  let gesture: { id: number; element: SVGElement; channel: KeyframeChannel; zoom: number; range: KeyframeValueRange; rect: DOMRect; tick: number; side: 'incoming' | 'outgoing'; document: typeof compositionState.document; x: number; y: number; active: boolean; next: { x: number; y: number } | null } | null = null
  let owned: typeof compositionState.ui.keyframePreviews = []
  function clear(): void {
    compositionState.ui.keyframePreviews = compositionState.ui.keyframePreviews.filter(entry => !owned.some(item => item.value === entry.value))
    owned = []
  }
  function begin(event: PointerEvent, tick: number, side: 'incoming' | 'outgoing'): void {
    if (event.button !== 0 || channel.locked) return
    event.preventDefault(); event.stopPropagation()
    const element = event.currentTarget as SVGElement
    const rect = element.ownerSVGElement!.getBoundingClientRect()
    if (!rect.width || !rect.height) return
    gesture = { id: event.pointerId, element, channel, zoom, range: { ...range }, rect, tick, side, document: compositionState.document, x: event.clientX, y: event.clientY, active: false, next: null }
    onaxis(channel.id, range)
    element.setPointerCapture?.(event.pointerId)
  }
  function move(event: PointerEvent): void {
    const g = gesture
    if (!g || event.pointerId !== g.id) return
    g.active ||= Math.hypot(event.clientX-g.x,event.clientY-g.y) >= 3
    if (!g.active) return
    const point = keyframeTangentAtPoint(g.channel,g.tick,g.side,{ x: (event.clientX-g.rect.left)*Number(g.element.ownerSVGElement!.getAttribute('width'))/g.rect.width, y: (event.clientY-g.rect.top)*120/g.rect.height },g.zoom,120,g.range)
    if (!point) return
    try {
      const value = editKeyframeTangent(g.channel.value,g.tick,g.side,point.x,point.y)
      const patches = linkedScaleAutomationPatches([{ target: g.channel.target, property: g.channel.property, value }])
      clear()
      const ids = patches.map(p => JSON.stringify([p.target,p.property]))
      compositionState.ui.keyframePreviews = [...compositionState.ui.keyframePreviews.filter(p => !ids.includes(JSON.stringify([p.target,p.property]))), ...patches.map(p => ({ target: p.target, property: p.property, value: p.value!, ticks: [] }))]
      owned = compositionState.ui.keyframePreviews.filter(p => ids.includes(JSON.stringify([p.target,p.property])))
      g.next = point
    } catch { g.next = null; clear() }
  }
  function finish(event?: PointerEvent, cancel = false): void {
    const g = gesture
    if (!g || event && event.pointerId !== g.id) return
    if (event && !cancel) move(event)
    gesture = null; clear(); onaxis(g.channel.id,null)
    if (g.element.hasPointerCapture?.(g.id)) g.element.releasePointerCapture(g.id)
    if (!cancel && g.next && compositionState.document === g.document) {
      try { editCompositionAutomationTangent(g.channel.target,g.channel.property,g.tick,g.side,g.next.x,g.next.y,true) } catch { /* Authoring admission rejected the draft. */ }
    }
  }
  function keyboard(event: KeyboardEvent, tick: number, side: 'incoming' | 'outgoing'): void {
    if (event.key === 'ContextMenu' || event.shiftKey && event.key === 'F10') { onmenu(event,channel,tick); return }
    if (!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return
    event.preventDefault(); event.stopPropagation()
    if (channel.locked) return
    const index = channel.value.track.keyframes.findIndex(key => key.tick === tick)
    const tangent = resolvedKeyframeTangents(channel.value.track.keyframes,index,channel.value.track.interpolation)[side]
    const step = event.shiftKey ? 0.1 : 0.01
    const x = Math.max(0,Math.min(1,tangent.x+(event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0)))
    const y = tangent.y+(event.key === 'ArrowUp' ? step : event.key === 'ArrowDown' ? -step : 0)
    try { editCompositionAutomationTangent(channel.target,channel.property,tick,side,x,y,true) } catch { /* Admission retains authored state. */ }
  }
  $effect(() => {
    const document = compositionState.document
    if (channel.locked || gesture && gesture.document !== document) finish(undefined,true)
  })
  $effect(() => () => finish(undefined,true))
</script>
<svelte:window onpointermove={move} onpointerup={(event) => finish(event)} onpointercancel={(event) => finish(event,true)} onkeydown={(event) => { if (gesture && event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); finish(undefined,true) } }} />
{#each keyframeTangentPoints(channel,zoom,120,range) as handle (`${handle.tick}:${handle.side}`)}
  <line x1={handle.keyX} y1={handle.keyY} x2={handle.x} y2={handle.y} stroke="currentColor" stroke-dasharray="2 2" style="pointer-events:none;opacity:.4" />
  <circle aria-label={`Касательная ${channel.property} ${handle.side} ${handle.tick}`} role="button" tabindex={channel.locked ? -1 : 0} aria-disabled={channel.locked} cx={handle.x} cy={handle.y} r="4" fill="var(--panel-2)" stroke="currentColor" style="pointer-events:all;cursor:move" oncontextmenu={(event) => onmenu(event,channel,handle.tick)} onkeydown={(event) => keyboard(event,handle.tick,handle.side)} onpointerdown={(event) => begin(event,handle.tick,handle.side)} onlostpointercapture={() => finish(undefined,true)} />
{/each}
