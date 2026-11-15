<script lang="ts">
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import { on } from 'svelte/events'
  import { editFilmcraftMarks, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  import FilmcraftPosterPreview from './FilmcraftPosterPreview.svelte'

  let { project, sources, token, sequenceId, clipId, onedit, selected = $bindable<string[]>([]), disabled = false }: {
    project: FilmcraftImportResult; sources: Record<string, string>; token: string | null; sequenceId: string; clipId: string
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean; selected?: string[]; disabled?: boolean
  } = $props()
  const pageSize = 12
  let open = $state(false)
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
  const visible = $derived(entries.slice(currentPage * pageSize, (currentPage + 1) * pageSize))

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
    return () => { stop(); markRequest?.abort() }
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
    if (marking || event.repeat) return
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
      {#if pages > 1}
        <button type="button" aria-label="Предыдущая страница карточек" disabled={currentPage === 0} onclick={() => { page = currentPage - 1; hover = undefined }}>‹</button>
        <span>{currentPage + 1} / {pages}</span>
        <button type="button" aria-label="Следующая страница карточек" disabled={currentPage + 1 >= pages} onclick={() => { page = currentPage + 1; hover = undefined }}>›</button>
      {/if}
    </div>
    {#if markError}<p role="alert">{markError}</p>{/if}
    {#if visible.length}
      <ul aria-label="Карточки проекта Filmcraft">
        {#each visible as entry (entry.kind === 'bin' ? `bin-${entry.folder.id}` : `item-${entry.item.id}`)}
          {#if entry.kind === 'bin'}
            <li><button type="button" class="folder" disabled={disabled} onclick={() => enter(entry.folder.id)}>Папка «{entry.folder.name}»</button></li>
          {:else}
            <li class:selected={selected.includes(entry.item.id)} onpointermove={event => scrub(entry.item.id, event)} onpointerleave={() => { hover = undefined }}>
              {#if entry.item.kind === 'Media' || entry.item.kind === 'Sequence'}
                <FilmcraftPosterPreview card document={project.documentText} item={entry.item.id} name={entry.item.name} {sources} {token} markRange={entry.item.cardMarkRange} hoverFraction={hoverEnabled && hover?.item === entry.item.id ? hover.fraction : undefined} />
              {/if}
              <button type="button" class="item" aria-pressed={selected.includes(entry.item.id)} disabled={disabled} onclick={event => select(entry.item.id, event)}>{entry.item.name || entry.item.id}</button>
            </li>
          {/if}
        {/each}
      </ul>
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
  li.selected { border-color: var(--accent); }
  button.item, button.folder { width: 100%; overflow-wrap: anywhere; }
  button.item::after { content: ""; position: absolute; inset: 0; }
  button.folder { min-height: 3rem; }
</style>
