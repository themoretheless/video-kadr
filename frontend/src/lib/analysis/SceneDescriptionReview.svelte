<script lang="ts">
  import type { Snippet } from 'svelte'
  import { describeLibraryShot, type SceneDescription } from '../api.js'
  import type { ShotSpan } from './shots.js'
  import SceneModelSetup from './SceneModelSetup.svelte'

  let { sourceId, sourceHash, token, spaceId, disabled, batchSpans, children }: {
    sourceId: string
    sourceHash: string
    token?: string | null
    spaceId?: string | null
    disabled?: boolean
    batchSpans: readonly ShotSpan[]
    children: Snippet<[Snippet<[ShotSpan, string]>]>
  } = $props()

  let focus = $state<'general' | 'shot-type'>('general')
  let language = $state<'ru' | 'en'>('ru')
  let descriptions = $state<Record<string, SceneDescription>>({})
  let pending = $state<string>()
  let setupOpen = $state(false)
  let failure = $state<{ key: string; text: string }>()
  let batch = $state<{ done: number; total: number; stop: boolean }>()
  let batchMessage = $state('')
  const lifecycle = new AbortController()
  $effect(() => () => lifecycle.abort())
  const batchKey = $derived(batchSpans.map(({ startTicks, endTicks }) => `${startTicks}:${endTicks}`).join(','))
  const remaining = $derived(batchSpans.filter(span => !descriptions[descriptionKey(span)]))
  const working = $derived(Boolean(pending || batch))

  function descriptionKey({ startTicks, endTicks }: ShotSpan): string {
    return `${startTicks}:${endTicks}:${focus}:${language}`
  }

  async function describe({ startTicks, endTicks }: ShotSpan, owner: string): Promise<boolean> {
    const span = { startTicks, endTicks }
    const key = descriptionKey(span)
    pending = key
    failure = undefined
    try {
      const description = await describeLibraryShot(sourceId, {
        span, expectedSourceHash: sourceHash, focus, language,
      }, owner, spaceId, lifecycle.signal)
      if (lifecycle.signal.aborted) return false
      descriptions[key] = description
      return true
    } catch (cause) {
      if (!lifecycle.signal.aborted) {
        failure = { key, text: cause instanceof Error ? cause.message : String(cause) }
      }
      return false
    } finally {
      if (!lifecycle.signal.aborted) pending = undefined
    }
  }

  function describeOne(span: ShotSpan) {
    if (working || disabled || !token) return
    batchMessage = ''
    void describe(span, token)
  }

  async function describeBatch() {
    if (working || disabled || !token || !remaining.length) return
    const spans = remaining
    const owner = token
    const initialKey = batchKey
    batchMessage = ''
    batch = { done: 0, total: spans.length, stop: false }
    try {
      for (const span of spans) {
        if (!await describe(span, owner)) {
          if (!lifecycle.signal.aborted) batchMessage = 'Пакет остановлен из-за ошибки. Готовые описания сохранены.'
          break
        }
        batch.done++
        if (batchKey !== initialKey) {
          batchMessage = 'Список сцен изменился. Пакет остановлен.'
          break
        }
        if (batch.stop) {
          batchMessage = `Пакет остановлен. Готово ${batch.done} из ${batch.total}.`
          break
        }
      }
    } finally {
      if (!lifecycle.signal.aborted) batch = undefined
    }
  }
</script>

{#if token}
  <fieldset disabled={disabled || working}>
    <legend>Описание сцен моделью</legend>
    <label>Язык
      <select value={language} onchange={event => { language = event.currentTarget.value === 'en' ? 'en' : 'ru'; batchMessage = '' }}>
        <option value="ru">Русский</option><option value="en">English</option>
      </select>
    </label>
    <label>Акцент
      <select value={focus} onchange={event => { focus = event.currentTarget.value === 'shot-type' ? 'shot-type' : 'general'; batchMessage = '' }}>
        <option value="general">Содержание сцены</option><option value="shot-type">Камера и план</option>
      </select>
    </label>
    <small>Запрос описания передаёт кадры настроенной на сервере модели.</small>
  </fieldset>
  {#if batch}
    <button class="btn ghost sm" type="button" disabled={batch.stop} onclick={() => { if (batch) batch.stop = true }}>
      Остановить после текущей сцены
    </button>
    <small role="status">Готово {batch.done} из {batch.total}{batch.stop ? ' · останавливаю…' : ''}</small>
  {:else if batchSpans.length > 1 && remaining.length}
    <button class="btn ghost sm" type="button" disabled={disabled || working} onclick={() => void describeBatch()}>
      Описать все сцены ({remaining.length})
    </button>
  {/if}
  {#if batchMessage}
    <small role="status">{batchMessage}</small>
  {:else if !batch && batchSpans.length > 1 && !remaining.length}
    <small role="status">Все сцены описаны</small>
  {/if}
  <details ontoggle={event => setupOpen = event.currentTarget.open}>
    <summary>Локальная модель</summary>
    {#if setupOpen}<SceneModelSetup {token} />{/if}
  </details>
{/if}

{#snippet action(span: ShotSpan, label: string)}
  {#if token}
    {@const key = descriptionKey(span)}
    {@const description = descriptions[key]}
    {#if description}
      <p>{description.text}</p>
      {#if description.tags.length}<small>{description.tags.join(' · ')}</small>{/if}
    {:else}
      <button class="btn ghost sm" type="button" disabled={disabled || working} onclick={() => describeOne(span)}>
        {label}
      </button>
      {#if pending === key}<small role="status">Модель описывает кадры…</small>{/if}
      {#if failure?.key === key}<small role="alert">{failure.text}</small>{/if}
    {/if}
  {/if}
{/snippet}

{@render children(action)}

<style>
  fieldset { margin-block: 0.5rem; }
  label { margin-inline-end: 0.5rem; }
  small { display: block; color: var(--muted); }
</style>
