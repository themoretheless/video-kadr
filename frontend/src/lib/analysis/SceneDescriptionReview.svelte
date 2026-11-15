<script lang="ts">
  import type { Snippet } from 'svelte'
  import { describeLibraryShot, type SceneDescription } from '../api.js'
  import type { ShotSpan } from './shots.js'
  import SceneModelSetup from './SceneModelSetup.svelte'

  let { sourceId, sourceHash, token, spaceId, disabled, children }: {
    sourceId: string
    sourceHash: string
    token?: string | null
    spaceId?: string | null
    disabled?: boolean
    children: Snippet<[Snippet<[ShotSpan, string]>]>
  } = $props()

  let focus = $state<'general' | 'shot-type'>('general')
  let language = $state<'ru' | 'en'>('ru')
  let descriptions = $state<Record<string, SceneDescription>>({})
  let pending = $state<string>()
  let setupOpen = $state(false)
  let failure = $state<{ key: string; text: string }>()
  const lifecycle = new AbortController()
  $effect(() => () => lifecycle.abort())

  function descriptionKey({ startTicks, endTicks }: ShotSpan): string {
    return `${startTicks}:${endTicks}:${focus}:${language}`
  }

  async function describe({ startTicks, endTicks }: ShotSpan) {
    if (pending || disabled || !token) return
    const span = { startTicks, endTicks }
    const key = descriptionKey(span)
    pending = key
    failure = undefined
    try {
      const description = await describeLibraryShot(sourceId, {
        span, expectedSourceHash: sourceHash, focus, language,
      }, token, spaceId, lifecycle.signal)
      if (!lifecycle.signal.aborted) descriptions[key] = description
    } catch (cause) {
      if (!lifecycle.signal.aborted) {
        failure = { key, text: cause instanceof Error ? cause.message : String(cause) }
      }
    } finally {
      if (!lifecycle.signal.aborted) pending = undefined
    }
  }
</script>

{#if token}
  <fieldset disabled={disabled || Boolean(pending)}>
    <legend>Описание сцен моделью</legend>
    <label>Язык
      <select value={language} onchange={event => language = event.currentTarget.value === 'en' ? 'en' : 'ru'}>
        <option value="ru">Русский</option><option value="en">English</option>
      </select>
    </label>
    <label>Акцент
      <select value={focus} onchange={event => focus = event.currentTarget.value === 'shot-type' ? 'shot-type' : 'general'}>
        <option value="general">Содержание сцены</option><option value="shot-type">Камера и план</option>
      </select>
    </label>
    <small>Запрос описания передаёт кадры настроенной на сервере модели.</small>
  </fieldset>
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
      <button class="btn ghost sm" type="button" disabled={disabled || Boolean(pending)} onclick={() => void describe(span)}>
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
