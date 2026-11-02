<script lang="ts">
  import { untrack } from 'svelte'
  import { startSpeechModelDownload, listSpeechModelDownloads, cancelSpeechModelDownload, retrySpeechModelDownload, type SpeechModelDownloadSummary } from '$lib/api'
  import type { Job } from '$lib/types'
  import { observeModelDownload } from '$lib/speech-model-download-client'
  import { getSpeechModelCatalog, type SpeechCatalogModel } from '$lib/api'
  let { token, onready, disabled = false }: { token: string; onready: () => void; disabled?: boolean } = $props()
  let models = $state<SpeechCatalogModel[]>([]), selected = $state('tiny-q5_1'), catalogError = $state(''), catalogBusy = $state(false)
  let catalogRequest: AbortController | undefined
  async function loadCatalog() {
    catalogRequest?.abort(); const request = new AbortController(); catalogRequest = request; catalogBusy = true; catalogError = ''
    try { const result = await getSpeechModelCatalog(request.signal); if (!request.signal.aborted) models = result.models }
    catch (reason) { if (!request.signal.aborted) catalogError = message(reason) }
    finally { if (!request.signal.aborted) catalogBusy = false }
  }
  $effect(() => () => catalogRequest?.abort())
  let jobId = $state(''), job = $state<Job | null>(null), busy = $state(false), cancelling = $state(false), error = $state('')
  let history = $state<SpeechModelDownloadSummary[]>([]), cursor = $state<string | null>(null), historyBusy = $state(false), historyError = $state('')
  const notified = new Set<string>()
  let observer: AbortController | undefined, historyRequest: AbortController | undefined, cancelRequest: AbortController | undefined
  const scope = $derived(token)
  const active = $derived(job?.status === 'pending' || job?.status === 'running')
  const retryable = $derived(job && ['error', 'interrupted'].includes(job.status) && ['timeout', 'tool_unavailable', 'process_exit', 'storage', 'interrupted'].includes(job.errorKind ?? ''))
  const progress = $derived(typeof job?.progress === 'number' && Number.isFinite(job.progress) ? Math.max(0, Math.min(100, job.progress)) : undefined)
  const labels: Record<Job['status'], string> = { pending: 'Ожидает', running: 'Скачивается', done: 'Готово', error: 'Ошибка', cancelled: 'Отменено', interrupted: 'Прервано' }
  function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason) }
  $effect(() => {
    const context = scope
    untrack(() => { observer?.abort(); cancelRequest?.abort(); jobId = ''; job = null; busy = false; cancelling = false; error = '' })
    return () => { void context; observer?.abort(); cancelRequest?.abort() }
  })
  $effect(() => {
    const owner = token
    untrack(() => { history = []; cursor = null; historyError = ''; historyBusy = false; if (owner) void loadHistory(false, owner) })
    return () => historyRequest?.abort()
  })
  async function loadHistory(more = false, owner = token) {
    if (!owner) return
    historyRequest?.abort(); const controller = new AbortController(); historyRequest = controller; historyBusy = true; historyError = ''
    try {
      const page = await listSpeechModelDownloads(owner, more ? cursor ?? undefined : undefined, controller.signal)
      if (controller.signal.aborted || owner !== token) return
      history = more ? [...history, ...page.jobs.filter(item => !history.some(old => old.jobId === item.jobId))] : page.jobs
      cursor = page.nextCursor
    } catch (reason) { if (!controller.signal.aborted && owner === token) historyError = message(reason) }
    finally { if (!controller.signal.aborted && owner === token) historyBusy = false }
  }
  async function watch(id: string, refresh: boolean, controller: AbortController, context: string, owner: string) {
    const result = await observeModelDownload(owner, id, controller.signal, value => { if (!controller.signal.aborted && context === scope) job = value })
    if (controller.signal.aborted || context !== scope) return
    job = result
    if (result.status === 'error' || result.status === 'interrupted') error = result.error || 'Загрузка остановлена'
    if (result.status === 'done' && refresh && !notified.has(id)) { notified.add(id); onready() }
    void loadHistory()
  }
  async function start() {
    if (!token || disabled || busy || active) return
    observer?.abort(); cancelRequest?.abort(); cancelling = false; const controller = new AbortController(); observer = controller
    const context = scope, owner = token, catalogId = selected
    busy = true; error = ''; job = null; jobId = ''
    try {
      const result = await startSpeechModelDownload(owner, catalogId, controller.signal)
      if (controller.signal.aborted || context !== scope) return
      if (!result.jobId) throw new Error('Сервер не вернул номер загрузки')
      jobId = result.jobId
      await watch(jobId, true, controller, context, owner)
    } catch (reason) { if (!controller.signal.aborted && context === scope) error = message(reason) }
    finally { if (!controller.signal.aborted && context === scope) busy = false }
  }
  async function reconnect(id = jobId) {
    if (!token || !id) return
    observer?.abort(); cancelRequest?.abort(); cancelling = false; const controller = new AbortController(); observer = controller
    const context = scope, owner = token; jobId = id; job = null; busy = true; error = ''
    try { await watch(id, true, controller, context, owner) }
    catch (reason) { if (!controller.signal.aborted && context === scope) error = message(reason) }
    finally { if (!controller.signal.aborted && context === scope) busy = false }
  }
  async function retry() {
    if (!token || disabled || busy || !jobId || !retryable) return
    observer?.abort(); cancelRequest?.abort(); cancelling = false
    const controller = new AbortController(); observer = controller
    const owner = token, context = scope, id = jobId
    busy = true; error = ''
    try {
      const admitted = await retrySpeechModelDownload(owner, id, controller.signal)
      if (controller.signal.aborted || context !== scope) return
      if (admitted.jobId !== id) throw new Error('Сервер вернул другое задание загрузки')
      job = null
      await watch(id, true, controller, context, owner)
    } catch (reason) { if (!controller.signal.aborted && context === scope) error = `Не удалось повторить загрузку: ${message(reason)}` }
    finally { if (!controller.signal.aborted && context === scope) busy = false }
  }
  async function cancel() {
    if (!token || !jobId || cancelling) return
    const context = scope, owner = token, id = jobId, controller = new AbortController(); cancelRequest = controller; cancelling = true; error = ''
    try { await cancelSpeechModelDownload(owner, id, controller.signal); if (!controller.signal.aborted && context === scope && id === jobId) { error = 'Отмена запрошена'; if (!busy) void reconnect(id) } }
    catch (reason) { if (!controller.signal.aborted && context === scope && id === jobId) error = `Не удалось отменить: ${message(reason)}` }
    finally { if (!controller.signal.aborted && context === scope && id === jobId) cancelling = false }
  }
</script>
<section aria-label="Скачивание моделей речи">
  <button type="button" disabled={catalogBusy || disabled} onclick={loadCatalog}>Загрузить каталог моделей</button>
  {#if catalogError}<small role="alert">{catalogError}</small>{/if}
  {#if models.length}<label>Модель для скачивания<select aria-label="Модель для скачивания" bind:value={selected} disabled={busy || disabled}>{#each models as model (model.id)}<option value={model.id}>{model.id} — {(model.bytes / 1048576).toFixed(1)} МБ</option>{/each}</select></label>{/if}
  <div class="actions"><button type="button" disabled={!token || disabled || !models.length || busy || active || (!!jobId && !job)} onclick={start}>Скачать модель</button>
    {#if jobId && (active || !job)}<button type="button" disabled={cancelling} onclick={cancel}>{cancelling ? 'Отмена…' : 'Отменить скачивание'}</button>{/if}
    {#if retryable}<button type="button" disabled={busy || disabled} onclick={retry}>Повторить скачивание</button>{/if}
    {#if jobId && !busy}<button type="button" onclick={() => reconnect()}>Обновить состояние</button>{/if}
  </div>
  <small>{token ? 'Можно закрыть панель: скачивание продолжится. После загрузки выберите модель в списке.' : 'Войдите, чтобы скачать модель.'}</small>
  {#if busy || job}<div role="status">{job ? labels[job.status] : 'Запуск скачивания…'}{progress !== undefined ? ` — ${Math.round(progress)}%` : ''}</div>{/if}
  {#if active}<progress max="100" value={progress} aria-label="Прогресс скачивания"></progress>{/if}
  {#if error}<small role="alert">{error}</small>{/if}
  {#if token}<details><summary>История скачивания</summary>
    <button type="button" disabled={historyBusy} onclick={() => loadHistory()}>Обновить историю</button>
    {#if historyError}<small role="alert">{historyError}</small>{/if}
    {#if !historyBusy && !history.length}<small>Заданий пока нет</small>{/if}
    <ul>{#each history as item (item.jobId)}<li><button type="button" onclick={() => reconnect(item.jobId)}>{item.catalogId} — {new Date(item.createdAt * 1000).toLocaleString('ru-RU')} — {labels[item.status]}{item.progress !== null ? ` (${Math.round(item.progress)}%)` : ''}</button></li>{/each}</ul>
    {#if cursor}<button type="button" disabled={historyBusy} onclick={() => loadHistory(true)}>Ещё задания</button>{/if}
  </details>{/if}
</section>
<style>
  section { display: flex; flex-direction: column; gap: 0.5rem; flex-basis: 100%; min-width: 0; }
  .actions { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  progress { width: 100%; }
  select { max-width: 100%; min-width: 0; }
  li, small { overflow-wrap: anywhere; }
  ul { padding-inline-start: 1rem; }
</style>
