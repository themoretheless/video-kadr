<script lang="ts">
  import { findFilmcraftTimeline, type FilmcraftFindQuery, type FilmcraftFindRow, type FilmcraftTimelineFindResult, type FilmcraftTimelineFindScope } from '$lib/api'

  let { document, sequence, token, disabled, onhit }: {
    document: string; sequence: string; token: string | null; disabled: boolean
    onhit: (result: FilmcraftTimelineFindResult) => void
  } = $props()
  const operators = [
    ['contains', 'Содержит'], ['matches', 'Совпадает'], ['beginsWith', 'Начинается с'],
    ['endsWith', 'Заканчивается на'], ['doesNotContain', 'Не содержит'],
  ] as const
  const emptyRow = (): FilmcraftFindRow => ({ column: 'Name', operator: 'contains', text: '' })
  let rows = $state<FilmcraftFindRow[]>([emptyRow()])
  let within = $state<FilmcraftTimelineFindScope>('all')
  let matchAll = $state(true)
  let caseSensitive = $state(false)
  let pending = $state(false)
  let error = $state('')
  let last = $state<{ document: string; sequence: string; input: FilmcraftFindQuery; within: FilmcraftTimelineFindScope; result: FilmcraftTimelineFindResult }>()
  const current = $derived(last?.document === document && last.sequence === sequence ? last : undefined)
  let request: AbortController | undefined
  $effect(() => () => request?.abort())

  async function find(next: boolean) {
    const saved = next ? current : undefined
    if (!token || !sequence || disabled || pending || (next && !saved)) return
    const owner = token
    const source = document
    const target = sequence
    const input = saved?.input ?? { rows: rows.map(row => ({ ...row })), matchAll, caseSensitive }
    const scope = saved?.within ?? within
    const controller = new AbortController()
    request = controller
    pending = true
    error = ''
    try {
      const result = await findFilmcraftTimeline(owner, {
        ...input, document: source, sequence: target, in: scope, index: saved?.result.index,
      }, controller.signal)
      if (controller.signal.aborted || document !== source || sequence !== target || token !== owner || result.sequence !== target) return
      last = { document: source, sequence: target, input, within: scope, result }
      if (result.matches) onhit(result)
    } catch (failure) {
      if (!controller.signal.aborted && document === source && sequence === target) error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      if (request === controller) { request = undefined; pending = false }
    }
  }
</script>

<details>
  <summary>Поиск на таймлайне Filmcraft</summary>
  <form onsubmit={event => { event.preventDefault(); void find(false) }}>
    <div class="row">
      <label>Искать на таймлайне
        <select aria-label="Искать на таймлайне" bind:value={within}>
          <option value="all">Клипы и маркеры</option>
          <option value="clips">Клипы</option>
          <option value="markers">Маркеры</option>
        </select>
      </label>
      <label><input type="checkbox" bind:checked={matchAll} />Все условия таймлайна</label>
      <label><input type="checkbox" bind:checked={caseSensitive} />Учитывать регистр на таймлайне</label>
    </div>
    {#each rows as row, index (row)}
      <div class="row">
        <label>Условие
          <select aria-label={`Условие таймлайна ${index + 1}`} bind:value={row.operator}>
            {#each operators as [key, label] (key)}<option value={key}>{label}</option>{/each}
          </select>
        </label>
        <label>Текст <input aria-label={`Текст на таймлайне ${index + 1}`} bind:value={row.text} /></label>
        {#if rows.length > 1}<button type="button" aria-label={`Удалить условие таймлайна ${index + 1}`} onclick={() => { rows = rows.filter(candidate => candidate !== row) }}>×</button>{/if}
      </div>
    {/each}
    <div class="row">
      <button type="button" onclick={() => { rows = [...rows, emptyRow()] }}>Добавить условие таймлайна</button>
      <button type="submit" disabled={disabled || pending || !token || !sequence || !rows.some(row => row.text)}>Найти на таймлайне</button>
      <button type="button" disabled={disabled || pending || !token || !current?.result.matches} onclick={() => find(true)}>Далее на таймлайне</button>
    </div>
  </form>
  {#if current}
    {@const result = current.result}
    <p role="status">{result.matches ? `${result.index + 1} из ${result.matches} · ${result.clip ? 'Клип' : 'Маркер'} ${result.name || ''} · ${result.timecode}` : 'Совпадений на таймлайне нет'}</p>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</details>

<style>
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-block: 0.5rem; }
  summary { cursor: pointer; }
</style>
