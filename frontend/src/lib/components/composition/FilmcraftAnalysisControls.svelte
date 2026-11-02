<script lang="ts">
  import { untrack } from 'svelte'
  import { startFilmcraftAnalysis, listFilmcraftAnalysis, cancelFilmcraftAnalysis, type FilmcraftPreviewInput, type FilmcraftAnalysisJobSummary } from '$lib/api'
  import type { Job } from '$lib/types'
  import { observeAnalysis } from '$lib/filmcraft/analysis-client'
  let { input, token, onready }: { input: FilmcraftPreviewInput; token: string | null; onready: () => void } = $props()
  let jobId = $state(''), job = $state<Job | null>(null), busy = $state(false), cancelling = $state(false), error = $state('')
  let history = $state<FilmcraftAnalysisJobSummary[]>([]), cursor = $state<string | null>(null), historyBusy = $state(false), historyError = $state('')
  let submittedId = ''
  const notified = new Set<string>()
  let observer: AbortController | undefined, historyRequest: AbortController | undefined, cancelRequest: AbortController | undefined
  const scope = $derived(JSON.stringify([input.document, input.sequenceId, input.sources, input.effects, input.captions, token]))
  const active = $derived(job?.status === 'pending' || job?.status === 'running')
  const progress = $derived(typeof job?.progress === 'number' && Number.isFinite(job.progress) ? Math.max(0, Math.min(100, job.progress)) : undefined)
  const labels: Record<Job['status'], string> = { pending: 'Ожидает', running: 'Анализируется', done: 'Готово', error: 'Ошибка', cancelled: 'Отменено', interrupted: 'Прервано' }
  function message(reason: unknown) { return reason instanceof Error ? reason.message : String(reason) }
  $effect(() => {
    const context = scope
    untrack(() => { observer?.abort(); cancelRequest?.abort(); jobId = ''; submittedId = ''; job = null; busy = false; cancelling = false; error = '' })
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
      const page = await listFilmcraftAnalysis(owner, more ? cursor ?? undefined : undefined, controller.signal)
      if (controller.signal.aborted || owner !== token) return
      history = more ? [...history, ...page.jobs.filter(item => !history.some(old => old.jobId === item.jobId))] : page.jobs
      cursor = page.nextCursor
    } catch (reason) { if (!controller.signal.aborted && owner === token) historyError = message(reason) }
    finally { if (!controller.signal.aborted && owner === token) historyBusy = false }
  }
  async function watch(id: string, refresh: boolean, controller: AbortController, context: string, owner: string) {
    const result = await observeAnalysis(owner, id, controller.signal, value => { if (!controller.signal.aborted && context === scope) job = value })
    if (controller.signal.aborted || context !== scope) return
    if (result.status === 'error' || result.status === 'interrupted') error = result.error || 'Анализ остановлен'
    const ready = result.result
    if (result.status === 'done' && refresh && ready && typeof ready === 'object' && 'ready' in ready && ready.ready === true && !notified.has(id)) { notified.add(id); onready() }
    void loadHistory()
  }
  async function start() {
    if (!token || busy || active) return
    observer?.abort(); const controller = new AbortController(); observer = controller
    const context = scope, owner = token, snapshot = structuredClone($state.snapshot(input))
    busy = true; error = ''; job = null; jobId = ''
    try {
      const result = await startFilmcraftAnalysis(owner, snapshot, controller.signal)
      if (controller.signal.aborted || context !== scope) return
      if (!result.jobId) throw new Error('Сервер не вернул номер задания анализа')
      jobId = result.jobId; submittedId = result.jobId
      await watch(jobId, true, controller, context, owner)
    } catch (reason) { if (!controller.signal.aborted && context === scope) error = message(reason) }
    finally { if (!controller.signal.aborted && context === scope) busy = false }
  }
  async function reconnect(id = jobId) {
    if (!token || !id) return
    observer?.abort(); const controller = new AbortController(); observer = controller
    const context = scope, owner = token; jobId = id; job = null; busy = true; error = ''
    try { await watch(id, id === submittedId, controller, context, owner) }
    catch (reason) { if (!controller.signal.aborted && context === scope) error = message(reason) }
    finally { if (!controller.signal.aborted && context === scope) busy = false }
  }
  async function cancel() {
    if (!token || !jobId || cancelling) return
    const context = scope, owner = token, id = jobId, controller = new AbortController(); cancelRequest = controller; cancelling = true; error = ''
    try { await cancelFilmcraftAnalysis(owner, id, controller.signal); if (!controller.signal.aborted && context === scope) { error = 'Отмена запрошена'; if (!busy) void reconnect(id) } }
    catch (reason) { if (!controller.signal.aborted && context === scope) error = `Не удалось отменить: ${message(reason)}` }
    finally { if (!controller.signal.aborted && context === scope) cancelling = false }
  }
</script>
<section aria-label="Фоновый анализ композиции">
  <div class="actions"><button type="button" disabled={!token || busy || active || (!!jobId && !job)} onclick={start}>Подготовить эффекты в фоне</button>
    {#if jobId && (active || (!job && busy))}<button type="button" disabled={cancelling} onclick={cancel}>{cancelling ? 'Отмена…' : 'Отменить анализ'}</button>{/if}
    {#if jobId && !busy}<button type="button" onclick={() => reconnect()}>Обновить состояние</button>{/if}
  </div>
  <small>{token ? 'Можно закрыть предпросмотр: анализ продолжится.' : 'Войдите, чтобы запустить фоновый анализ.'}</small>
  {#if busy || job}<div role="status">{job ? labels[job.status] : 'Запуск анализа…'}{progress !== undefined ? ` — ${Math.round(progress)}%` : ''}</div>{/if}
  {#if active}<progress max="100" value={progress} aria-label="Прогресс анализа"></progress>{/if}
  {#if error}<small role="alert">{error}</small>{/if}
  {#if token}<details><summary>История анализа</summary>
    <button type="button" disabled={historyBusy} onclick={() => loadHistory()}>Обновить историю</button>
    {#if historyError}<small role="alert">{historyError}</small>{/if}
    {#if !historyBusy && !history.length}<small>Заданий пока нет</small>{/if}
    <ul>{#each history as item (item.jobId)}<li><button type="button" onclick={() => reconnect(item.jobId)}>{new Date(item.createdAt * 1000).toLocaleString('ru-RU')} — {labels[item.status]}{item.progress !== null ? ` (${Math.round(item.progress)}%)` : ''}</button></li>{/each}</ul>
    {#if cursor}<button type="button" disabled={historyBusy} onclick={() => loadHistory(true)}>Ещё задания</button>{/if}
  </details>{/if}
</section>
<style>
  section { display: flex; flex-direction: column; gap: 0.5rem; flex-basis: 100%; min-width: 0; }
  .actions { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  progress { width: 100%; }
  ul { padding-inline-start: 1rem; }
</style>
