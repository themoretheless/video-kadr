<script lang="ts">
  import type { VisualClip } from '$lib/composition/types'
  import { editCompositionMotionPath,insertMotionPathNodeNear } from '$lib/composition/motionPathCommands'
  import { compositionState,commitDocument } from '$lib/state/composition/core.svelte'
  let { clip,width,height }: { clip: VisualClip;width: number;height: number } = $props()
  const locked = $derived(Boolean(clip.locked || compositionState.document.tracks.some(track => track.locked && track.clips.some(candidate => candidate.id === clip.id))))
  let busy = $state(false)
  let controller: AbortController | undefined
  $effect(() => () => controller?.abort())
  type Part = 'point' | 'in_handle' | 'out_handle'
  const field = (part: Part) => part === 'point' ? 'point' : part === 'in_handle' ? 'inHandle' : 'outHandle'
  let draft = $state<{ index: number;part: Part;point: { x: number;y: number } } | null>(null)
  let drag: { element: SVGSVGElement;pointerId: number;index: number;part: Part;x: number;y: number;rect: DOMRect;captured: typeof compositionState.document;active: boolean } | null = null
  const authoredNodes = $derived(clip.position?.mode === 'path' ? clip.position.path.nodes : [])
  const nodes = $derived(authoredNodes.map((node,index) => draft?.index === index ? { ...node,[field(draft.part)]: draft.point } : node))
  const pathData = $derived(nodes.map((node,index) => {
    if (!index) return `M ${node.point.x} ${node.point.y}`
    const previous = nodes[index-1]!
    return previous.segment === 'line' ? `L ${node.point.x} ${node.point.y}` : `C ${previous.point.x+previous.outHandle.x} ${previous.point.y+previous.outHandle.y} ${node.point.x+node.inHandle.x} ${node.point.y+node.inHandle.y} ${node.point.x} ${node.point.y}`
  }).join(' '))
  function begin(event: PointerEvent,index: number,part: Part = 'point') {
    if (event.button !== 0 || busy || locked) return
    const svg = (event.currentTarget as SVGElement).ownerSVGElement!
    const rect = svg.getBoundingClientRect()
    if (!rect.width || !rect.height) return
    event.preventDefault();event.stopPropagation()
    svg.setPointerCapture?.(event.pointerId)
    drag = { element: svg,pointerId: event.pointerId,index,part,x: event.clientX,y: event.clientY,rect,captured: compositionState.document,active: false }
  }
  function move(event: PointerEvent) {
    if (!drag || drag.pointerId !== event.pointerId) return
    if (locked || compositionState.document !== drag.captured) { drag = null;draft = null;return }
    drag.active ||= Math.hypot(event.clientX-drag.x,event.clientY-drag.y) >= 3
    if (!drag.active) return
    const point = authoredNodes[drag.index]![field(drag.part)]
    draft = { index: drag.index,part: drag.part,point: { x: point.x+(event.clientX-drag.x)*width/drag.rect.width,y: point.y+(event.clientY-drag.y)*height/drag.rect.height } }
  }
  async function finish(event?: PointerEvent,cancel = false) {
    if (!drag || event && drag.pointerId !== event.pointerId) return
    if (event && !cancel) move(event)
    const gesture = drag
    const result = draft
    drag = null;draft = null
    if (gesture?.element.hasPointerCapture?.(gesture.pointerId)) gesture.element.releasePointerCapture(gesture.pointerId)
    if (!gesture || cancel || !result || compositionState.document !== gesture.captured || locked) return
    controller = new AbortController();busy = true
    try {
      const updated = await editCompositionMotionPath(gesture.captured,clip.id,{ operation: 'edit',index: result.index,part: result.part,value: result.point },controller.signal)
      if (compositionState.document !== gesture.captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(updated)
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    finally { busy = false }
  }
  async function moveNode(event: KeyboardEvent,index: number,part: Part = 'point') {
    if (busy || locked || !['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)) return
    event.preventDefault();event.stopPropagation()
    const node = nodes[index]![field(part)]
    const step = event.shiftKey ? 10 : 1
    const point = { x: node.x+(event.key === 'ArrowRight' ? step : event.key === 'ArrowLeft' ? -step : 0),y: node.y+(event.key === 'ArrowDown' ? step : event.key === 'ArrowUp' ? -step : 0) }
    const captured = compositionState.document
    controller = new AbortController()
    busy = true
    try {
      const updated = await editCompositionMotionPath(captured,clip.id,{ operation: 'edit',index,part,value: point },controller.signal)
      if (compositionState.document !== captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(updated)
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    finally { busy = false }
  }
  async function insert(event: MouseEvent) {
    if (busy || locked) return
    const rect = event.currentTarget instanceof SVGElement ? event.currentTarget.getBoundingClientRect() : null
    if (!rect?.width || !rect.height) return
    const captured = compositionState.document
    controller = new AbortController()
    busy = true
    try {
      const point = { x: (event.clientX-rect.left)*width/rect.width-width/2,y: (event.clientY-rect.top)*height/rect.height-height/2 }
      const updated = await insertMotionPathNodeNear(captured,clip.id,point,crypto.randomUUID(),8*Math.max(width/rect.width,height/rect.height),controller.signal)
      if (compositionState.document !== captured) throw new Error('Документ изменился во время редактирования пути')
      commitDocument(updated)
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
    finally { busy = false }
  }
</script>
<svelte:window onkeydown={event => { if (event.key === 'Escape' && drag) { event.preventDefault();void finish(undefined,true) } }} />
<svg onlostpointercapture={() => finish(undefined,true)} onpointermove={move} onpointerup={event => finish(event)} onpointercancel={event => finish(event,true)} aria-label="Траектория выбранного слоя" role="img" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" ondblclick={insert} style="position:absolute;inset:0;width:100%;height:100%;z-index:5;pointer-events:none;overflow:visible">
  <g transform={`translate(${width/2} ${height/2})`}>
    <path d={pathData} fill="none" stroke="var(--accent,#60a5fa)" stroke-width="2" vector-effect="non-scaling-stroke" style="pointer-events:stroke" />
    {#each nodes as node,index (node.id)}
      {#each ['in_handle','out_handle'] as part (part)}
        {@const handle = node[field(part as Part)]}
        <line x1={node.point.x} y1={node.point.y} x2={node.point.x+handle.x} y2={node.point.y+handle.y} stroke="var(--accent,#60a5fa)" opacity="0.5" vector-effect="non-scaling-stroke" />
        <circle role="button" tabindex={locked || busy ? -1 : 0} aria-disabled={locked || busy} aria-label={`Ручка траектории ${index+1} ${part}`} onpointerdown={event => begin(event,index,part as Part)} onkeydown={event => moveNode(event,index,part as Part)} style="pointer-events:auto" cx={node.point.x+handle.x} cy={node.point.y+handle.y} r="3" fill="white" stroke="var(--accent,#60a5fa)" vector-effect="non-scaling-stroke" />
      {/each}
    {/each}
    {#each nodes as node,index (node.id)}<circle role="button" tabindex={locked || busy ? -1 : 0} aria-disabled={locked || busy} aria-label={`Узел траектории ${index+1}`} onpointerdown={event => begin(event,index)} onkeydown={event => moveNode(event,index)} style="pointer-events:auto" cx={node.point.x} cy={node.point.y} r="4" fill="var(--accent,#60a5fa)" vector-effect="non-scaling-stroke" />{/each}
  </g>
</svg>
