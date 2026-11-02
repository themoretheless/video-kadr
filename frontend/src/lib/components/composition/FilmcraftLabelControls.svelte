<script lang="ts">
  import { untrack } from 'svelte'
  import { selectFilmcraftSelection, type FilmcraftSelectionCommand, selectFilmcraftLabelGroup, type FilmcraftLabelGroupSelection, editFilmcraftLabels, FILMCRAFT_LABELS, type FilmcraftLabel, type FilmcraftLabelCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, items, palette, clipLabel, clipSelection, selected = $bindable<string[]>([]), disabled = false, onedit, onselection = () => false }: {
    document: string; sequenceId: string; clipId: string; token: string | null
    items: NonNullable<FilmcraftImportResult['projectItems']>; palette: NonNullable<FilmcraftImportResult['labelPalette']>; clipLabel?: FilmcraftLabel; clipSelection?: string[]; selected?: string[]; disabled?: boolean
    onselection?: (result: FilmcraftLabelGroupSelection, expected: FilmcraftSourceEditFence) => boolean | void
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  const timeline = $derived(clipSelection ?? (clipId ? [clipId] : []))
  let label = $state<FilmcraftLabel>('Violet')
  let operation = $state<'label' | 'selection'>('label')
  let busy = $state(false)
  let error = $state('')
  let status = $state('')
  let alive = true
  let controller: AbortController | undefined
  function context() { return JSON.stringify([document, sequenceId, clipId, token, disabled, selected, timeline, label]) }
  function color() {
    const rgb = palette.find(row => row.name === label)?.rgb
    return rgb?.length === 3 && rgb.every(value => Number.isInteger(value) && value >= 0 && value <= 255) ? `rgb(${rgb.join(',')})` : 'transparent'
  }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => {
    void document; void sequenceId; void clipId; void token; void disabled
    const ids = new Set(items.map(item => item.id))
    untrack(() => { if (selected.some(id => !ids.has(id))) selected = selected.filter(id => ids.has(id)); error = ''; status = '' })
    return () => { controller?.abort() }
  })
  async function apply(target: 'items' | 'clip') {
    if (!token || busy || disabled || (target === 'clip' ? !timeline.length : !selected.length || selected.some(id => !items.some(item => item.id === id)))) return
    const expected = { document, sequenceId, clipId, token, projectSelection: [...selected], timelineSelection: [...timeline] }
    const captured = context(); const current = new AbortController()
    controller = current; busy = true; operation = 'label'; error = ''; status = ''
    try {
      const result = await editFilmcraftLabels(token, `edit.label.${label.toLowerCase()}` as FilmcraftLabelCommand, { document, ...(sequenceId ? { sequence: sequenceId } : {}), ...(target === 'clip' ? { clips: [...timeline] } : { items: [...selected] }) }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Метки сохранены'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
  async function group(command: FilmcraftSelectionCommand | 'edit.selectLabelGroup') {
    if (!token || busy || disabled || (command === 'edit.selectLabelGroup' && !selected.length && !timeline.length) || (command === 'edit.selectAllMatching' && (!timeline.length || !sequenceId))) return
    const expected = { document, sequenceId, clipId, token, projectSelection: [...selected], timelineSelection: [...timeline] }
    const captured = context(); const current = new AbortController()
    controller = current; busy = true; operation = 'selection'; error = ''; status = ''
    try {
      const input = { document, ...(sequenceId ? { sequence: sequenceId } : {}), context: { selection: [...timeline], projectSelection: [...selected] } }
      const result = command === 'edit.selectLabelGroup' ? await selectFilmcraftLabelGroup(token, input, current.signal) : await selectFilmcraftSelection(token, command, input, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onselection(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Выделение обновлено'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Метки элементов и клипов</summary>
  <label>Цветовая метка <select aria-label="Метка Filmcraft" value={label} onchange={(event) => { label = event.currentTarget.value as FilmcraftLabel }} disabled={disabled || busy || !token}>{#each FILMCRAFT_LABELS as name (name)}<option value={name}>{name}</option>{/each}</select></label>
  <span class="swatch" aria-label="Цвет метки Filmcraft" style:background-color={color()}></span>
  <label>Элементы проекта <select aria-label="Элементы для меток Filmcraft" multiple value={selected} disabled={disabled || busy || !token} onchange={(event) => { selected = [...event.currentTarget.options].filter(option => option.selected).map(option => option.value) }}>{#each items as item (item.id)}<option value={item.id}>{item.name || item.id}{item.label ? ` — ${item.label}` : ''}</option>{/each}</select></label>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={() => apply('items')}>Метка для элементов</button>
  <button type="button" disabled={disabled || busy || !token || !timeline.length} onclick={() => apply('clip')}>{timeline.length > 1 ? 'Метка для клипов' : 'Метка для клипа'}</button>
  <button type="button" disabled={disabled || busy || !token || (!selected.length && !timeline.length)} onclick={() => group('edit.selectLabelGroup')}>{timeline.length ? 'Выбрать клипы по метке' : 'Выбрать элементы по метке'}</button>
  <button type="button" disabled={disabled || busy || !token || !timeline.length || !sequenceId} onclick={() => group('edit.selectAllMatching')}>Выбрать клипы того же исходного элемента</button>
  <button type="button" disabled={disabled || busy || !token} onclick={() => group('project.selectAll')}>Выбрать все элементы проекта</button>
  <button type="button" disabled={disabled || busy || !token || !selected.length} onclick={() => group('project.deselectAll')}>Снять выделение элементов проекта</button>
  {#if clipLabel}<span>Метка клипа: {clipLabel}</span>{/if}
  {#if busy}<span role="status">{operation === 'selection' ? 'Обновляется выделение…' : 'Сохраняются метки…'}</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
<style>
  .swatch { display: inline-block; width: 1em; height: 1em; border: 1px solid currentColor; vertical-align: middle; }
</style>
