<script lang="ts">
  import { listFilmcraftExportQueue, editFilmcraftExportQueue, type FilmcraftExportQueue, type FilmcraftExportQueueCommand, type FilmcraftVideoRequest } from '$lib/api'
  let { token, disabled = false, input }: { token: string | null; disabled?: boolean; input: () => FilmcraftVideoRequest } = $props()
  let queue = $state<FilmcraftExportQueue>({ running: false, items: [] })
  let busy = $state(false)
  let error = $state('')
  let revision = 0
  let controller: AbortController | null = null
  $effect(() => {
    const actor = token
    const current = new AbortController()
    controller = current
    queue = { running: false, items: [] }; error = ''; busy = false
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
    return () => { current.abort(); clearTimeout(timer); if (controller === current) controller = null }
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
  const labels = { ready: 'Ожидает', encoding: 'Экспортируется', done: 'Готово', failed: 'Ошибка', cancelled: 'Отменено' }
</script>

<section aria-label="Очередь экспорта Filmcraft">
  <h4>Очередь экспорта</h4>
  <button type="button" disabled={!token || busy || disabled} onclick={() => command({ action: 'add', input: input() })}>Добавить в очередь</button>
  <button type="button" disabled={!token || busy || queue.running || !queue.items.some(item => item.status === 'ready')} onclick={() => command({ action: 'start' })}>Запустить очередь</button>
  <button type="button" disabled={!token || busy || !queue.running} onclick={() => command({ action: 'stop' })}>Остановить после текущего</button>
  <button type="button" disabled={!token || busy || !queue.items.some(item => ['ready', 'encoding'].includes(item.status))} onclick={() => command({ action: 'cancel' })}>Отменить все</button>
  <button type="button" disabled={!token || busy || !queue.items.some(item => ['done', 'failed', 'cancelled'].includes(item.status))} onclick={() => command({ action: 'clear' })}>Убрать завершённые</button>
  {#if error}<p role="alert">{error}</p>{/if}
  {#each queue.items as item, index (item.id)}
    <article aria-label={`Экспорт ${item.id}`}>
      <p>{item.sequenceName} · {item.preset} · {labels[item.status]}</p>
      {#if item.progress !== null}<progress max="1" value={item.progress} aria-label="Ход экспорта"></progress>{/if}
      {#if item.status === 'encoding' && item.etaText}<p>Осталось: {item.etaText}</p>{/if}
      {#if item.statusText}<p>{item.statusText}</p>{/if}
      {#if item.error}<p role="alert">{item.error}</p>{/if}
      <button type="button" aria-label={`Поднять ${item.id}`} disabled={busy || index === 0} onclick={() => command({ action: 'move', id: item.id, by: -1 })}>↑</button>
      <button type="button" aria-label={`Опустить ${item.id}`} disabled={busy || index === queue.items.length - 1} onclick={() => command({ action: 'move', id: item.id, by: 1 })}>↓</button>
      {#if item.status === 'ready' || item.status === 'encoding'}
        <button type="button" disabled={busy} onclick={() => command({ action: 'cancel', id: item.id })}>Отменить</button>
      {:else}
        <button type="button" disabled={busy} onclick={() => command({ action: 'retry', id: item.id })}>Повторить</button>
      {/if}
      <button type="button" disabled={busy || item.status === 'encoding'} onclick={() => command({ action: 'remove', id: item.id })}>Убрать</button>
    </article>
  {/each}
  {#if !queue.items.length}<p>Очередь пуста</p>{/if}
</section>
