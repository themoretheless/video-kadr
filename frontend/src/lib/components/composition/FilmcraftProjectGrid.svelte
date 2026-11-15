<script lang="ts">
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { on } from 'svelte/events'
  import { editFilmcraftMarks, moveFilmcraftProjectCards, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  import FilmcraftPosterPreview from './FilmcraftPosterPreview.svelte'

  let { project, sources, token, sequenceId, clipId, onedit, selected = $bindable<string[]>([]), disabled = false }: {
    project: FilmcraftImportResult; sources: Record<string, string>; token: string | null; sequenceId: string; clipId: string
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean; selected?: string[]; disabled?: boolean
  } = $props()
  const pageSize = 12
  let open = $state(false)
  let freeform = $state(false)
  let snapCards = $state(false)
  let canvasWidth = $state(800)
  let moving = $state(false)
  let moveRequest: AbortController | undefined
  let suppressClick = false
  let drag = $state<{ pointer: number; startX: number; startY: number; x: number; y: number; items: string[]; document: string }>()
  let bin = $state('')
  let query = $state('')
  let page = $state(0)
  let hoverEnabled = $state(true)
  let hover = $state<{ item: string; fraction: number }>()
  let marking = $state(false)
  let markError = $state('')
  let markRequest: AbortController | undefined


  const bins = $derived(project.projectBins ?? [])
  const items = $derived(project.projectItems ?? [])
  const activeBin = $derived(bins.find(folder => folder.id === bin) ?? bins.find(folder => folder.parentId === null))
  const parent = $derived(bins.find(folder => folder.id === activeBin?.parentId))
  const search = $derived(query.trim().toLocaleLowerCase())
  const entries = $derived.by(() => {
    const byId = new Map(items.map(item => [item.id, item]))
    const memberIds = new SvelteSet<string>()
    const children = new SvelteMap<string, typeof bins>()
    for (const folder of bins) {
      if (folder.parentId === null) continue
      const siblings = children.get(folder.parentId) ?? []
      siblings.push(folder)
      children.set(folder.parentId, siblings)
    }
    if (activeBin) {
      const queue = [activeBin]
      const visited = new SvelteSet<string>()
      for (const folder of queue) {
        if (visited.has(folder.id)) continue
        visited.add(folder.id)
        for (const id of folder.itemIds ?? []) memberIds.add(id)
        if (search) for (const child of children.get(folder.id) ?? []) queue.push(child)
      }
    }
    const folders = search || !activeBin ? [] : children.get(activeBin.id) ?? []
    const available = activeBin && (search || activeBin.itemIds !== undefined)
      ? [...memberIds].flatMap(id => { const item = byId.get(id); return item ? [item] : [] })
      : items
    return [
      ...folders.map(folder => ({ kind: 'bin' as const, folder })),
      ...available.filter(item => !search || (item.name || item.id).toLocaleLowerCase().includes(search))
        .map(item => ({ kind: 'item' as const, item })),
    ]
  })
  const pages = $derived(Math.max(1, Math.ceil(entries.length / pageSize)))
  const currentPage = $derived(Math.min(page, pages - 1))
  const visible = $derived(freeform ? entries : entries.slice(currentPage * pageSize, (currentPage + 1) * pageSize))
  const layout = $derived.by(() => {
    if (!freeform) return {}
    const cards = entries.flatMap(entry => entry.kind === 'item' ? [entry.item] : [])
    const positions: Record<string, { x: number; y: number; size: number }> = {}
    let bottom = 0
    for (const item of cards) {
      const saved = item.freeformPosition
      if (!saved) continue
      const size = Math.min(1024, Math.max(32, saved.size ?? 128))
      positions[item.id] = { x: saved.x, y: saved.y, size }
      bottom = Math.max(bottom, saved.y + size * 9 / 16 + 40)
    }
    const columns = Math.max(1, Math.floor((canvasWidth - 12) / 152))
    const top = bottom > 0 ? Math.round((bottom + 8) / 20) * 20 : 12
    let index = 0
    for (const item of cards) {
      if (Object.hasOwn(positions, item.id)) continue
      positions[item.id] = { x: 12 + index % columns * 152, y: top + Math.floor(index / columns) * 116, size: 128 }
      index++
    }
    return positions
  })
  const canvasHeight = $derived(Object.values(layout).reduce((height, card) => Math.max(height, card.y + card.size * 9 / 16 + 40), 160))
  const canvasRight = $derived(Object.values(layout).reduce((right, card) => Math.max(right, card.x + card.size + 12), 0))

  function beginMove(id: string, event: PointerEvent) {
    suppressClick = false
    if (!freeform || !token || disabled || moving || marking || event.button !== 0) return
    const items = [id, ...(selected.includes(id) ? selected.filter(item => item !== id && Object.hasOwn(layout, item)) : [])]
    drag = { pointer: event.pointerId, startX: event.clientX, startY: event.clientY, x: 0, y: 0, items, document: project.documentText }
    ;(event.currentTarget as HTMLElement).setPointerCapture(event.pointerId)
  }

  function trackMove(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointer) return
    drag.x = event.clientX - drag.startX
    drag.y = event.clientY - drag.startY
  }

  async function finishMove(event: PointerEvent) {
    if (!drag || event.pointerId !== drag.pointer) return
    trackMove(event)
    const moved = drag
    drag = undefined
    if (Math.hypot(moved.x, moved.y) <= 1) return
    suppressClick = true
    if (!token || disabled || project.documentText !== moved.document) return
    const lead = layout[moved.items[0]!]
    if (!lead) return
    const dx = Math.max(0, lead.x + moved.x) - lead.x
    const dy = Math.max(0, lead.y + moved.y) - lead.y
    const positions = Object.fromEntries(moved.items.flatMap(id => {
      const card = layout[id]
      return card ? [[id, [card.x + dx, card.y + dy] as [number, number]]] : []
    }))
    const expected = { document: moved.document, sequenceId, clipId, token }
    const request = new AbortController()
    moveRequest = request
    moving = true
    markError = ''
    try {
      const result = await moveFilmcraftProjectCards(token, { document: expected.document, positions, snap: snapCards }, request.signal)
      if (!request.signal.aborted) onedit(result, expected)
    } catch (error) {
      if (!request.signal.aborted && project.documentText === expected.document && token === expected.token) {
        markError = error instanceof Error ? error.message : String(error)
      }
    } finally {
      if (moveRequest === request) { moveRequest = undefined; moving = false }
    }
  }

  function enter(id: string) {
    bin = id
    hover = undefined
    query = ''
    page = 0
  }
  function scrub(id: string, event: PointerEvent) {
    if (!hoverEnabled || event.pointerType === 'touch' || event.buttons) return
    const thumbnail = (event.currentTarget as HTMLElement).querySelector('figure')
    const rect = thumbnail?.getBoundingClientRect()
    if (!rect?.width || event.clientY < rect.top || event.clientY > rect.bottom) {
      hover = undefined
      return
    }
    const fraction = Math.max(0, Math.min(1, (event.clientX - rect.left) / rect.width))
    if (hover?.item !== id || hover.fraction !== fraction) hover = { item: id, fraction }
  }

  $effect(() => {
    if (!open) return
    const stop = on(window, 'keydown', markHovered, { capture: true })
    return () => { stop(); markRequest?.abort(); moveRequest?.abort() }
  })

  function markHovered(event: KeyboardEvent) {
    if (!hoverEnabled || !hover || !token || disabled || event.defaultPrevented || event.isComposing
      || event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return
    const target = event.target
    if (target instanceof HTMLElement && (target.isContentEditable || target.closest('input, textarea, select'))) return
    const key = event.key.toLowerCase()
    if (key !== 'i' && key !== 'o') return
    if (!visible.some(entry => entry.kind === 'item' && entry.item.id === hover?.item
      && (entry.item.kind === 'Media' || entry.item.kind === 'Sequence'))) return
    event.preventDefault()
    if (marking || moving || event.repeat) return
    void applyHoverMark(token, hover, key === 'i' ? 'in' : 'out')
  }

  async function applyHoverMark(owner: string, at: { item: string; fraction: number }, side: 'in' | 'out') {
    const expected = { document: project.documentText, sequenceId, clipId, token: owner }
    const request = new AbortController()
    markRequest = request
    marking = true
    markError = ''
    try {
      const result = await editFilmcraftMarks(owner, 'project.setMarks', {
        document: expected.document, item: at.item, hoverMark: { side, fraction: at.fraction },
      }, request.signal)
      if (!request.signal.aborted) onedit(result, expected)
    } catch (error) {
      if (!request.signal.aborted && project.documentText === expected.document && token === owner) {
        markError = error instanceof Error ? error.message : String(error)
      }
    } finally {
      if (markRequest === request) { markRequest = undefined; marking = false }
    }
  }

  function select(id: string, event: MouseEvent) {
    if (event.detail > 0 && suppressClick) { suppressClick = false; return }
    selected = event.metaKey || event.ctrlKey || event.shiftKey
      ? selected.includes(id) ? selected.filter(item => item !== id) : [...selected, id]
      : [id]
  }
</script>

<details ontoggle={event => { open = event.currentTarget.open; if (!open) hover = undefined }}>
  <summary>Сетка проекта Filmcraft</summary>
  {#if open}
    <div class="toolbar">
      {#if parent}<button type="button" disabled={disabled} onclick={() => enter(parent.id)}>На уровень выше</button>{/if}
      {#if activeBin}<span>{activeBin.name}</span>{/if}
      <label>Поиск <input type="search" aria-label="Поиск по папке Filmcraft" value={query} oninput={event => { query = event.currentTarget.value; page = 0; hover = undefined }} /></label>
      <label><input type="checkbox" bind:checked={hoverEnabled} onchange={() => { hover = undefined }} />Просмотр при наведении</label>
      <label><input type="checkbox" bind:checked={freeform} onchange={() => { hover = undefined; drag = undefined }} />Свободная раскладка</label>
      {#if freeform}<label><input type="checkbox" bind:checked={snapCards} />Привязка к сетке</label>{/if}
      {#if !freeform && pages > 1}
        <button type="button" aria-label="Предыдущая страница карточек" disabled={currentPage === 0} onclick={() => { page = currentPage - 1; hover = undefined }}>‹</button>
        <span>{currentPage + 1} / {pages}</span>
        <button type="button" aria-label="Следующая страница карточек" disabled={currentPage + 1 >= pages} onclick={() => { page = currentPage + 1; hover = undefined }}>›</button>
      {/if}
    </div>
    {#if markError}<p role="alert">{markError}</p>{/if}
    {#if visible.length}
      {#if freeform}
        <div class="toolbar">
          {#each visible as entry (entry.kind === 'bin' ? `bin-${entry.folder.id}` : `item-${entry.item.id}`)}
            {#if entry.kind === 'bin'}<button type="button" disabled={disabled} onclick={() => enter(entry.folder.id)}>Папка «{entry.folder.name}»</button>{/if}
          {/each}
        </div>
      {/if}
      <div class:scroll={freeform} bind:clientWidth={canvasWidth}>
      <ul class:freeform aria-label="Карточки проекта Filmcraft" style:min-height={freeform ? `${canvasHeight}px` : undefined} style:min-width={freeform ? `${canvasRight}px` : undefined}>
        {#each visible as entry (entry.kind === 'bin' ? `bin-${entry.folder.id}` : `item-${entry.item.id}`)}
          {#if entry.kind === 'bin'}
            {#if !freeform}<li><button type="button" class="folder" disabled={disabled} onclick={() => enter(entry.folder.id)}>Папка «{entry.folder.name}»</button></li>{/if}
          {:else}
            {@const position = layout[entry.item.id]}
            {@const dragged = drag?.items.includes(entry.item.id) ? drag : undefined}
            <li style:left={position ? `${Math.max(0, position.x + (dragged?.x ?? 0))}px` : undefined}
                style:top={position ? `${Math.max(0, position.y + (dragged?.y ?? 0))}px` : undefined}
                style:width={position ? `${position.size}px` : undefined}
                class:dragged={Boolean(dragged)} class:selected={selected.includes(entry.item.id)} onpointermove={event => scrub(entry.item.id, event)} onpointerleave={() => { hover = undefined }}>
              {#if entry.item.kind === 'Media' || entry.item.kind === 'Sequence'}
                <FilmcraftPosterPreview card lazy={freeform} document={project.documentText} item={entry.item.id} name={entry.item.name} {sources} {token} markRange={entry.item.cardMarkRange} hoverFraction={hoverEnabled && hover?.item === entry.item.id ? hover.fraction : undefined} />
              {/if}
              <button type="button" class="item" aria-pressed={selected.includes(entry.item.id)} disabled={disabled} onpointerdown={event => beginMove(entry.item.id, event)} onpointermove={trackMove} onpointerup={finishMove}
                onpointercancel={() => { drag = undefined }} onlostpointercapture={() => { drag = undefined }}
                onclick={event => select(entry.item.id, event)}>{entry.item.name || entry.item.id}</button>
            </li>
          {/if}
        {/each}
      </ul>
      </div>
    {:else}
      <p>{search ? 'Совпадений нет' : 'Папка пуста'}</p>
    {/if}
  {/if}
</details>

<style>
  details { flex-basis: 100%; min-width: 0; }
  summary { cursor: pointer; }
  .toolbar { display: flex; align-items: center; flex-wrap: wrap; gap: 0.5rem; margin-block: 0.5rem; }
  ul { display: grid; grid-template-columns: repeat(auto-fill, minmax(min(100%, 9rem), 1fr)); gap: 0.5rem; padding: 0; list-style: none; }
  li { position: relative; min-width: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 0.3rem; background: var(--panel); }
  .scroll { overflow: auto; max-height: 65vh; }
  ul.freeform { display: block; position: relative; margin: 0; }
  ul.freeform li { position: absolute; }
  ul.freeform button.item { touch-action: none; cursor: grab; }
  ul.freeform li.dragged { z-index: 1; }
  li.selected { border-color: var(--accent); }
  button.item, button.folder { width: 100%; overflow-wrap: anywhere; }
  button.item::after { content: ""; position: absolute; inset: 0; }
  button.folder { min-height: 3rem; }
</style>
