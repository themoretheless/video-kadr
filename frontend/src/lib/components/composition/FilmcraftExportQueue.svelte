<script lang="ts">
  import { downloadFilmcraftVideo, getFilmcraftVideoJob, listFilmcraftExportQueue, editFilmcraftExportQueue, type FilmcraftExportQueue, type FilmcraftExportQueueCommand, type FilmcraftExportQueueItem, type FilmcraftVideoRequest } from '$lib/api'
  import { filmcraftDownloadName } from '$lib/filmcraft/export-filename'
  import { exactExportRange } from '$lib/filmcraft/export-range'
  import { captionSidecarBlob, readCaptionSidecar } from '$lib/filmcraft/export-captions'
  import { SvelteMap } from 'svelte/reactivity'
  let { token, disabled = false, batchDisabled = false, input, sequences = [], activeSequence }: {
    token: string | null; disabled?: boolean; batchDisabled?: boolean; input: () => FilmcraftVideoRequest
    sequences?: Array<{ id: string; name: string; hasWorkArea: boolean }>; activeSequence?: string
  } = $props()
  type RangeDraft = { mode: 'entire' | 'inOut' | 'workArea' | 'custom'; start: string; end: string }
  let sequenceSelection = $state<string[] | null>(null)
  let ranges = $state<RangeDraft[]>([])
  const picked = $derived(sequences.filter(sequence => (sequenceSelection ?? (activeSequence ? [activeSequence] : [])).includes(sequence.id)))
  const canUseWorkArea = $derived(picked.length > 0 && picked.every(sequence => sequence.hasWorkArea))
  const resolved = $derived(ranges.map(row => row.mode === 'custom' ? exactExportRange(row.start, row.end)
    : row.mode === 'workArea' && !canUseWorkArea ? undefined : row.mode))
  const batchReady = $derived(picked.length > 0 && !batchDisabled && (ranges.length ? resolved.every(range => range !== undefined) : !disabled))
  const batchCount = $derived(picked.length * Math.max(1, ranges.length))
  const formatSeconds = new Intl.NumberFormat('ru-RU', { maximumFractionDigits: 6 })
  let queue = $state<FilmcraftExportQueue>({ running: false, items: [] })
  let busy = $state(false)
  let error = $state('')
  let downloading = $state('')
  let downloadError = $state('')
  const outputFormats = ['mkv', 'mp4', 'mov', 'mxf', 'zip', 'gif', 'wav', 'aiff'] as const
  const outputUrls = new SvelteMap<string, ReturnType<typeof setTimeout>>()
  let revision = 0
  let controller: AbortController | null = null
  $effect(() => {
    const actor = token
    const current = new AbortController()
    controller = current
    queue = { running: false, items: [] }; error = ''; busy = false; downloading = ''; downloadError = ''
    let timer: ReturnType<typeof setTimeout> | undefined
    async function poll() {
      if (!actor) return
      const version = revision
      try {
        const result = await listFilmcraftExportQueue(actor, current.signal)
        if (!current.signal.aborted && version === revision && !busy) { queue = result; error = '' }
      } catch (failure) {
        if (!current.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
      }
      if (!current.signal.aborted) timer = setTimeout(poll, 1000)
    }
    void poll()
    return () => {
      current.abort()
      clearTimeout(timer)
      for (const [url, timeout] of outputUrls) { clearTimeout(timeout); URL.revokeObjectURL(url) }
      outputUrls.clear()
      if (controller === current) controller = null
    }
  })
  async function command(value: FilmcraftExportQueueCommand) {
    const current = controller; const actor = token
    if (!actor || !current || busy) return
    busy = true; error = ''; revision++
    try {
      const result = await editFilmcraftExportQueue(actor, value, current.signal)
      if (!current.signal.aborted) { queue = result }
    } catch (failure) {
      if (!current.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
    } finally { if (!current.signal.aborted) { revision++; busy = false } }
  }
  function toggleSequence(id: string, checked: boolean) {
    const ids = picked.map(sequence => sequence.id)
    sequenceSelection = checked ? [...ids, id] : ids.filter(sequence => sequence !== id)
  }
  function enqueueBatch() {
    if (!batchReady) return
    const bounds = resolved.filter(range => range !== undefined)
    void command({ action: 'add', input: input(), sequences: picked.map(sequence => sequence.id),
      ...(bounds.length ? { ranges: bounds } : {}) })
  }
  async function download(item: FilmcraftExportQueueItem, captions = false) {
    const current = controller
    const actor = token
    const format = outputFormats.find(format => format === item.container)
    if (!current || !actor || downloading || item.status !== 'done' || !item.jobId || !format) return
    downloading = captions ? `${item.id}:captions` : item.id
    downloadError = ''
    try {
      const job = await getFilmcraftVideoJob(actor, item.jobId, current.signal)
      if (current.signal.aborted) return
      const result = job.result
      if (job.status !== 'done' || !result || typeof result !== 'object' || !('id' in result) || typeof result.id !== 'string') {
        throw new Error('Готовый файл экспорта недоступен')
      }
      const sidecar = captions ? readCaptionSidecar(result) : null
      if (captions && !sidecar) throw new Error('Файл субтитров недоступен')
      const blob = sidecar ? captionSidecarBlob(sidecar) : await downloadFilmcraftVideo(actor, result.id, current.signal, format)
      if (current.signal.aborted) return
      const url = URL.createObjectURL(blob)
      outputUrls.set(url, setTimeout(() => { URL.revokeObjectURL(url); outputUrls.delete(url) }, 1000))
      const anchor = document.createElement('a')
      anchor.href = url
      anchor.download = filmcraftDownloadName(result, result.id, sidecar?.extension ?? format)
      document.body.append(anchor)
      try { anchor.click() } finally { anchor.remove() }
    } catch (failure) {
      if (!current.signal.aborted) downloadError = failure instanceof Error ? failure.message : String(failure)
    } finally {
      if (!current.signal.aborted) downloading = ''
    }
  }
  const labels = { ready: 'Ожидает', encoding: 'Экспортируется', done: 'Готово', failed: 'Ошибка', cancelled: 'Отменено' }
</script>

<section aria-label="Очередь экспорта Filmcraft">
  <h4>Очередь экспорта</h4>
  <button type="button" disabled={!token || busy || disabled} onclick={() => command({ action: 'add', input: input() })}>Добавить в очередь</button>
  {#if sequences.length}
    <details>
      <summary>Пакетный экспорт</summary>
      <fieldset>
        <legend>Секвенции</legend>
        {#each sequences as sequence (sequence.id)}
          <label><input type="checkbox" checked={picked.some(item => item.id === sequence.id)} disabled={busy}
            onchange={event => toggleSequence(sequence.id, event.currentTarget.checked)} />{sequence.name || sequence.id}</label>
        {/each}
      </fieldset>
      <fieldset>
        <legend>Диапазоны</legend>
        {#if !ranges.length}<p>Будет использован выбранный диапазон обычного экспорта.</p>{/if}
        {#each ranges as row, index (row)}
          <div class="range-row">
            <select aria-label={`Диапазон пакета ${index + 1}`} value={row.mode} disabled={busy}
              onchange={event => { row.mode = event.currentTarget.value as RangeDraft['mode'] }}>
              <option value="entire">Вся секвенция</option>
              <option value="inOut">Точки In/Out</option>
              <option value="workArea" disabled={!canUseWorkArea}>Рабочая область</option>
              <option value="custom">Свой диапазон</option>
            </select>
            {#if row.mode === 'custom'}
              <label>Начало, с <input aria-label={`Начало диапазона пакета ${index + 1}`} inputmode="decimal" bind:value={row.start} disabled={busy} /></label>
              <label>Конец, с <input aria-label={`Конец диапазона пакета ${index + 1}`} inputmode="decimal" bind:value={row.end} disabled={busy} /></label>
            {/if}
            <button type="button" disabled={busy} aria-label={`Убрать диапазон пакета ${index + 1}`}
              onclick={() => { ranges = ranges.filter(candidate => candidate !== row) }}>×</button>
          </div>
        {/each}
        <button type="button" disabled={busy} onclick={() => { ranges = [...ranges, { mode: 'entire', start: '0', end: '' }] }}>Добавить диапазон</button>
      </fieldset>
      <button type="button" disabled={!token || busy || !batchReady} onclick={enqueueBatch}>Добавить пакет в очередь</button>
      {#if batchCount && batchReady}<span>Задач: {batchCount}</span>{/if}
    </details>
  {/if}
  <button type="button" disabled={!token || busy || queue.running || !queue.items.some(item => item.status === 'ready')} onclick={() => command({ action: 'start' })}>Запустить очередь</button>
  <button type="button" disabled={!token || busy || !queue.running} onclick={() => command({ action: 'stop' })}>Остановить после текущего</button>
  <button type="button" disabled={!token || busy || !queue.items.some(item => ['ready', 'encoding'].includes(item.status))} onclick={() => command({ action: 'cancel' })}>Отменить все</button>
  <button type="button" disabled={!token || busy || !queue.items.some(item => ['done', 'failed', 'cancelled'].includes(item.status))} onclick={() => command({ action: 'clear' })}>Убрать завершённые</button>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if downloadError}<p role="alert">{downloadError}</p>{/if}
  {#each queue.items as item, index (item.id)}
    <article aria-label={`Экспорт ${item.id}`}>
      <p>{item.sequenceName} · {item.preset} · {labels[item.status]}</p>
      {#if item.sourceRange}
        <p title={`${item.sourceRange.startTicks}–${item.sourceRange.endTicks} тиков`}>
          {formatSeconds.format(Number(item.sourceRange.startTicks) / 254_016_000_000)}–{formatSeconds.format(Number(item.sourceRange.endTicks) / 254_016_000_000)} с
        </p>
      {/if}
      {#if item.progress !== null}<progress max="1" value={item.progress} aria-label="Ход экспорта"></progress>{/if}
      {#if item.status === 'encoding' && item.etaText}<p>Осталось: {item.etaText}</p>{/if}
      {#if item.statusText}<p>{item.statusText}</p>{/if}
      {#if item.error}<p role="alert">{item.error}</p>{/if}
      <button type="button" aria-label={`Поднять ${item.id}`} disabled={busy || index === 0 || item.status === 'encoding'} onclick={() => command({ action: 'move', id: item.id, by: -1 })}>↑</button>
      <button type="button" aria-label={`Опустить ${item.id}`} disabled={busy || index === queue.items.length - 1} onclick={() => command({ action: 'move', id: item.id, by: 1 })}>↓</button>
      {#if item.status === 'ready' || item.status === 'encoding'}
        <button type="button" disabled={busy} onclick={() => command({ action: 'cancel', id: item.id })}>Отменить</button>
      {:else}
        <button type="button" disabled={busy} onclick={() => command({ action: 'retry', id: item.id, start: true })}>Повторить</button>
      {/if}
      {#if item.status === 'done' && item.jobId && outputFormats.some(format => format === item.container)}
        <button type="button" disabled={!!downloading} onclick={() => download(item)}>
          {downloading === item.id ? 'Скачивание…' : `Скачать ${item.container.toUpperCase()}`}
        </button>
        {#if item.captionSidecarExtension}
          <button type="button" disabled={!!downloading} onclick={() => download(item, true)}>
            {downloading === `${item.id}:captions` ? 'Скачивание…' : `Скачать субтитры ${item.captionSidecarExtension.toUpperCase()}`}
          </button>
        {/if}
      {/if}
      <button type="button" disabled={busy || item.status === 'encoding'} onclick={() => command({ action: 'remove', id: item.id })}>Убрать</button>
    </article>
  {/each}
  {#if !queue.items.length}<p>Очередь пуста</p>{/if}
</section>

<style>
  fieldset { margin-block: 0.4rem; }
  fieldset label { display: inline-flex; gap: 0.3rem; align-items: center; margin-inline-end: 0.6rem; }
  .range-row { display: flex; flex-wrap: wrap; align-items: center; gap: 0.4rem; margin-block: 0.4rem; }
  .range-row input { width: 7rem; }
</style>
