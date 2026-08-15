<script lang="ts">
  import {
    compositionRelinkCompatibility,
    compositionRelinkRequirements,
    type CompositionRelinkRequirements,
  } from '$lib/composition/relink.js'
  import {
    COMPOSITION_TIME_BASE,
    type Composition,
    type CompositionSource,
  } from '$lib/composition/types.js'

  export interface RelinkCandidate {
    readonly source: CompositionSource
    readonly label: string
  }

  interface Props {
    document: Composition
    missingSourceIds: readonly string[]
    candidates: readonly RelinkCandidate[]
    onrelink: (sourceId: string, replacement: CompositionSource) => void | Promise<void>
  }

  let { document, missingSourceIds, candidates, onrelink }: Props = $props()
  let selections = $state<Record<string, string>>({})
  let busySourceId = $state<string | null>(null)
  let message = $state('')
  let error = $state('')

  function requirementsFor(sourceId: string): CompositionRelinkRequirements {
    return compositionRelinkRequirements(document, sourceId)
  }

  function candidatesFor(sourceId: string): readonly RelinkCandidate[] {
    const requirements = requirementsFor(sourceId)
    return candidates.filter(
      (candidate) =>
        candidate.source.id !== sourceId &&
        compositionRelinkCompatibility(requirements, candidate.source).compatible,
    )
  }

  function durationLabel(requirements: CompositionRelinkRequirements): string {
    if (requirements.kind === 'image') return 'статичное изображение'
    const seconds = requirements.minimumDurationTicks / COMPOSITION_TIME_BASE
    return `минимум ${seconds.toFixed(3)} с${requirements.requiresAudio ? ' · с аудио' : ''}`
  }

  async function replace(sourceId: string): Promise<void> {
    if (busySourceId) return
    const replacementId = selections[sourceId]
    const replacement = candidatesFor(sourceId).find((candidate) => candidate.source.id === replacementId)
    if (!replacement) {
      error = 'Выберите совместимый файл из медиатеки.'
      message = ''
      return
    }
    busySourceId = sourceId
    error = ''
    message = ''
    try {
      await onrelink(sourceId, replacement.source)
      message = `Источник ${sourceId} заменён на «${replacement.label}».`
      delete selections[sourceId]
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught)
    } finally {
      busySourceId = null
    }
  }
</script>

{#if missingSourceIds.length}
  <section class="composition-relink card" aria-label="Восстановление потерянных медиа">
    <h3>Восстановить медиа</h3>
    <p>Замена меняет только source id. Монтаж, тайминг, эффекты и ключевые кадры сохраняются.</p>

    <div class="composition-relink-list">
      {#each missingSourceIds as sourceId (sourceId)}
        {@const requirements = requirementsFor(sourceId)}
        {@const compatible = candidatesFor(sourceId)}
        <fieldset disabled={busySourceId !== null}>
          <legend>{sourceId}</legend>
          <small>{requirements.kind} · {durationLabel(requirements)} · {requirements.referencedClipIds.length} clips</small>
          <label>
            Новый локальный файл
            <select
              aria-label={`Замена для ${sourceId}`}
              value={selections[sourceId] ?? ''}
              onchange={(event) => { selections[sourceId] = event.currentTarget.value }}
            >
              <option value="">Выберите совместимое медиа…</option>
              {#each compatible as candidate (candidate.source.id)}
                <option value={candidate.source.id}>{candidate.label}</option>
              {/each}
            </select>
          </label>
          {#if !compatible.length}
            <p class="composition-relink-empty">В медиатеке нет файла нужного типа, длительности и аудио.</p>
          {/if}
          <button
            class="btn ghost sm"
            type="button"
            disabled={!selections[sourceId] || !compatible.length}
            onclick={() => void replace(sourceId)}
          >
            {busySourceId === sourceId ? 'Проверяю…' : 'Перепривязать'}
          </button>
        </fieldset>
      {/each}
    </div>

    {#if message}<p role="status">{message}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </section>
{/if}

<style>
  .composition-relink {
    display: grid;
    gap: 0.75rem;
  }

  .composition-relink h3,
  .composition-relink p {
    margin: 0;
  }

  .composition-relink-list {
    display: grid;
    gap: 0.75rem;
  }

  fieldset {
    display: grid;
    gap: 0.55rem;
    min-width: 0;
    margin: 0;
    padding: 0.75rem;
    border: 1px solid var(--border);
    border-radius: 0.6rem;
  }

  fieldset label,
  fieldset select {
    min-width: 0;
    width: 100%;
  }

  .composition-relink-empty {
    color: var(--muted);
    font-size: 0.85rem;
  }

  fieldset .btn {
    justify-self: start;
  }
</style>
