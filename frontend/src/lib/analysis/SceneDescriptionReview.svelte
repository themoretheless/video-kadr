<script lang="ts">
  import type { Snippet } from 'svelte'
  import { describeLibraryShot, readLibraryShotDescriptions, type SceneDescription } from '../api.js'
  import type { ShotSpan } from './shots.js'
  import { parseSceneDescriptionLanguage, sceneDescriptionLanguages } from './sceneDescriptionLanguages.js'
  import SceneModelSetup from './SceneModelSetup.svelte'
  import { sceneDescriptionPreferences, setSceneDescriptionPreferences, type SceneDescriptionPreferences } from '../state/sceneDescriptionPreferences.svelte.js'

  let { sourceId, sourceHash, token, spaceId, disabled, batchSpans, wholeSpan, children }: {
    sourceId: string
    sourceHash: string
    token?: string | null
    spaceId?: string | null
    disabled?: boolean
    batchSpans: readonly ShotSpan[]
    wholeSpan?: ShotSpan
    children: Snippet<[Snippet<[ShotSpan, string]>]>
  } = $props()

  const { focus, language, fps } = $derived(sceneDescriptionPreferences)
  const samplingRates = [0.1, 0.25, 0.5, 1, 2, 4, 8, 15, 30]
  let descriptions = $state<Record<string, SceneDescription>>({})
  let pending = $state<string>()
  let setupOpen = $state(false)
  let failure = $state<{ key: string; text: string }>()
  let batch = $state<{ done: number; total: number; stop: boolean }>()
  let batchMessage = $state('')
  let readingCache = $state(false)
  const lifecycle = new AbortController()
  $effect(() => () => lifecycle.abort())
  const batchKey = $derived(batchSpans.map(({ startTicks, endTicks }) => `${startTicks}:${endTicks}`).join(','))
  const remaining = $derived(batchSpans.filter(span => !descriptions[descriptionKey(span)]))
  const working = $derived(Boolean(pending || batch))
  const cacheSpans = $derived.by(() => {
    const spans = batchSpans.map(({ startTicks, endTicks }) => ({ startTicks, endTicks }))
    const whole = wholeSpan
    if (whole && !spans.some(span => span.startTicks === whole.startTicks && span.endTicks === whole.endTicks)) spans.push(whole)
    return spans
  })

  $effect(() => {
    if (!token || !cacheSpans.length) {
      readingCache = false
      return
    }
    const controller = new AbortController()
    const owner = token
    const spans = cacheSpans
    const options = { expectedSourceHash: sourceHash, ...sceneDescriptionPreferences }
    const source = sourceId
    const space = spaceId
    readingCache = true
    void restore()
    return () => controller.abort()

    async function restore() {
      try {
        for (let index = 0; index < spans.length; index += 128) {
          const cached = await readLibraryShotDescriptions(source, {
            ...options, spans: spans.slice(index, index + 128),
          }, owner, space, controller.signal)
          if (controller.signal.aborted) return
          for (const { span, description } of cached) {
            descriptions[descriptionKey(span, options)] ??= description
          }
        }
      } catch {
        // Lookup is optional. An explicit description request reports service failures.
      } finally {
        if (!controller.signal.aborted) readingCache = false
      }
    }
  })

  function descriptionKey({ startTicks, endTicks }: ShotSpan, view = sceneDescriptionPreferences): string {
    return `${startTicks}:${endTicks}:${view.focus}:${view.language}:${view.fps}`
  }

  async function describe({ startTicks, endTicks }: ShotSpan, owner: string, view: SceneDescriptionPreferences): Promise<boolean> {
    const span = { startTicks, endTicks }
    const key = descriptionKey(span, view)
    pending = key
    failure = undefined
    try {
      const description = await describeLibraryShot(sourceId, {
        span, expectedSourceHash: sourceHash, ...view,
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
    if (working || disabled || readingCache || !token) return
    batchMessage = ''
    void describe(span, token, { ...sceneDescriptionPreferences })
  }

  async function describeBatch() {
    if (working || disabled || readingCache || !token || !remaining.length) return
    const spans = remaining
    const owner = token
    const initialKey = batchKey
    const view = { ...sceneDescriptionPreferences }
    batchMessage = ''
    batch = { done: 0, total: spans.length, stop: false }
    try {
      for (const span of spans) {
        if (!await describe(span, owner, view)) {
          if (!lifecycle.signal.aborted) batchMessage = 'Пакет остановлен из-за ошибки. Готовые описания сохранены.'
          break
        }
        batch.done++
        if (batchKey !== initialKey) {
          batchMessage = 'Список сцен изменился. Пакет остановлен.'
          break
        }
        const current = sceneDescriptionPreferences
        if (current.focus !== view.focus || current.language !== view.language || current.fps !== view.fps) {
          batchMessage = 'Настройки описания изменились. Пакет остановлен.'
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
      <select value={language} onchange={event => { setSceneDescriptionPreferences({ language: parseSceneDescriptionLanguage(event.currentTarget.value) ?? language }); batchMessage = '' }}>
        {#each Object.entries(sceneDescriptionLanguages) as [code, name] (code)}
          <option value={code} lang={code}>{name}</option>
        {/each}
      </select>
    </label>
    <label>Акцент
      <select value={focus} onchange={event => { setSceneDescriptionPreferences({ focus: event.currentTarget.value === 'shot-type' ? 'shot-type' : 'general' }); batchMessage = '' }}>
        <option value="general">Содержание сцены</option><option value="shot-type">Камера и план</option>
      </select>
    </label>
    <label>Кадров в секунду
      <select value={String(fps)} onchange={event => { setSceneDescriptionPreferences({ fps: Number(event.currentTarget.value) }); batchMessage = '' }}>
        {#if !samplingRates.includes(fps)}<option value={String(fps)}>{fps.toLocaleString('ru')}</option>{/if}
        {#each samplingRates as rate (rate)}
          <option value={String(rate)}>{rate.toLocaleString('ru')}</option>
        {/each}
      </select>
    </label>
    <small>Настроенная на сервере модель получает до восьми кадров из каждой сцены.</small>
  </fieldset>
  {#if readingCache}<small role="status">Восстанавливаю готовые описания…</small>{/if}
  {#if batch}
    <button class="btn ghost sm" type="button" disabled={batch.stop} onclick={() => { if (batch) batch.stop = true }}>
      Остановить после текущей сцены
    </button>
    <small role="status">Готово {batch.done} из {batch.total}{batch.stop ? ' · останавливаю…' : ''}</small>
  {:else if batchSpans.length > 1 && remaining.length}
    <button class="btn ghost sm" type="button" disabled={disabled || working || readingCache} onclick={() => void describeBatch()}>
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
      <button class="btn ghost sm" type="button" disabled={disabled || working || readingCache} onclick={() => describeOne(span)}>
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
