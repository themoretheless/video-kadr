<script lang="ts">
  import { untrack } from 'svelte'
  import { editFilmcraftMaskMonitor, getFilmcraftPreviewFrame, type FilmcraftImportResult, type FilmcraftMaskOverlay, type FilmcraftMaskFalloffHandle, type FilmcraftMaskMonitorOperation, type FilmcraftMaskMonitorPart, type FilmcraftMaskMonitorResult, type FilmcraftPreviewFrame } from '$lib/api'
  import { MonitorQueue } from '$lib/filmcraft/monitor-queue'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'
  let { document, address, createMask = false, sources, timeFrames, timeTicks, token, disabled = false, onbusy, onedit, ondeselect = () => {} }: { document: string; address: { sequenceId: string; clipId: string; effectIndex: number; effectKind: string; maskIndex?: number; parameter: 'path' }; createMask?: boolean; sources: Record<string, string>; timeFrames: string; timeTicks?: string; token?: string | null; disabled?: boolean; onbusy: (busy: boolean) => void; onedit: (project: FilmcraftImportResult, createdMaskIndex?: number) => void; ondeselect?: () => void } = $props()
  let open = $state(false), busy = $state(false), pending = $state(false)
  let tool = $state<'move' | 'pen' | 'rectangle' | 'ellipse' | 'insert'>('move')
  let overlay = $state<FilmcraftMaskOverlay | null>(null)
  let frame = $state<FilmcraftPreviewFrame | null>(null)
  let error = $state(''), previewError = $state('')
  let selected = $state<{ index: number; part: FilmcraftMaskMonitorPart } | null>(null)
  let svg = $state<SVGSVGElement>()
  let monitorWidth = $state(0), monitorHeight = $state(0)
  const identity = $derived(JSON.stringify([document, address, sources, timeFrames, timeTicks, token, disabled, createMask]))
  const clock = $derived(timeTicks ? { timeTicks } : { timeFrames })
  type Layout = { left: number; top: number; width: number; height: number }
  type Gesture = { pointer: number; owner: string; doc: string; start: [number, number]; layout: Layout; target: { index: number; part: FilmcraftMaskMonitorPart } | null; base: FilmcraftMaskOverlay; final: boolean; pen?: boolean; penIndex?: number; falloff?: FilmcraftMaskFalloffHandle['parameter'] }
  let gesture: Gesture | null = null
  type DraftPoint = { point: [number, number]; handle?: [number, number] }
  let penDraft = $state<{ owner: string; doc: string; base: FilmcraftMaskOverlay; points: DraftPoint[]; final: boolean } | null>(null)
  let epoch = 0
  let previousCreateMask: boolean | undefined
  let acceptedTarget: { doc: string; context: string; selection: typeof selected } | null = null
  const selectionContext = $derived(JSON.stringify([address.sequenceId, address.clipId, address.effectIndex, address.effectKind, address.maskIndex ?? null, sources, timeFrames, timeTicks, token, createMask]))
  type Input = { owner: string; epoch: number; doc: string; operation: FilmcraftMaskMonitorOperation; commit: boolean; layout?: Layout }
  const frames = new MonitorQueue<{ owner: string; doc: string }, FilmcraftPreviewFrame>(
    (request, signal) => getFilmcraftPreviewFrame({ document: request.doc, sequenceId: address.sequenceId, ...clock, sources, effects: true, captions: true }, signal, token),
    { reset: () => { previewError = '' }, result: value => { frame = value }, error: reason => { previewError = String(reason) }, pending: () => {} },
    (a, b) => a.owner === b.owner && a.doc === b.doc
  )
  const queue = new MonitorQueue<Input, { input: Input; result: FilmcraftMaskMonitorResult }>(
    async (input, signal) => ({ input, result: await editFilmcraftMaskMonitor({ document: input.doc, address, ...clock, operation: input.operation, ...(createMask ? { createMask: true } : {}) }, signal) }),
    { reset: () => {}, pending: value => { pending = value }, error: reason => { cancel(); error = String(reason) }, result: ({ input, result }) => {
      if (!open || input.owner !== identity || input.epoch !== epoch || disabled) return
      if (input.layout && !sameLayout(input.layout)) { cancel(); error = 'Размер монитора изменился'; return }
      overlay = result.overlay
      if (input.operation.kind !== 'penDraft' || input.commit) frames.submit(input.owner, { owner: input.owner, doc: result.project?.documentText ?? input.doc })
      if (input.commit && !result.project) {
        gesture = null; busy = false; onbusy(false)
        if (input.operation.kind === 'bodyClick' && result.overlay.deselectMask) { selected = null; acceptedTarget = null; open = false; ondeselect() }
        return
      }
      if (input.commit && result.project) {
        if (input.operation.kind === 'penDraft') { penDraft = null; tool = 'move' }
        if (result.overlay.selectedVertex !== undefined) selected = { index: result.overlay.selectedVertex, part: 'vertex' }
        else if (input.operation.kind === 'append' && result.overlay.vertices.length) selected = { index: result.overlay.vertices.length - 1, part: 'vertex' }
        else if (selected) selected = result.overlay.vertices.length ? { ...selected, index: Math.min(selected.index, result.overlay.vertices.length - 1) } : null
        const created = createMask ? result.overlay.createdMaskIndex : undefined
        const nextContext = created === undefined ? selectionContext : JSON.stringify([address.sequenceId, address.clipId, address.effectIndex, address.effectKind, created, sources, timeFrames, timeTicks, token, false])
        acceptedTarget = { doc: result.project.documentText, context: nextContext, selection: selected }; gesture = null; busy = false; onbusy(false)
        if (created !== undefined) { tool = 'move'; onedit(result.project, created) } else onedit(result.project)
      }
    } },
    (a, b) => a.epoch === b.epoch && a.commit === b.commit && JSON.stringify(a.operation) === JSON.stringify(b.operation)
  )
  function layout(): Layout | null { const box = svg?.getBoundingClientRect(); return box && box.width > 0 && box.height > 0 ? { left: box.left, top: box.top, width: box.width, height: box.height } : null }
  function sameLayout(previous: Layout) { const box = layout(); return box !== null && (['left', 'top', 'width', 'height'] as const).every(key => box[key] === previous[key]) }
  function scene(event: PointerEvent, box: Layout): [number, number] { return [(event.clientX - box.left) * overlay!.canvas[0] / box.width, (event.clientY - box.top) * overlay!.canvas[1] / box.height] }
  function release() { if (gesture) gesture.final = true; if (gesture && svg?.hasPointerCapture?.(gesture.pointer)) svg.releasePointerCapture(gesture.pointer) }
  function cancel() { epoch++; release(); if (penDraft) { overlay = penDraft.base; penDraft = null; tool = createMask ? 'pen' : 'move' } else if (gesture) overlay = gesture.base; gesture = null; busy = false; queue.clear(); frames.clear(); onbusy(false); if (open) frames.submit(identity, { owner: identity, doc: document }) }
  function command(operation: FilmcraftMaskMonitorOperation) {
    if (disabled || busy || penDraft || !overlay?.editable) return
    epoch++; busy = true; onbusy(true); error = ''
    queue.submit(identity, { owner: identity, epoch, doc: document, operation, commit: true })
  }
  function start(event: PointerEvent, target: Gesture['target'] = null, falloff?: FilmcraftMaskFalloffHandle['parameter']) {
    if (event.button !== 0 || disabled || busy || !overlay?.editable || (falloff && tool !== 'move')) return
    const box = layout(); if (!box) return
    event.preventDefault(); event.stopPropagation(); svg?.focus()
    if (target) selected = target
    if (!falloff && (tool === 'insert' || (tool === 'move' && (event.metaKey || event.ctrlKey) && !target))) {
      command({ kind: 'insertNear', point: scene(event, box), pixelScale: [box.width / overlay.canvas[0], box.height / overlay.canvas[1]] }); return
    }
    if (tool === 'move' && (event.metaKey || event.ctrlKey) && target?.part === 'vertex') { command({ kind: 'delete', index: target.index }); return }
    if (tool === 'pen') {
      if (penDraft?.final) return
      if (penDraft && penDraft.points.length >= 3 && target?.index === 0 && target.part === 'vertex') { finishPen(); return }
      if (!penDraft) penDraft = { owner: identity, doc: document, base: overlay, points: [], final: false }
      if (penDraft.points.length >= 4096) { error = 'Контур пера содержит не более 4096 точек'; return }
      epoch++; busy = true; onbusy(true); error = ''
      const point = scene(event, box)
      penDraft.points = [...penDraft.points, { point }]
      gesture = { pointer: event.pointerId, owner: identity, doc: penDraft.doc, start: point, layout: box, target: null, base: penDraft.base, final: false, pen: true, penIndex: penDraft.points.length - 1 }
      svg?.setPointerCapture?.(event.pointerId); previewPen(); return
    }
    epoch++; busy = true; onbusy(true); error = ''
    gesture = { pointer: event.pointerId, owner: identity, doc: document, start: scene(event, box), layout: box, target, base: overlay, final: false, falloff }
    svg?.setPointerCapture?.(event.pointerId)
  }
  function penOperation(closed = false): FilmcraftMaskMonitorOperation {
    return { kind: 'penDraft', closed, points: penDraft!.points.map(p => ({ point: [...p.point] as [number, number], ...(p.handle ? { handle: [...p.handle] as [number, number] } : {}) })) }
  }
  function previewPen() {
    if (!penDraft || penDraft.final) return
    queue.submit(penDraft.owner, { owner: penDraft.owner, epoch, doc: penDraft.doc, operation: penOperation(), commit: false, ...(gesture ? { layout: gesture.layout } : {}) })
  }
  function finishPen() {
    if (!penDraft || penDraft.final || penDraft.points.length < 3 || disabled || penDraft.owner !== identity) return
    if (gesture) { release(); gesture = null }
    penDraft.final = true; busy = true; onbusy(true)
    queue.submit(penDraft.owner, { owner: penDraft.owner, epoch, doc: penDraft.doc, operation: penOperation(true), commit: true })
  }
  function setTool(next: typeof tool) { if (penDraft) cancel(); tool = next }
  function dragOperation(event: PointerEvent): FilmcraftMaskMonitorOperation {
    const g = gesture!, point = scene(event, g.layout)
    if (g.falloff) return { kind: 'falloff', parameter: g.falloff, delta: [point[0] - g.start[0], point[1] - g.start[1]] }
    if (g.target) { const v = g.base.vertices[g.target.index]!, initial = g.target.part === 'vertex' ? v.point : g.target.part === 'in' ? v.incoming : v.outgoing; return { kind: 'move', ...g.target, point: [initial[0] + point[0] - g.start[0], initial[1] + point[1] - g.start[1]], breakHandles: event.altKey } }
    if (tool === 'rectangle' || tool === 'ellipse') return { kind: 'box', shape: tool, from: g.start, to: point }
    return { kind: 'translate', origin: g.start, delta: [point[0] - g.start[0], point[1] - g.start[1]] }
  }
  function move(event: PointerEvent) {
    if (!gesture || gesture.final || event.pointerId !== gesture.pointer || !sameLayout(gesture.layout)) return
    const point = scene(event, gesture.layout)
    if (gesture.pen && penDraft) {
      const index = gesture.penIndex!
      penDraft.points = penDraft.points.map((p, i) => i === index ? { ...p, handle: point } : p)
      previewPen(); return
    }
    if (point[0] === gesture.start[0] && point[1] === gesture.start[1]) return
    queue.submit(gesture.owner, { owner: gesture.owner, epoch, doc: gesture.doc, operation: dragOperation(event), commit: false, layout: gesture.layout })
  }
  function finish(event: PointerEvent) {
    if (!gesture || gesture.final || event.pointerId !== gesture.pointer) return
    const g = gesture
    if (!sameLayout(g.layout) || g.owner !== identity) { cancel(); return }
    const point = scene(event, g.layout)
    if (g.pen && penDraft) {
      const index = g.penIndex!
      penDraft.points = penDraft.points.map((p, i) => i === index ? { point: p.point, ...(point[0] !== g.start[0] || point[1] !== g.start[1] ? { handle: point } : {}) } : p)
      release(); gesture = null; busy = false; previewPen(); return
    }
    if (point[0] === g.start[0] && point[1] === g.start[1]) {
      if (tool === 'move' && !g.target && !g.falloff) {
        g.final = true; release()
        queue.submit(g.owner, { owner: g.owner, epoch, doc: g.doc, operation: { kind: 'bodyClick', point, pixelScale: [g.layout.width / g.base.canvas[0], g.layout.height / g.base.canvas[1]] }, commit: true, layout: g.layout })
        return
      }
      const smooth = g.target?.part === 'vertex' && event.altKey ? g.target.index : undefined
      cancel(); if (smooth !== undefined) command({ kind: 'smooth', index: smooth }); return
    }
    g.final = true; release()
    queue.submit(g.owner, { owner: g.owner, epoch, doc: g.doc, operation: dragOperation(event), commit: true, layout: g.layout })
  }
  function key(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); cancel(); return }
    if (tool === 'pen') { if (event.key === 'Enter') { event.preventDefault(); finishPen() } return }
    if (!selected || !overlay?.editable || disabled || busy) return
    if (event.key === 'Delete' || event.key === 'Backspace') { event.preventDefault(); command({ kind: 'delete', index: selected.index }); return }
    if (event.key === 'Enter') { event.preventDefault(); command({ kind: 'smooth', index: selected.index }); return }
    const delta: Record<string, [number, number]> = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, -1], ArrowDown: [0, 1] }
    const step = delta[event.key], vertex = overlay.vertices[selected.index]
    if (step && vertex) { event.preventDefault(); const point = selected.part === 'vertex' ? vertex.point : selected.part === 'in' ? vertex.incoming : vertex.outgoing; const size = event.shiftKey ? 10 : 1; command({ kind: 'move', ...selected, point: [point[0] + step[0] * size, point[1] + step[1] * size], breakHandles: event.altKey }) }
  }
  // Only the original 14 CSS pixel stem is viewport presentation. Native
  // points, normals, contours and drag values come from Rust.
  function falloffPoint(handle: FilmcraftMaskFalloffHandle): [number, number] {
    if (!overlay || !monitorWidth || !monitorHeight) return handle.point
    const sx = monitorWidth / overlay.canvas[0], sy = monitorHeight / overlay.canvas[1]
    const dx = handle.direction[0] * sx, dy = handle.direction[1] * sy
    const length = Math.hypot(dx, dy)
    return [handle.point[0] + (length > 1e-6 ? dx / length : 0) * 14 / sx, handle.point[1] + (length > 1e-6 ? dy / length : -1) * 14 / sy]
  }
  function contour(points: Array<[number, number]>) { return points.length ? `M ${points.map(p => `${p[0]} ${p[1]}`).join(' L ')} Z` : '' }
  $effect(() => {
    const owner = identity, enabled = open
    untrack(() => { epoch++; queue.clear(); frames.clear(); release(); gesture = null; if (penDraft) { penDraft = null; tool = 'move' } if (createMask) tool = 'pen'; else if (previousCreateMask) tool = 'move'; previousCreateMask = createMask; busy = false; onbusy(false); overlay = null; frame = null; error = ''; previewError = ''; selected = acceptedTarget?.doc === document && acceptedTarget.context === selectionContext ? acceptedTarget.selection : null; if (!disabled && acceptedTarget?.doc === document) acceptedTarget = null; if (enabled && !disabled) queue.submit(owner, { owner, epoch, doc: document, operation: { kind: 'read' }, commit: false }) })
    return () => untrack(() => { epoch++; queue.clear(); frames.clear(); release(); gesture = null; busy = false; onbusy(false) })
  })
</script>
<details bind:open>
  <summary>{createMask ? 'Создать маску в мониторе Filmcraft' : 'Маска в мониторе Filmcraft'}</summary>
  {#if open}
    <label>Инструмент <select aria-label="Инструмент маски в мониторе Filmcraft" value={tool} disabled={busy} onchange={event => setTool(event.currentTarget.value as typeof tool)}><option value="move" disabled={createMask}>Перемещение</option><option value="pen">Перо</option><option value="rectangle">Прямоугольный контур</option><option value="ellipse">Эллиптический контур</option><option value="insert" disabled={createMask}>Вставить вершину на ребре</option></select></label>
    <button disabled={disabled || createMask || busy || !!penDraft || !overlay?.editable || !selected} onclick={() => selected && command({ kind: 'smooth', index: selected.index })}>Сгладить/угол маски Filmcraft</button>
    <button disabled={disabled || (createMask && !penDraft) || busy || !overlay?.editable || (penDraft ? penDraft.points.length < 3 : (overlay?.vertices.length ?? 0) < 3)} onclick={() => penDraft ? finishPen() : overlay && command({ kind: 'close', closed: !overlay.closed })}>{tool === 'pen' ? 'Завершить контур пера Filmcraft' : overlay?.closed ? 'Разомкнуть контур Filmcraft' : 'Замкнуть контур Filmcraft'}</button>
    {#if overlay}
      {#if !overlay.editable}<p>Маска заблокирована, клип не активен или преобразование не обратимо.</p>{/if}
      <div class="monitor" bind:clientWidth={monitorWidth} bind:clientHeight={monitorHeight} style:aspect-ratio={`${overlay.canvas[0]} / ${overlay.canvas[1]}`}>
        {#if frame}<FilmcraftMonitorFrame {frame} label="Кадр редактирования маски Filmcraft" fluid metadata={false} />{/if}
        <!-- Interactive drawing surface: keyboard commands and named focusable vertex handles. -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <svg bind:this={svg} viewBox={`0 0 ${overlay.canvas[0]} ${overlay.canvas[1]}`} aria-label="Контур маски Filmcraft" role="application" tabindex="0" onpointerdown={event => start(event)} onpointermove={move} onpointerup={finish} onpointercancel={cancel} onlostpointercapture={() => { if (gesture && !gesture.final) cancel() }} onkeydown={key}>
          <path d={overlay.svgPath} fill="rgba(0,200,255,0.06)" stroke="#00c8ff" stroke-width="2" vector-effect="non-scaling-stroke" />
          {#if overlay.falloff && tool !== 'pen' && !createMask}
            <path d={contour(overlay.falloff.expansionContour)} fill="none" stroke="#00c8ff" stroke-opacity="0.8" stroke-dasharray="4 3" vector-effect="non-scaling-stroke" />
            <path d={contour(overlay.falloff.featherContour)} fill="none" stroke="#00c8ff" stroke-opacity="0.6" stroke-dasharray="4 3" vector-effect="non-scaling-stroke" />
            {#each overlay.falloff.handles as handle (handle.parameter)}
              {@const hp = falloffPoint(handle)}
              {@const radius = 4 * overlay.canvas[0] / (monitorWidth || overlay.canvas[0])}
              <line x1={handle.point[0]} y1={handle.point[1]} x2={hp[0]} y2={hp[1]} stroke="#00c8ff" vector-effect="non-scaling-stroke" />
              {#if handle.parameter === 'feather'}
                <circle cx={hp[0]} cy={hp[1]} r={radius} fill="#00c8ff" stroke="#17202a" vector-effect="non-scaling-stroke" role="button" tabindex="0" aria-label="Растушёвка маски Filmcraft" onfocus={() => selected = null} onpointerdown={event => start(event, null, handle.parameter)} />
              {:else}
                <polygon points={`${hp[0]},${hp[1] - radius} ${hp[0] + radius},${hp[1]} ${hp[0]},${hp[1] + radius} ${hp[0] - radius},${hp[1]}`} fill="#00c8ff" stroke="#17202a" vector-effect="non-scaling-stroke" role="button" tabindex="0" aria-label="Расширение маски Filmcraft" onfocus={() => selected = null} onpointerdown={event => start(event, null, handle.parameter)} />
              {/if}
            {/each}
          {/if}
          {#each overlay.vertices as vertex (vertex.index)}
            <line x1={vertex.point[0]} y1={vertex.point[1]} x2={vertex.incoming[0]} y2={vertex.incoming[1]} stroke="#fff" vector-effect="non-scaling-stroke" />
            <line x1={vertex.point[0]} y1={vertex.point[1]} x2={vertex.outgoing[0]} y2={vertex.outgoing[1]} stroke="#fff" vector-effect="non-scaling-stroke" />
            {#each ['in', 'out', 'vertex'] as item (item)}
              {@const part = item as FilmcraftMaskMonitorPart}
              {@const point = part === 'in' ? vertex.incoming : part === 'out' ? vertex.outgoing : vertex.point}
              <circle cx={point[0]} cy={point[1]} r={part === 'vertex' ? '0.9%' : '0.65%'} fill={part === 'vertex' ? '#00c8ff' : '#fff'} stroke="#17202a" vector-effect="non-scaling-stroke" role="button" tabindex="0" aria-label={`Маска ${vertex.index + 1} ${part} Filmcraft`} onfocus={() => selected = { index: vertex.index, part }} onpointerdown={event => start(event, { index: vertex.index, part })} />
            {/each}
          {/each}
        </svg>
      </div>
      <small>Перетаскивание: точка, ручка или весь контур. Alt разделяет ручки. Стрелки сдвигают выбранную точку, Enter меняет сглаживание, Delete удаляет, Escape отменяет. Ctrl/Cmd-клик по ребру вставляет вершину, по вершине — удаляет.</small>
    {/if}
    {#if tool === 'pen'}<small role="status">{penDraft ? `Черновик пера · ${penDraft.points.length} точек.` : 'Поставьте первую точку контура.'} Протяжка создаёт ручки, Enter или первая точка завершает, Escape отменяет.</small>{/if}
    {#if pending}<small role="status">Подготовка контура…</small>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
    {#if previewError}<p role="alert">{previewError}</p>{/if}
  {/if}
</details>
<style>
  .monitor { position: relative; width: min(100%, 720px); background: #20242a; overflow: visible; }
  .monitor :global(canvas) { position: absolute; inset: 0; width: 100%; height: 100%; }
  svg { position: absolute; inset: 0; width: 100%; height: 100%; touch-action: none; cursor: crosshair; overflow: visible; clip-path: inset(-16px); }
  circle { cursor: grab; }
  circle:focus { stroke: #ffca28; stroke-width: 3; outline: none; }
</style>
