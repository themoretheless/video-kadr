<script lang="ts">
  import { SvelteMap, SvelteSet } from 'svelte/reactivity'
  import type { FilmcraftImportResult } from '$lib/api'
  import FilmcraftPosterPreview from './FilmcraftPosterPreview.svelte'

  let { project, sources, token, selected = $bindable<string[]>([]), disabled = false }: {
    project: FilmcraftImportResult; sources: Record<string, string>; token: string | null; selected?: string[]; disabled?: boolean
  } = $props()
  const pageSize = 12
  let open = $state(false)
  let bin = $state('')
  let query = $state('')
  let page = $state(0)

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
    query = ''
    page = 0
  }
  function select(id: string, event: MouseEvent) {
    selected = event.metaKey || event.ctrlKey || event.shiftKey
      ? selected.includes(id) ? selected.filter(item => item !== id) : [...selected, id]
      : [id]
  }
</script>

<details ontoggle={event => { open = event.currentTarget.open }}>
  <summary>Сетка проекта Filmcraft</summary>
  {#if open}
    <div class="toolbar">
      {#if parent}<button type="button" disabled={disabled} onclick={() => enter(parent.id)}>На уровень выше</button>{/if}
      {#if activeBin}<span>{activeBin.name}</span>{/if}
      <label>Поиск <input type="search" aria-label="Поиск по папке Filmcraft" value={query} oninput={event => { query = event.currentTarget.value; page = 0 }} /></label>
      {#if pages > 1}
        <button type="button" aria-label="Предыдущая страница карточек" disabled={currentPage === 0} onclick={() => { page = currentPage - 1 }}>‹</button>
        <span>{currentPage + 1} / {pages}</span>
        <button type="button" aria-label="Следующая страница карточек" disabled={currentPage + 1 >= pages} onclick={() => { page = currentPage + 1 }}>›</button>
      {/if}
    </div>
    {#if visible.length}
      <ul aria-label="Карточки проекта Filmcraft">
        {#each visible as entry (entry.kind === 'bin' ? `bin-${entry.folder.id}` : `item-${entry.item.id}`)}
          {#if entry.kind === 'bin'}
            <li><button type="button" class="folder" disabled={disabled} onclick={() => enter(entry.folder.id)}>Папка «{entry.folder.name}»</button></li>
          {:else}
            <li class:selected={selected.includes(entry.item.id)}>
              {#if entry.item.kind === 'Media' || entry.item.kind === 'Sequence'}
                <FilmcraftPosterPreview card document={project.documentText} item={entry.item.id} name={entry.item.name} {sources} {token} />
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
