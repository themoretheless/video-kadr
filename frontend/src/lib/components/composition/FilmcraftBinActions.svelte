<script lang="ts">
  import { untrack } from 'svelte'
  import { deleteFilmcraftProjectItems, duplicateFilmcraftProjectItems, editFilmcraftBins, type FilmcraftBinEditCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, items, bins, selected = $bindable<string[]>([]), clipSelection, revealRequest = null, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null
    items: NonNullable<FilmcraftImportResult['projectItems']>; bins: NonNullable<FilmcraftImportResult['projectBins']>; selected?: string[]; clipSelection?: string[]; revealRequest?: { itemId: string; serial: number } | null; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let details: HTMLDetailsElement | undefined
  let itemSelect: HTMLSelectElement | undefined
  $effect(() => { const request = revealRequest; if (!request) return; untrack(() => { if (items.some(item => item.id === request.itemId) && details && itemSelect) { details.open = true; itemSelect.focus(); [...itemSelect.options].find(option => option.value === request.itemId)?.scrollIntoView?.({ block: 'nearest' }) } }) })
  let target = $state('')
  let automatic = $state(true)
  let name = $state('')
  let operation = $state<'bins' | 'duplicate' | 'delete'>('bins')
  let busy = $state(false)
  let error = $state('')
  let status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function location(id: string) { return bins.find(bin => bin.itemIds?.includes(id))?.name }
  function context() { return JSON.stringify([document, sequenceId, clipId, token, disabled, selected, clipSelection, target, automatic, name]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => {
    void document; void sequenceId; void clipId; void token; void disabled
    const itemIds = new Set(items.map(item => item.id)); const binIds = new Set(bins.map(bin => bin.id))
    untrack(() => { if (selected.some(id => !itemIds.has(id))) selected = selected.filter(id => itemIds.has(id)); if (target && !binIds.has(target)) target = ''; error = ''; status = '' })
    return () => { controller?.abort() }
  })
  async function run(command: FilmcraftBinEditCommand) {
    if (!token || disabled || busy || (command !== 'file.newBin' && !selected.length)) return
    if (selected.some(id => !items.some(item => item.id === id)) || target && !bins.some(bin => bin.id === target)) return
    const expected = { document, sequenceId, clipId, token, projectSelection: [...selected] }
    const captured = context()
    const current = new AbortController()
    controller = current; busy = true; operation = 'bins'; error = ''; status = ''
    const input = { document, ...(command !== 'file.newBin' ? { items: [...selected] } : {}), ...(command === 'file.newBin' ? { parent: target || null } : command === 'project.moveToBin' ? { bin: target || null } : {}), ...(command !== 'project.moveToBin' && !automatic ? { name } : {}) }
    try {
      const result = await editFilmcraftBins(token, command, input, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Папки проекта обновлены'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
  async function duplicate() {
    if (!token || disabled || busy || !selected.length || selected.some(id => !items.some(item => item.id === id))) return
    const expected = { document, sequenceId, clipId, token, projectSelection: [...selected] }
    const captured = context(); const current = new AbortController()
    controller = current; busy = true; operation = 'duplicate'; error = ''; status = ''
    try {
      const result = await duplicateFilmcraftProjectItems(token, { document, context: { projectSelection: [...selected] } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      if (onedit(result, expected) === false) status = 'Результат не применён: контекст изменился'
      else { selected = [...result.duplicatedItemIds]; status = 'Копии элементов созданы' }
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
  async function remove(folder: boolean) {
    const ids = folder ? [target] : [...selected]
    if (!token || disabled || busy || !ids.length || (folder ? !target || !bins.some(bin => bin.id === target && bin.parentId !== null) : ids.some(id => !items.some(item => item.id === id)))) return
    const selection = clipSelection ?? (clipId ? [clipId] : [])
    const expected = { document, sequenceId, clipId, token, projectSelection: [...selected], timelineSelection: [...selection] }
    const captured = context(); const current = new AbortController()
    controller = current; busy = true; operation = 'delete'; error = ''; status = ''
    try {
      const result = await deleteFilmcraftProjectItems(token, { document, items: ids, context: { activeSequence: sequenceId || null, selection: [...selection], projectSelection: folder ? ids : [...selected] } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      if (onedit(result, expected) === false) status = 'Результат не применён: контекст изменился'
      else { selected = []; status = 'Удалено из проекта. Правку можно отменить.' }
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }

</script>
<details bind:this={details}>
  <summary>Элементы и папки проекта</summary>
  <label>Элементы <select bind:this={itemSelect} aria-label="Элементы для организации Filmcraft" multiple value={selected} disabled={disabled || busy || !token} onchange={(event) => { selected = [...event.currentTarget.options].filter(option => option.selected).map(option => option.value) }}>
    {#each items as item (item.id)}<option value={item.id}>{item.name || item.id}{location(item.id) ? ` — ${location(item.id)}` : ''}</option>{/each}
  </select></label>
  <label>Папка назначения <select aria-label="Папка назначения Filmcraft" value={target} disabled={disabled || busy || !token} onchange={(event) => { target = event.currentTarget.value }}><option value="">Корень проекта</option>{#each bins.filter(bin => bin.parentId !== null) as bin (bin.id)}<option value={bin.id}>{'—'.repeat(Math.min(bin.depth, 10))} {bin.name || bin.id}</option>{/each}</select></label>
  <label><input type="checkbox" aria-label="Автоматическое имя папки Filmcraft" bind:checked={automatic} disabled={disabled || busy || !token} />Автоматическое имя</label>
  <label>Имя новой папки <input aria-label="Имя новой папки Filmcraft" bind:value={name} disabled={disabled || busy || !token || automatic} /></label>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={duplicate}>Дублировать элементы проекта</button>
  <span>Копии появятся в корне проекта.</span>
  <button type="button" disabled={disabled || busy || !token} onclick={() => run('file.newBin')}>Создать папку</button>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={() => run('file.newBinFromSelection')}>Создать папку из выделения</button>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={() => run('project.moveToBin')}>Переместить в папку</button>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={() => remove(false)}>Удалить элементы из проекта</button>
  <button type="button" disabled={disabled || busy || !token || !target} onclick={() => remove(true)}>Удалить папку с содержимым</button>
  {#if busy}<span role="status">{operation === 'duplicate' ? 'Создаются копии…' : operation === 'delete' ? 'Удаляется из проекта…' : 'Обновляются папки…'}</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
