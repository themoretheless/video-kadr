<script lang="ts">
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { on } from 'svelte/events'
  import { editFilmcraftLayout, type FilmcraftLayoutCommand, editFilmcraftArrangement, type FilmcraftArrangementCommand, editFilmcraftMarks, moveFilmcraftProjectCards, stackFilmcraftProjectCards, type FilmcraftIconSortKeys, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  import FilmcraftPosterPreview from './FilmcraftPosterPreview.svelte'

  let { project, sources, token, sequenceId, clipId, onedit, onopen, previewArea = $bindable(true), selected = $bindable<string[]>([]), disabled = false }: {
    project: FilmcraftImportResult; sources: Record<string, string>; token: string | null; sequenceId: string; clipId: string
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean; onopen: (item: string) => void; previewArea?: boolean; selected?: string[]; disabled?: boolean
  } = $props()
  const pageSize = 12
  let open = $state(false)
  let freeform = $state(false)
  let listView = $state(false)
  let listThumbnails = $state(false)
  const fontSizes = [[10.5, 'Мелкий'], [12, 'Обычный'], [14, 'Крупный'], [16, 'Очень крупный']] as const
  let fontSize = $state(12)
  let arrangementName = $state('')
  let arrangementChoice = $state('')
  let snapCards = $state(false)
  let canvasWidth = $state(800)
  let moving = $state(false)
  let moveRequest: AbortController | undefined
  let suppressClick = false
  let drag = $state<{ pointer: number; startX: number; startY: number; x: number; y: number; items: string[]; document: string }>()
  let bin = $state('')
  let query = $state('')
  const sortOptions = [
    ['user', 'Порядок в проекте'], ['name', 'Имя'], ['label', 'Метка'], ['mediaType', 'Тип медиа'],
    ['frameRate', 'Частота кадров'], ['mediaStart', 'Начало медиа'], ['mediaEnd', 'Конец медиа'],
    ['mediaDuration', 'Длительность'], ['videoInfo', 'Сведения о видео'], ['audioInfo', 'Сведения о звуке'],
    ['dateCreated', 'Дата создания'],
  ] as const satisfies readonly (readonly [keyof FilmcraftIconSortKeys | 'user' | 'name' | 'label', string])[]
  const viewStorageKey = 'video-kadr.filmcraft-project-view.v1'
  let iconSize = $state(110)
  let sortColumn = $state<(typeof sortOptions)[number][0]>('user')
  let descending = $state(false)
  let page = $state(0)
  let hoverEnabled = $state(true)
  let hover = $state<{ item: string; fraction: number }>()
  let marking = $state(false)
  let markError = $state('')
  let markRequest: AbortController | undefined


  $effect(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(viewStorageKey) ?? 'null')
      if (!saved || typeof saved !== 'object') return
      const column = sortOptions.find(([column]) => column === saved.column)
      if (column) sortColumn = column[0]
      descending = column?.[0] !== 'user' && Boolean(column) && saved.descending === true
      if (typeof saved.hover === 'boolean') hoverEnabled = saved.hover
      if (typeof saved.freeform === 'boolean') freeform = saved.freeform
      listView = !freeform && saved.list === true
      if (typeof saved.listThumbnails === 'boolean') listThumbnails = saved.listThumbnails
      if (typeof saved.previewArea === 'boolean') previewArea = saved.previewArea
      if (fontSizes.some(([size]) => size === saved.fontSize)) fontSize = saved.fontSize
      if (typeof saved.snap === 'boolean') snapCards = saved.snap
      if (typeof saved.iconSize === 'number' && Number.isFinite(saved.iconSize)) {
        iconSize = Math.max(48, Math.min(400, saved.iconSize))
      }
    } catch { /* Use defaults when browser storage is unavailable. */ }
  })

  function saveView() {
    try {
      localStorage.setItem(viewStorageKey, JSON.stringify({
        column: sortColumn, descending, hover: hoverEnabled, freeform, snap: snapCards, iconSize, list: listView, listThumbnails, previewArea, fontSize,
      }))
    } catch { /* Keep the current view usable without storage. */ }
  }

  const bins = $derived(project.projectBins ?? [])
  const items = $derived(project.projectItems ?? [])
  const activeBin = $derived(bins.find(folder => folder.id === bin) ?? bins.find(folder => folder.parentId === null))
  const arrangements = $derived.by(() => {
    const ids = activeBin?.itemIds && new Set(activeBin.itemIds)
    return [...new Set(items.filter(item => !ids || ids.has(item.id)).flatMap(item => item.freeformArrangements ?? []))].sort()
  })
  const currentArrangement = $derived(arrangements.includes(arrangementChoice) ? arrangementChoice : arrangements[0] ?? '')
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
    const matches = available.filter(item => !search || (item.name || item.id).toLocaleLowerCase().includes(search))
    if (!freeform && sortColumn !== 'user') {
      const column = sortColumn
      const key = (item: typeof items[number]) => column === 'name' ? item.name : column === 'label' ? item.label ?? '' : item.iconSort?.[column] ?? ''
      const compare = (left: string, right: string) => {
        const a = left.toLowerCase(), b = right.toLowerCase()
        return a < b ? -1 : a > b ? 1 : 0
      }
      matches.sort((left, right) => {
        const a = key(left), b = key(right)
        const order = typeof a === 'number' && typeof b === 'number' ? a - b : compare(String(a), String(b))
        return (order || compare(left.name, right.name)) * (descending ? -1 : 1)
      })
    }
    return [
      ...folders.map(folder => ({ kind: 'bin' as const, folder })),
      ...matches.map(item => ({ kind: 'item' as const, item })),
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
    const anchors = new Set(cards.filter(card => card.freeformStack === card.id).map(card => card.id))
    const depths: Record<string, number> = {}
    for (const item of cards) {
      const anchor = item.freeformStack
      if (!anchor || anchor === item.id || !anchors.has(anchor)) continue
      const leader = positions[anchor]
      const member = positions[item.id]
      if (!leader || !member) continue
      const depth = (depths[anchor] ?? 0) + 1
      depths[anchor] = depth
      member.x = leader.x + depth * 6
      member.y = leader.y + depth * 6
    }
    return positions
  })
  const canvasHeight = $derived(Object.values(layout).reduce((height, card) => Math.max(height, card.y + card.size * 9 / 16 + 40), 160))
  const canvasRight = $derived(Object.values(layout).reduce((right, card) => Math.max(right, card.x + card.size + 12), 0))

  const visibleItems = $derived(visible.flatMap(entry => entry.kind === 'item' ? [entry.item] : []))
  const stackSelection = $derived(selected.filter(id => Object.hasOwn(layout, id)))
  const canUnstack = $derived(visibleItems.some(item => item.freeformStack && stackSelection.includes(item.id)))
  function stackCount(id: string) { return visibleItems.filter(item => item.freeformStack === id).length }

  function beginMove(id: string, event: PointerEvent) {
    suppressClick = false
    if (!freeform || !token || disabled || moving || marking || event.button !== 0) return
    const stack = visibleItems.find(item => item.id === id)?.freeformStack
    const items = [...new Set([id, ...(selected.includes(id) ? stackSelection : []),
      ...visibleItems.filter(item => stack && item.freeformStack === stack).map(item => item.id)])]
    if (stack && Object.hasOwn(layout, stack)) items.splice(0, items.length, stack, ...items.filter(item => item !== stack))
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
    await editFreeform(token, (owner, document, signal) => moveFilmcraftProjectCards(owner, { document, positions, snap: snapCards }, signal))
  }

  async function editStack(stack: boolean) {
    if (!token || disabled || moving || marking || !stackSelection.length) return
    const items = [...stackSelection]
    await editFreeform(token, (owner, document, signal) => stackFilmcraftProjectCards(owner, stack, { document, items, width: canvasWidth }, signal))
  }

  function editLayout(command: FilmcraftLayoutCommand, step?: number) {
    if (!token || disabled || moving || marking) return
    const resize = command === 'project.freeform.resize'
    const targets = [...stackSelection]
    if (resize && !targets.length) return
    const bin = activeBin?.id
    const width = canvasWidth
    void editFreeform(token, (owner, document, signal) => editFilmcraftLayout(owner, command, {
      document, width, ...(resize ? { items: targets, step } : bin === undefined ? {} : { bin }),
    }, signal))
  }

  function editArrangement(command: FilmcraftArrangementCommand) {
    if (!token || disabled || moving || marking) return
    const save = command === 'project.freeform.saveArrangement'
    const name = save ? arrangementName.trim() : currentArrangement
    if (!name) return
    const bin = activeBin?.id
    const width = canvasWidth
    if (save) arrangementChoice = name
    void editFreeform(token, (owner, document, signal) => editFilmcraftArrangement(owner, command, {
      document, name, ...(bin === undefined ? {} : { bin }), ...(save ? { width } : {}),
    }, signal))
  }

  async function editFreeform(owner: string, run: (owner: string, document: string, signal: AbortSignal) => Promise<FilmcraftImportResult>) {
    const expected = { document: project.documentText, sequenceId, clipId, token: owner }
    const request = new AbortController()
    moveRequest = request
    moving = true
    markError = ''
    try {
      const result = await run(owner, expected.document, request.signal)
      if (!request.signal.aborted) onedit(result, expected)
    } catch (error) {
      if (!request.signal.aborted && project.documentText === expected.document && token === owner) {
        markError = error instanceof Error ? error.message : String(error)
      }
    } finally {
      if (moveRequest === request) { moveRequest = undefined; moving = false }
    }
  }

  function enter(id: string) {
    bin = id
    arrangementChoice = ''
    arrangementName = ''
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
      <label><input type="checkbox" checked={hoverEnabled} onchange={event => { hoverEnabled = event.currentTarget.checked; hover = undefined; saveView() }} />Просмотр при наведении</label>
      <label><input type="checkbox" checked={freeform} onchange={event => { freeform = event.currentTarget.checked; if (freeform) listView = false; hover = undefined; drag = undefined; saveView() }} />Свободная раскладка</label>
      <label><input type="checkbox" checked={listView} onchange={event => {
        listView = event.currentTarget.checked
        if (listView) freeform = false
        hover = undefined; drag = undefined; page = 0; saveView()
      }} />Список</label>
      {#if listView}
        <label><input type="checkbox" checked={listThumbnails} onchange={event => { listThumbnails = event.currentTarget.checked; hover = undefined; saveView() }} />Миниатюры в списке</label>
      {/if}
      <label><input type="checkbox" checked={previewArea} onchange={event => { previewArea = event.currentTarget.checked; saveView() }} />Предпросмотр выбранного элемента</label>
      <label>Размер текста
        <select value={fontSize} onchange={event => { fontSize = Number(event.currentTarget.value); saveView() }}>
          {#each fontSizes as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </label>
      {#if freeform}
        <label><input type="checkbox" checked={snapCards} onchange={event => { snapCards = event.currentTarget.checked; saveView() }} />Привязка к сетке</label>
        <button type="button" disabled={disabled || !token || moving || marking || !stackSelection.length} onclick={() => editLayout('project.freeform.resize', 1)}>Увеличить карточки</button>
        <button type="button" disabled={disabled || !token || moving || marking || !stackSelection.length} onclick={() => editLayout('project.freeform.resize', -1)}>Уменьшить карточки</button>
        <button type="button" disabled={disabled || !token || moving || marking} onclick={() => editLayout('project.freeform.alignToGrid')}>Выровнять по сетке</button>
        <button type="button" disabled={disabled || !token || moving || marking} onclick={() => editLayout('project.freeform.reset')}>Сбросить позиции и стеки</button>
        <button type="button" disabled={disabled || !token || moving || marking || stackSelection.length < 2} onclick={() => editStack(true)}>Сложить в стек</button>
        <button type="button" disabled={disabled || !token || moving || marking || !canUnstack} onclick={() => editStack(false)}>Разобрать стек</button>
      {/if}
      {#if !freeform && !listView}
        <label>Размер карточек
          <input type="range" min="48" max="400" step="1" value={iconSize}
            oninput={event => { iconSize = event.currentTarget.valueAsNumber }}
            onchange={saveView} />
        </label>
        <label>Сортировка карточек
          <select value={sortColumn} onchange={event => { sortColumn = event.currentTarget.value as typeof sortColumn; descending = false; page = 0; hover = undefined; saveView() }}>
            {#each sortOptions as [value, label] (value)}<option {value}>{label}</option>{/each}
          </select>
        </label>
        <label><input type="checkbox" checked={descending} disabled={sortColumn === 'user'} onchange={event => { descending = event.currentTarget.checked; page = 0; hover = undefined; saveView() }} />По убыванию</label>
      {/if}
      {#if !freeform && pages > 1}
        <button type="button" aria-label="Предыдущая страница карточек" disabled={currentPage === 0} onclick={() => { page = currentPage - 1; hover = undefined }}>‹</button>
        <span>{currentPage + 1} / {pages}</span>
        <button type="button" aria-label="Следующая страница карточек" disabled={currentPage + 1 >= pages} onclick={() => { page = currentPage + 1; hover = undefined }}>›</button>
      {/if}
    </div>
    {#if freeform}
      <div class="toolbar">
        <label>Имя раскладки <input bind:value={arrangementName} /></label>
        <button type="button" disabled={disabled || !token || moving || marking || !arrangementName.trim()}
          onclick={() => editArrangement('project.freeform.saveArrangement')}>Сохранить раскладку</button>
        {#if arrangements.length}
          <label>Сохранённые раскладки
            <select value={currentArrangement} onchange={event => { arrangementChoice = event.currentTarget.value }}>
              {#each arrangements as name (name)}<option value={name}>{name}</option>{/each}
            </select>
          </label>
          <button type="button" disabled={disabled || !token || moving || marking}
            onclick={() => editArrangement('project.freeform.restoreArrangement')}>Восстановить раскладку</button>
          <button type="button" disabled={disabled || !token || moving || marking}
            onclick={() => editArrangement('project.freeform.deleteArrangement')}>Удалить раскладку</button>
        {/if}
      </div>
    {/if}
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
      <ul class:freeform class:listView style:--filmcraft-font-size={`${fontSize}px`} style:--icon-size={`${iconSize}px`} aria-label="Карточки проекта Filmcraft" style:min-height={freeform ? `${canvasHeight}px` : undefined} style:min-width={freeform ? `${canvasRight}px` : undefined}>
        {#each visible as entry (entry.kind === 'bin' ? `bin-${entry.folder.id}` : `item-${entry.item.id}`)}
          {#if entry.kind === 'bin'}
            {#if !freeform}<li><button type="button" class="folder" disabled={disabled} onclick={() => enter(entry.folder.id)}>Папка «{entry.folder.name}»</button></li>{/if}
          {:else}
            {@const position = layout[entry.item.id]}
            {@const dragged = drag?.items.includes(entry.item.id) ? drag : undefined}
            <li style:left={position ? `${Math.max(0, position.x + (dragged?.x ?? 0))}px` : undefined}
                style:top={position ? `${Math.max(0, position.y + (dragged?.y ?? 0))}px` : undefined}
                style:width={position ? `${position.size}px` : undefined}
                class:stackAnchor={freeform && entry.item.freeformStack === entry.item.id} class:dragged={Boolean(dragged)} class:selected={selected.includes(entry.item.id)} onpointermove={event => scrub(entry.item.id, event)} onpointerleave={() => { hover = undefined }}>
              {#if (!listView || listThumbnails) && (entry.item.kind === 'Media' || entry.item.kind === 'Sequence')}
                <FilmcraftPosterPreview card lazy={freeform} document={project.documentText} item={entry.item.id} name={entry.item.name} {sources} {token} markRange={entry.item.cardMarkRange} hoverFraction={hoverEnabled && hover?.item === entry.item.id ? hover.fraction : undefined} />
              {/if}
              {#if freeform && entry.item.freeformStack === entry.item.id && stackCount(entry.item.id) > 1}
                <span class="stack-count">×{stackCount(entry.item.id)}</span>
              {/if}
              <button type="button" class="item" aria-pressed={selected.includes(entry.item.id)} disabled={disabled} onpointerdown={event => beginMove(entry.item.id, event)} onpointermove={trackMove} onpointerup={finishMove}
                onpointercancel={() => { drag = undefined }} onlostpointercapture={() => { drag = undefined }}
                onclick={event => select(entry.item.id, event)}
                ondblclick={() => onopen(entry.item.id)}
                onkeydown={event => {
                  if (event.key !== 'Enter') return
                  event.preventDefault()
                  if (event.repeat) return
                  selected = [entry.item.id]
                  onopen(entry.item.id)
                }}>{entry.item.name || entry.item.id}</button>
              {#if listView}
                <span class="media-type">{entry.item.iconSort?.mediaType || entry.item.kind}</span>
                {#if entry.item.iconSort?.mediaDuration !== undefined}
                  <span class="duration">{(entry.item.iconSort.mediaDuration / 254_016_000_000).toFixed(2)} с</span>
                {/if}
              {/if}
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
  ul { display: grid; grid-template-columns: repeat(auto-fill, min(100%, var(--icon-size))); gap: 0.5rem; padding: 0; list-style: none; }
  ul { font-size: var(--filmcraft-font-size); }
  ul :is(button, span) { font-size: inherit; }
  ul.listView { display: block; }
  ul.listView li { display: flex; align-items: center; min-height: 1.85em; gap: 0.5rem; padding-inline: 0.25rem; margin-block: 0.15rem; }
  ul.listView li :global(figure) { width: 64px; flex: 0 0 64px; }
  ul.listView button.item { width: auto; flex: 1 1 auto; text-align: start; }
  ul.listView .media-type { flex: 0 1 11em; overflow-wrap: anywhere; }
  ul.listView .duration { flex: 0 0 6em; text-align: end; }
  li { position: relative; min-width: 0; overflow: hidden; border: 1px solid var(--border); border-radius: 0.3rem; background: var(--panel); }
  .scroll { overflow: auto; max-height: 65vh; }
  ul.freeform { display: block; position: relative; margin: 0; }
  ul.freeform li { position: absolute; }
  ul.freeform button.item { touch-action: none; cursor: grab; }
  ul.freeform li.stackAnchor { z-index: 1; }
  ul.freeform li.dragged { z-index: 2; }
  .stack-count { position: absolute; top: 0.15rem; left: 0.2rem; padding-inline: 0.3rem; border-radius: 0.2rem; background: var(--panel); }
  li.selected { border-color: var(--accent); }
  button.item, button.folder { width: 100%; overflow-wrap: anywhere; }
  button.item::after { content: ""; position: absolute; inset: 0; }
  button.folder { min-height: 3rem; }
</style>
