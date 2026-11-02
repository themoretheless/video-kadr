<script lang="ts">
  import { editFilmcraftSubclip, getFilmcraftSubclipDefaults, type FilmcraftImportResult, type FilmcraftSourceEditFence, type FilmcraftSubclipDefaults, type FilmcraftSubclipInput } from '$lib/api'
  let { document, sequenceId, clipId, token, items, sourceItem, sourcePlayhead, selection, selected, stillDuration = $bindable('5'), stillUnit = $bindable<'seconds' | 'frames'>('seconds'), disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; items: NonNullable<FilmcraftImportResult['projectItems']>
    sourceItem: string; sourcePlayhead: string; selection: string[]; selected: string[]; stillDuration?: string; stillUnit?: 'seconds' | 'frames'; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let mode = $state<'create' | 'edit'>('create')
  let origin = $state<'automatic' | 'source' | 'project' | 'timeline'>('automatic')
  let itemId = $state('')
  let autoName = $state(true)
  let name = $state('')
  let startOverride = $state(false)
  let endOverride = $state(false)
  let start = $state('0')
  let end = $state('0')
  let unit = $state<'ticks' | 'frames'>('ticks')
  let restrict = $state(true)
  let restrictOverride = $state(false)
  let convert = $state(false)
  let defaults = $state<FilmcraftSubclipDefaults | null>(null)
  let busy = $state(false)
  let error = $state('')
  let status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function tick(value: string) { return /^-?\d{1,19}$/.test(value) && BigInt(value) >= -9223372036854775808n && BigInt(value) <= 9223372036854775807n }
  const currentItem = $derived(items.find(item => item.id === itemId) ?? (mode === 'edit' ? items.find(item => selected.includes(item.id) && item.kind === 'Subclip') : undefined))
  const targetReady = $derived(mode === 'edit' ? itemId ? currentItem?.kind === 'Subclip' : selected.some(id => items.some(item => item.id === id && item.kind === 'Subclip')) : origin === 'source' ? !!sourceItem : origin === 'project' ? !!itemId : origin === 'timeline' ? !!sequenceId && !!clipId : !!sourceItem || selected.length > 0 || selection.length > 0)
  const validPreferences = $derived(stillDuration.trim() !== '' && Number.isFinite(Number(stillDuration)))
  const valid = $derived((!startOverride || tick(start)) && (!endOverride || tick(end)) && validPreferences)
  function context() { return JSON.stringify([document, sequenceId, clipId, token, items.map(item => [item.id, item.kind]), sourceItem, sourcePlayhead, selection, selected, mode, origin, itemId, autoName, name, startOverride, endOverride, start, end, unit, restrict, restrictOverride, convert, stillDuration, stillUnit, disabled]) }
  function input(): FilmcraftSubclipInput {
    return { document, ...(mode === 'edit' ? itemId ? { item: itemId } : {} : origin === 'source' ? { item: sourceItem } : origin === 'project' ? { item: itemId } : origin === 'timeline' ? { clip: clipId } : {}), ...(mode === 'create' && !autoName ? { name } : {}), ...(startOverride ? unit === 'frames' ? { startFrame: start } : { start } : {}), ...(endOverride ? unit === 'frames' ? { endFrame: end } : { end } : {}), ...(mode === 'create' || restrictOverride ? { restrictTrims: restrict } : {}), ...(mode === 'edit' ? { convertToMaster: convert } : {}), context: { activeSequence: sequenceId || null, sourceItem: sourceItem || null, selection: [...selection], projectSelection: [...selected], stillImageDuration: Number(stillDuration), stillImageUnit: stillUnit } }
  }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => {
    void document; void sequenceId; void clipId; void token; void sourceItem; void sourcePlayhead; void selection; void selected; void mode; void origin; void stillDuration; void stillUnit
    if (itemId && !items.some(item => item.id === itemId && (mode !== 'edit' || item.kind === 'Subclip'))) itemId = ''
    defaults = null; error = ''; status = ''
  })
  async function run(preview: boolean) {
    if (!token || disabled || busy || !targetReady || !validPreferences || !preview && !valid) return
    const captured = context(); const current = new AbortController()
    const expected = { document, sequenceId, clipId, token, sourceItem: sourceItem || null, sourcePlayhead, timelineSelection: [...selection], projectSelection: [...selected] }
    controller = current; busy = true; error = ''; status = ''
    try {
      if (preview) {
        const result = await getFilmcraftSubclipDefaults(token, { ...input(), forEdit: mode === 'edit' }, current.signal)
        if (!alive || current.signal.aborted || captured !== context()) return
        defaults = result; start = unit === 'ticks' ? result.startTicks : result.startFrame; end = unit === 'ticks' ? result.endTicks : result.endFrame; if (autoName) name = result.name; restrict = result.restrictTrims
      } else {
        const result = await editFilmcraftSubclip(token, mode === 'create' ? 'clip.makeSubclip' : 'clip.editSubclip', input(), current.signal)
        if (!alive || current.signal.aborted || captured !== context()) return
        status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : mode === 'create' ? 'Подклип создан' : convert ? 'Подклип преобразован в мастер-клип' : 'Подклип обновлён'
      }
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Подклипы</summary>
  <label>Действие <select aria-label="Действие подклипа Filmcraft" value={mode} onchange={event => { mode = event.currentTarget.value as typeof mode; itemId = '' }} disabled={disabled || busy}><option value="create">Создать</option><option value="edit">Изменить</option></select></label>
  {#if mode === 'create'}
    <label>Источник <select aria-label="Источник подклипа Filmcraft" value={origin} onchange={event => { origin = event.currentTarget.value as typeof origin; itemId = '' }} disabled={disabled || busy}><option value="automatic">Автоматически: Source, проект, таймлайн</option><option value="source">Source Monitor</option><option value="project">Элемент проекта</option><option value="timeline">Выбранный клип таймлайна</option></select></label>
    <label><input type="checkbox" aria-label="Автоматическое имя подклипа Filmcraft" bind:checked={autoName} disabled={disabled || busy} />Автоматическое имя</label>
    <label>Имя <input aria-label="Имя подклипа Filmcraft" bind:value={name} disabled={disabled || busy || autoName} /></label>
  {/if}
  {#if mode === 'edit' || origin === 'project'}<label>Элемент <select aria-label="Элемент подклипа Filmcraft" value={itemId} onchange={event => { itemId = event.currentTarget.value }} disabled={disabled || busy}><option value="">{mode === 'edit' ? 'Из выделения проекта' : 'Выберите элемент'}</option>{#each items.filter(item => mode !== 'edit' || item.kind === 'Subclip') as item (item.id)}<option value={item.id}>{item.name || item.id}</option>{/each}</select></label>{/if}
  {#if currentItem?.subclip}<span aria-label="Границы подклипа Filmcraft">Сейчас: {currentItem.subclip.startTicks} — {currentItem.subclip.endTicks} тиков; ограничение тримминга {currentItem.subclip.restrictTrims ? 'включено' : 'выключено'}</span>{/if}
  <button type="button" disabled={disabled || busy || !token || !targetReady || !validPreferences} onclick={() => run(true)}>Показать исходные параметры подклипа</button>
  {#if defaults}<span aria-label="Параметры подклипа Filmcraft">{defaults.name}: {defaults.startTicks} — {defaults.endTicks} тиков, {defaults.startFrame} — {defaults.endFrame} кадров, {defaults.fps} fps</span>{/if}
  <label>Единицы <select aria-label="Единицы границ подклипа Filmcraft" value={unit} onchange={event => { unit = event.currentTarget.value as typeof unit }} disabled={disabled || busy}><option value="ticks">Тики</option><option value="frames">Кадры медиа</option></select></label>
  <label><input type="checkbox" aria-label="Изменить начало подклипа Filmcraft" bind:checked={startOverride} disabled={disabled || busy} />Задать начало</label>
  <input aria-label="Начало подклипа Filmcraft" bind:value={start} disabled={disabled || busy || !startOverride} />
  <label><input type="checkbox" aria-label="Изменить конец подклипа Filmcraft" bind:checked={endOverride} disabled={disabled || busy} />Задать конец (не включая)</label>
  <input aria-label="Конец подклипа Filmcraft" bind:value={end} disabled={disabled || busy || !endOverride} />
  {#if mode === 'edit'}<label><input type="checkbox" aria-label="Изменить ограничение подклипа Filmcraft" bind:checked={restrictOverride} disabled={disabled || busy} />Изменить ограничение тримминга</label>{/if}
  <label><input type="checkbox" aria-label="Ограничить тримминг подклипа Filmcraft" bind:checked={restrict} disabled={disabled || busy || mode === 'edit' && !restrictOverride} />Ограничить тримминг границами подклипа</label>
  {#if mode === 'edit'}<label><input type="checkbox" aria-label="Мастер-клип из подклипа Filmcraft" bind:checked={convert} disabled={disabled || busy} />Преобразовать в мастер-клип</label>{/if}
  <label>Длительность неподвижного изображения <input aria-label="Длительность изображения для подклипа Filmcraft" bind:value={stillDuration} disabled={disabled || busy} /></label>
  <label>Единицы длительности <select aria-label="Единицы изображения для подклипа Filmcraft" value={stillUnit} onchange={event => { stillUnit = event.currentTarget.value as typeof stillUnit }} disabled={disabled || busy}><option value="seconds">Секунды</option><option value="frames">Кадры</option></select></label>
  <button type="button" disabled={disabled || busy || !token || !targetReady || !valid} onclick={() => run(false)}>{mode === 'create' ? 'Создать подклип' : 'Применить подклип'}</button>
  {#if busy}<span role="status">Обрабатывается подклип…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
