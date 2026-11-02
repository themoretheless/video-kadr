<script lang="ts">
  import { tick } from 'svelte'
  import { compositionSearchEntries, searchCompositionEntries, type SearchEntry } from './composition.js'
  import { SHORTCUT_DEFINITIONS } from '../shortcuts.js'
  import { compositionCommandState, executeCompositionCommand, type CompositionCommandId } from '../composition/commandRegistry.js'
  import { compositionState, selectCompositionClip } from '../state/composition/core.svelte.js'
  import { setCompositionPlayhead } from '../state/composition/transport.svelte.js'
  import { addLibraryEntryToComposition, addMediaInfoToComposition } from '../state/composition/media.svelte.js'
  import { state as libraryState } from '../state/store.svelte.js'

  let dialog = $state<HTMLDialogElement>()
  let input = $state<HTMLInputElement>()
  let query = $state('')
  let active = $state(0)
  let error = $state('')
  let previousFocus: HTMLElement | null = null
  const commands = $derived(SHORTCUT_DEFINITIONS.filter((command) => command.mode === 'composition').map((command): SearchEntry => ({ id: command.id, kind: 'command', label: command.label, context: command.description, ...compositionCommandState(command.id as CompositionCommandId) })))
  const media = $derived(Object.values({
    ...Object.fromEntries(Object.values(compositionState.media).map((item) => [item.id, { id: item.id, label: item.filename }])),
    ...Object.fromEntries(libraryState.library.filter((item) => item.kind === 'source').map((item) => [item.id, { id: item.id, label: item.title?.trim() || item.filename }])),
  }))
  const entries = $derived(compositionSearchEntries(compositionState.document, media, commands))
  const results = $derived(searchCompositionEntries(entries, query))
  const category = { command: 'Команда', media: 'Медиа', clip: 'Клип', caption: 'Субтитр', marker: 'Маркер' }

  async function open(): Promise<void> {
    previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null
    query = ''; active = 0; error = ''
    dialog?.showModal()
    await tick()
    input?.focus()
  }
  function close(): void { dialog?.close(); previousFocus?.focus() }
  function shortcut(event: KeyboardEvent): void {
    if (!event.isComposing && !event.altKey && (event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault()
      if (dialog?.open) close()
      else void open()
    }
  }
  function navigate(event: KeyboardEvent): void {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault()
      active = results.length ? (active + (event.key === 'ArrowDown' ? 1 : -1) + results.length) % results.length : 0
      document.getElementById(`composition-search-result-${active}`)?.scrollIntoView({ block: 'nearest' })
    } else if (event.key === 'Enter' && results[active]) {
      event.preventDefault(); choose(results[active]!)
    }
  }
  function choose(entry: SearchEntry): void {
    if (entry.enabled === false) return
    try {
      if (entry.kind === 'command') executeCompositionCommand(entry.id as CompositionCommandId)
      else if (entry.kind === 'media') {
        const source = libraryState.library.find((item) => item.id === entry.id)
        if (source) addLibraryEntryToComposition(source)
        else if (compositionState.media[entry.id]) addMediaInfoToComposition(compositionState.media[entry.id]!)
        else throw new Error('Источник больше не доступен')
      } else {
        if (entry.trackId) selectCompositionClip(entry.trackId, entry.id)
        if (entry.kind === 'marker') compositionState.ui.selectedMarkerId = entry.id
        if (entry.tick !== undefined) setCompositionPlayhead(entry.tick)
      }
      close()
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
</script>

<svelte:window onkeydown={shortcut} />
<button class="btn ghost sm" type="button" onclick={() => void open()}>Поиск · Ctrl/⌘ K</button>
<dialog bind:this={dialog} aria-label="Поиск в редакторе" onclose={() => previousFocus?.focus()}>
  <div class="search-head">
    <input bind:this={input} bind:value={query} oninput={() => { active = 0 }} onkeydown={navigate} role="combobox" aria-label="Команды, медиа, клипы, субтитры и маркеры" aria-expanded="true" aria-controls="composition-search-results" aria-activedescendant={results.length ? `composition-search-result-${active}` : undefined} placeholder="Найти в проекте…" />
    <button class="btn ghost sm" type="button" onclick={close} aria-label="Закрыть поиск">×</button>
  </div>
  <div id="composition-search-results" role="listbox" aria-label="Результаты поиска">
    {#each results as entry, index (`${entry.kind}-${entry.id}`)}
      <button id={`composition-search-result-${index}`} type="button" role="option" aria-selected={index === active} aria-disabled={entry.enabled === false} tabindex="-1" onclick={() => choose(entry)}>
        <span>{category[entry.kind]} · {entry.label}</span>
        <small>{entry.enabled === false ? entry.disabledReason : entry.context}</small>
      </button>
    {/each}
  </div>
  {#if !results.length}<p role="status">Ничего не найдено</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</dialog>

<style>
  dialog { width: min(540px, calc(100vw - 24px)); padding: 12px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--panel); color: var(--text); }
  dialog::backdrop { background: color-mix(in srgb, var(--text) 20%, transparent); }
  .search-head { display: flex; gap: 8px; }
  .search-head input { flex: 1; min-width: 0; }
  [role='listbox'] { max-height: min(420px, 65vh); overflow: auto; margin-top: 8px; }
  [role='option'] { display: flex; flex-direction: column; width: 100%; padding: 8px; text-align: left; background: transparent; color: var(--text); border: 0; }
  [role='option'][aria-selected='true'] { background: var(--accent-soft); }
  [role='option'][aria-disabled='true'], small { color: var(--muted); }
</style>
