<script lang="ts">
  import {
    deleteFromLibrary,
    openFromLibrary,
    state as appState,
    updateLibraryMetadata,
  } from '$lib/state/store.svelte.js'
  import {
    addLibraryEntryToComposition,
    compositionState,
    inferMediaType,
    setEditorMode,
  } from '$lib/state/composition.svelte.js'
  import type { MediaEntry } from '$lib/types.js'
  import { libraryFilmstripUrl, libraryThumbnailUrl } from '$lib/api.js'
  import { proxyController } from '$lib/proxy/state.svelte.js'
  import {
    filterLibraryEntries,
    MAX_LIBRARY_TAG_CHARS,
    MAX_LIBRARY_TAGS,
    MAX_LIBRARY_TITLE_CHARS,
    parseLibraryTags,
    validateLibraryTitle,
  } from './mediaLibraryModel.js'

  let query = $state('')
  let favoritesOnly = $state(false)
  let editingId = $state<string | null>(null)
  let proxyId = $state<string | null>(null)
  let editTitle = $state('')
  let editTags = $state('')
  let busyId = $state<string | null>(null)
  let metadataError = $state('')
  let thumbnailState = $state<Record<string, 'loading' | 'ready' | 'error'>>({})
  let filmstripState = $state<Record<string, 'loading' | 'ready' | 'error'>>({})
  let filmstripIndex = $state<Record<string, number>>({})
  let filmstripMode = $state<Record<string, 'pointer' | 'keyboard'>>({})
  type ProxyControlsComponentType = typeof import('$lib/proxy/ProxyControls.svelte').default
  let ProxyControlsComponent = $state<ProxyControlsComponentType | null>(null)
  let filteredLibrary = $derived(filterLibraryEntries(appState.library, { query, favoritesOnly }))

  const label = (entry: MediaEntry) => entry.title?.trim() || entry.filename
  function fmtDuration(time?: number | null): string {
    if (!time || !Number.isFinite(time)) return ''
    return `${Math.floor(time / 60)}:${Math.floor(time % 60).toString().padStart(2, '0')}`
  }
  function fmtSize(bytes?: number | null): string {
    if (!bytes) return ''
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} ГБ`
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} МБ`
    return `${Math.max(1, Math.round(bytes / 1024))} КБ`
  }
  function meta(entry: MediaEntry): string {
    return [entry.width && entry.height ? `${entry.width}×${entry.height}` : '', fmtDuration(entry.duration), fmtSize(entry.sizeBytes)]
      .filter(Boolean).join(' · ')
  }
  const ext = (entry: MediaEntry) => entry.filename.split('.').pop()?.toUpperCase() || ''
  const FILMSTRIP_CELLS = 8

  function thumbnailUrl(id: string): string | null {
    try {
      return libraryThumbnailUrl(id)
    } catch {
      return null
    }
  }

  function thumbnailStatus(id: string): 'loading' | 'ready' | 'error' {
    return thumbnailState[id] ?? 'loading'
  }

  function filmstripUrl(entry: MediaEntry): string | null {
    if (
      inferMediaType(entry) !== 'video'
      || !entry.duration
      || !Number.isFinite(entry.duration)
      || entry.duration <= 0
    ) return null
    try {
      return libraryFilmstripUrl(entry.id)
    } catch {
      return null
    }
  }

  function currentFilmstripIndex(id: string): number {
    return filmstripIndex[id] ?? 0
  }

  function activateFilmstrip(entry: MediaEntry, mode: 'pointer' | 'keyboard'): void {
    if (!filmstripUrl(entry)) return
    filmstripMode[entry.id] = mode
    if (!filmstripState[entry.id]) filmstripState[entry.id] = 'loading'
  }

  function deactivateFilmstrip(id: string, mode: 'pointer' | 'keyboard'): void {
    if (filmstripMode[id] === mode) delete filmstripMode[id]
  }

  function scrubFilmstripPointer(entry: MediaEntry, event: PointerEvent): void {
    if (!filmstripUrl(entry)) return
    activateFilmstrip(entry, 'pointer')
    const bounds = event.currentTarget instanceof HTMLElement
      ? event.currentTarget.getBoundingClientRect()
      : null
    if (!bounds || bounds.width <= 0) return
    const ratio = Math.min(0.999999, Math.max(0, (event.clientX - bounds.left) / bounds.width))
    filmstripIndex[entry.id] = Math.floor(ratio * FILMSTRIP_CELLS)
  }

  function scrubFilmstripKeyboard(entry: MediaEntry, event: KeyboardEvent): void {
    if (!filmstripUrl(entry)) return
    let next = currentFilmstripIndex(entry.id)
    if (event.key === 'ArrowLeft') next -= 1
    else if (event.key === 'ArrowRight') next += 1
    else if (event.key === 'Home') next = 0
    else if (event.key === 'End') next = FILMSTRIP_CELLS - 1
    else return
    event.preventDefault()
    activateFilmstrip(entry, 'keyboard')
    filmstripIndex[entry.id] = Math.min(FILMSTRIP_CELLS - 1, Math.max(0, next))
  }

  function filmstripTransform(id: string): string {
    return 'translateX(-' + currentFilmstripIndex(id) * (100 / FILMSTRIP_CELLS) + '%)'
  }

  function filmstripValueText(entry: MediaEntry): string {
    const index = currentFilmstripIndex(entry.id)
    const time = (entry.duration ?? 0) * (index + 0.5) / FILMSTRIP_CELLS
    return 'Кадр ' + (index + 1) + ' из ' + FILMSTRIP_CELLS + ', ' + fmtDuration(time)
  }

  function mediaGlyph(entry: MediaEntry): string {
    const type = inferMediaType(entry)
    if (type === 'audio') return '♪'
    if (type === 'image') return '▧'
    return '▶'
  }

  function add(entry: MediaEntry): void {
    try {
      compositionState.ui.message = ''
      addLibraryEntryToComposition(entry)
    } catch (error) {
      compositionState.ui.message = error instanceof Error ? error.message : String(error)
      setEditorMode('composition')
    }
  }

  function openLegacy(entry: MediaEntry): void {
    setEditorMode('legacy')
    openFromLibrary(entry)
  }

  function beginEdit(entry: MediaEntry): void {
    proxyId = null
    editingId = entry.id
    editTitle = entry.title ?? ''
    editTags = (entry.tags ?? []).join(', ')
    metadataError = ''
  }

  async function toggleProxy(entry: MediaEntry): Promise<void> {
    editingId = null
    if (proxyId === entry.id) {
      proxyId = null
      return
    }
    proxyId = entry.id
    metadataError = ''
    if (!ProxyControlsComponent) {
      try {
        ProxyControlsComponent = (await import('$lib/proxy/ProxyControls.svelte')).default
      } catch {
        if (proxyId === entry.id) proxyId = null
        metadataError = 'Не удалось загрузить панель proxy'
      }
    }
  }

  function cancelEdit(): void {
    editingId = null
    metadataError = ''
  }

  async function toggleFavorite(entry: MediaEntry): Promise<void> {
    busyId = entry.id
    metadataError = ''
    try {
      await updateLibraryMetadata(entry.id, { favorite: !entry.favorite })
    } catch (error) {
      metadataError = error instanceof Error ? error.message : String(error)
    } finally {
      busyId = null
    }
  }

  async function saveMetadata(id: string): Promise<void> {
    busyId = id
    metadataError = ''
    try {
      await updateLibraryMetadata(id, {
        title: validateLibraryTitle(editTitle),
        tags: parseLibraryTags(editTags),
      })
      editingId = null
    } catch (error) {
      metadataError = error instanceof Error ? error.message : String(error)
    } finally {
      busyId = null
    }
  }

  async function deleteEntry(id: string): Promise<void> {
    await deleteFromLibrary(id)
    if (!appState.library.some((entry) => entry.id === id)) {
      proxyController.forget(id)
      if (proxyId === id) proxyId = null
    }
  }
</script>

{#snippet thumbnailContents(
  entry: MediaEntry,
  previewUrl: string | null,
  filmstripPreviewUrl: string | null,
)}
  {#if previewUrl && thumbnailStatus(entry.id) !== 'error'}
    <img
      class:loaded={thumbnailStatus(entry.id) === 'ready'}
      src={previewUrl}
      alt={'Предпросмотр «' + label(entry) + '»'}
      loading="lazy"
      decoding="async"
      onload={() => { thumbnailState[entry.id] = 'ready' }}
      onerror={() => { thumbnailState[entry.id] = 'error' }}
    />
  {/if}
  {#if previewUrl && thumbnailStatus(entry.id) === 'loading'}
    <span class="thumbnail-skeleton" aria-hidden="true"></span>
  {:else if !previewUrl || thumbnailStatus(entry.id) === 'error'}
    <span
      class="thumbnail-fallback"
      role="img"
      aria-label={'Предпросмотр «' + label(entry) + '» недоступен'}
    >{mediaGlyph(entry)}</span>
  {/if}
  {#if filmstripPreviewUrl && filmstripState[entry.id] !== undefined && filmstripState[entry.id] !== 'error'}
    <img
      class="filmstrip-sheet"
      class:loaded={filmstripState[entry.id] === 'ready'}
      src={filmstripPreviewUrl}
      alt=""
      aria-hidden="true"
      decoding="async"
      draggable="false"
      style:transform={filmstripTransform(entry.id)}
      onload={() => { filmstripState[entry.id] = 'ready' }}
      onerror={() => { filmstripState[entry.id] = 'error' }}
    />
  {/if}
  {#if filmstripPreviewUrl && filmstripMode[entry.id] !== undefined && filmstripState[entry.id] === 'ready'}
    <span class="filmstrip-position" aria-hidden="true">
      {currentFilmstripIndex(entry.id) + 1}/{FILMSTRIP_CELLS}
    </span>
  {/if}
{/snippet}

{#if appState.library.length}
  <div class="card library">
    <h2>Медиатека</h2>
    <div class="library-controls" role="search">
      <label class="library-search">
        <span>Поиск по файлу, названию, тегам или типу</span>
        <input type="search" bind:value={query} placeholder="Например: интервью audio" />
      </label>
      <label class="favorite-filter">
        <input type="checkbox" bind:checked={favoritesOnly} />
        Только избранное
      </label>
      <span class="library-count" aria-live="polite">{filteredLibrary.length} из {appState.library.length}</span>
    </div>

    {#if metadataError}
      <p class="library-error" role="alert">{metadataError}</p>
    {/if}

    {#if filteredLibrary.length}
      <ul class="lib-list">
        {#each filteredLibrary as entry (entry.id)}
        {@const previewUrl = thumbnailUrl(entry.id)}
        {@const filmstripPreviewUrl = filmstripUrl(entry)}
        <li class:has-editor={editingId === entry.id || proxyId === entry.id} class="lib-item" data-library-id={entry.id}>
          <button
            class:active={entry.favorite === true}
            class="favorite-button"
            type="button"
            aria-label={entry.favorite ? `Убрать «${label(entry)}» из избранного` : `Добавить «${label(entry)}» в избранное`}
            aria-pressed={entry.favorite === true}
            title={entry.favorite ? 'Убрать из избранного' : 'Добавить в избранное'}
            disabled={busyId === entry.id}
            onclick={() => void toggleFavorite(entry)}
          >{entry.favorite ? '★' : '☆'}</button>
          {#if filmstripPreviewUrl && filmstripState[entry.id] !== 'error'}
            <button
              class="library-thumbnail"
              class:filmstrip-active={filmstripMode[entry.id] !== undefined}
              type="button"
              data-thumbnail-state={previewUrl ? thumbnailStatus(entry.id) : 'error'}
              data-filmstrip-state={filmstripState[entry.id] ?? 'idle'}
              role="slider"
              aria-label={'Раскадровка «' + label(entry) + '»'}
              aria-valuemin={0}
              aria-valuemax={FILMSTRIP_CELLS - 1}
              aria-valuenow={currentFilmstripIndex(entry.id)}
              aria-valuetext={filmstripValueText(entry)}
              onpointerenter={() => activateFilmstrip(entry, 'pointer')}
              onpointermove={(event) => scrubFilmstripPointer(entry, event)}
              onpointerleave={() => deactivateFilmstrip(entry.id, 'pointer')}
              onfocus={() => activateFilmstrip(entry, 'keyboard')}
              onblur={() => deactivateFilmstrip(entry.id, 'keyboard')}
              onkeydown={(event) => scrubFilmstripKeyboard(entry, event)}
            >
              {@render thumbnailContents(entry, previewUrl, filmstripPreviewUrl)}
            </button>
          {:else}
            <div
              class="library-thumbnail"
              data-thumbnail-state={previewUrl ? thumbnailStatus(entry.id) : 'error'}
              data-filmstrip-state="idle"
            >
              {@render thumbnailContents(entry, previewUrl, null)}
            </div>
          {/if}
          <span class={`lib-badge ${entry.kind}`}>{entry.kind === 'output' ? 'результат' : inferMediaType(entry)}</span>
          <div class="lib-info">
            <div class="lib-name" title={label(entry)}>{label(entry)}</div>
            <div class="lib-meta">{ext(entry)}{meta(entry) ? ` · ${meta(entry)}` : ''}</div>
            {#if entry.tags?.length}
              <div class="library-tags" aria-label="Теги">
                {#each entry.tags as tag (tag)}<span>{tag}</span>{/each}
              </div>
            {/if}
          </div>
          <div class="lib-actions">
            {#if entry.kind === 'source'}
              <button class="btn ghost sm" onclick={() => add(entry)}>Добавить</button>
              {#if inferMediaType(entry) === 'video'}
                <button class="btn ghost sm" onclick={() => openLegacy(entry)}>Открыть</button>
                <button
                  class="btn ghost sm"
                  type="button"
                  aria-expanded={proxyId === entry.id}
                  aria-controls={`proxy-${entry.id}`}
                  onclick={() => void toggleProxy(entry)}
                >Proxy</button>
              {/if}
            {:else}
              <a class="btn ghost sm" href={entry.url} download={entry.filename}>Скачать</a>
            {/if}
            <button class="btn ghost sm" type="button" onclick={() => beginEdit(entry)}>Данные</button>
            <button class="btn ghost sm danger" title="Удалить" onclick={() => void deleteEntry(entry.id)}>✕</button>
          </div>

          {#if editingId === entry.id}
            <form
              class="metadata-editor"
              aria-label={`Данные «${label(entry)}»`}
              onsubmit={(event) => {
                event.preventDefault()
                void saveMetadata(entry.id)
              }}
            >
              <label>
                <span>Название</span>
                <input
                  bind:value={editTitle}
                  maxlength={MAX_LIBRARY_TITLE_CHARS}
                  placeholder={entry.filename}
                  disabled={busyId === entry.id}
                />
              </label>
              <label>
                <span>Теги через запятую</span>
                <input
                  bind:value={editTags}
                  aria-describedby={`tag-help-${entry.id}`}
                  placeholder="клиент, черновик"
                  disabled={busyId === entry.id}
                />
              </label>
              <small id={`tag-help-${entry.id}`}>До {MAX_LIBRARY_TAGS} тегов, каждый до {MAX_LIBRARY_TAG_CHARS} символов.</small>
              <div class="metadata-actions">
                <button class="btn primary sm" type="submit" disabled={busyId === entry.id}>Сохранить</button>
                <button class="btn ghost sm" type="button" disabled={busyId === entry.id} onclick={cancelEdit}>Отмена</button>
              </div>
            </form>
          {/if}
          {#if proxyId === entry.id && entry.kind === 'source' && inferMediaType(entry) === 'video' && ProxyControlsComponent}
            <div id={`proxy-${entry.id}`} class="proxy-editor">
              <ProxyControlsComponent {entry} capabilities={appState.capabilities} />
            </div>
          {/if}
        </li>
        {/each}
      </ul>
    {:else}
      <p class="library-empty" role="status">Ничего не найдено. Измените поиск или фильтр избранного.</p>
    {/if}
  </div>
{/if}

<style>
  .library-controls {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) auto auto;
    align-items: end;
    gap: 10px;
    margin-bottom: 14px;
  }

  .library-search {
    display: grid;
    gap: 5px;
    color: var(--muted);
    font-size: 12px;
  }

  .library-search input,
  .metadata-editor input {
    min-width: 0;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--panel-2);
    color: var(--text);
    font: inherit;
  }

  .favorite-filter {
    display: flex;
    align-items: center;
    gap: 7px;
    min-height: 36px;
    color: var(--muted);
    font-size: 13px;
  }

  .library-count {
    min-height: 36px;
    padding-top: 8px;
    color: var(--faint);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .favorite-button {
    flex: none;
    width: 32px;
    height: 32px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-size: 20px;
    line-height: 1;
  }

  .favorite-button:hover:not(:disabled),
  .favorite-button.active {
    border-color: var(--warn);
    color: var(--warn);
  }

  .favorite-button:disabled { opacity: .5; cursor: wait; }

  .library-thumbnail {
    position: relative;
    flex: 0 0 112px;
    width: 112px;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: #11131a;
    padding: 0;
    color: inherit;
    font: inherit;
    appearance: none;
  }

  .library-thumbnail img,
  .thumbnail-skeleton,
  .thumbnail-fallback {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .library-thumbnail img {
    z-index: 1;
    display: block;
    object-fit: contain;
    opacity: 0;
  }

  .library-thumbnail img.loaded { opacity: 1; }

  .library-thumbnail[role='slider'] {
    cursor: ew-resize;
  }

  .library-thumbnail[role='slider']:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .library-thumbnail img.filmstrip-sheet {
    z-index: 2;
    width: 800%;
    max-width: none;
    object-fit: fill;
    opacity: 0;
    user-select: none;
    pointer-events: none;
    will-change: transform;
  }

  .library-thumbnail.filmstrip-active img.filmstrip-sheet.loaded {
    opacity: 1;
  }

  .filmstrip-position {
    position: absolute;
    z-index: 3;
    right: 3px;
    bottom: 3px;
    padding: 1px 4px;
    border-radius: 4px;
    background: rgb(0 0 0 / 72%);
    color: white;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    line-height: 1.3;
    pointer-events: none;
  }

  .thumbnail-skeleton {
    background: linear-gradient(110deg, var(--panel-2) 25%, var(--border) 42%, var(--panel-2) 58%);
    background-size: 200% 100%;
    animation: thumbnail-loading 1.2s ease-in-out infinite;
  }

  .thumbnail-fallback {
    display: grid;
    place-items: center;
    color: var(--faint);
    font-size: 22px;
  }

  @keyframes thumbnail-loading {
    from { background-position: 100% 0; }
    to { background-position: -100% 0; }
  }

  @media (prefers-reduced-motion: reduce) {
    .thumbnail-skeleton { animation: none; }
  }

  .library-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 5px;
  }

  .library-tags span {
    padding: 1px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    color: var(--muted);
    font-size: 10px;
  }

  .lib-item.has-editor { flex-wrap: wrap; }

  .metadata-editor {
    display: grid;
    grid-template-columns: minmax(160px, 1fr) minmax(220px, 1.4fr) auto;
    align-items: end;
    gap: 10px;
    flex: 1 0 100%;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }

  .metadata-editor label {
    display: grid;
    gap: 5px;
    color: var(--muted);
    font-size: 12px;
  }

  .metadata-editor small {
    grid-column: 1 / -1;
    color: var(--faint);
    font-size: 11px;
  }

  .metadata-actions { display: flex; gap: 6px; }
  .proxy-editor { display: contents; }
  .library-error { margin: 0 0 12px; color: var(--danger); font-size: 13px; }
  .library-empty { margin: 4px 0 0; color: var(--muted); font-size: 13px; }

  @media (max-width: 760px) {
    .library-controls { grid-template-columns: 1fr; align-items: start; }
    .library-count { min-height: 0; padding-top: 0; }
    .metadata-editor { grid-template-columns: 1fr; }
    .library-thumbnail { flex-basis: 96px; width: 96px; }
  }
</style>
