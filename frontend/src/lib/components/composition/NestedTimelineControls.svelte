<script lang="ts">
  import NestedTimelineVideoSettings from './NestedTimelineVideoSettings.svelte'
  import { nestedComposition, nestedHistory, navigateNestedComposition, moveNestedTimeline, renameNestedTimeline, addNestedTimeline, removeNestedTimeline } from '$lib/state/nestedComposition.svelte'
  import { orderedTimelineIds, timelineReferences } from '$lib/composition/timelineOrder'
  let { disabled = false }: { disabled?: boolean } = $props()
  let removePlacements = $state(false)
  const document = $derived(nestedComposition.document)
  const ids = $derived(document ? orderedTimelineIds(document) : [])
  const active = $derived(nestedComposition.activeCompositionId ?? document?.root ?? '')
  const references = $derived(document ? timelineReferences(document, active) : [])
  $effect(() => { if (active) removePlacements = false })
  const index = $derived(ids.indexOf(active))
  const locked = $derived(disabled || nestedHistory.busy)
</script>
{#if document}
  <fieldset class="composition-fieldset" disabled={locked}>
    <legend>Таймлайны</legend>
    <button class="btn ghost sm" type="button" disabled={locked || ids.length >= 1024} onclick={() => void addNestedTimeline()}>Добавить таймлайн</button>
    {#if references.length}
      <label><input aria-label="Удалить также размещения таймлайна" type="checkbox" checked={removePlacements} onchange={(event) => { removePlacements = event.currentTarget.checked }} /> Удалить также размещения этого таймлайна ({references.length})</label>
    {/if}
    <button class="btn ghost sm" type="button" disabled={locked || ids.length <= 1 || references.length > 0 && !removePlacements} title={ids.length <= 1 ? 'Нельзя удалить последний таймлайн' : references.length && !removePlacements ? 'Разрешите удаление размещений таймлайна' : undefined} onclick={() => void removeNestedTimeline(active, removePlacements)}>Удалить таймлайн</button>
    <label>Композиция
      <select aria-label="Активная вложенная композиция" value={active} onchange={(event) => navigateNestedComposition(event.currentTarget.value)}>
        {#each ids as id (id)}<option value={id}>{document.compositions[id]?.content.metadata?.sequenceName || id}</option>{/each}
      </select>
    </label>
    <label>Имя таймлайна
      <input aria-label="Имя таймлайна" maxlength="128" value={document.compositions[active]?.content.metadata?.sequenceName ?? active} onchange={(event) => void renameNestedTimeline(active, event.currentTarget.value)} />
    </label>
    <button class="btn ghost sm" type="button" aria-label="Переместить таймлайн раньше" disabled={locked || index <= 0} onclick={() => void moveNestedTimeline(active, index - 1)}>Раньше</button>
    <button class="btn ghost sm" type="button" aria-label="Переместить таймлайн позже" disabled={locked || index < 0 || index >= ids.length - 1} onclick={() => void moveNestedTimeline(active, index + 1)}>Позже</button>
  </fieldset>
  <NestedTimelineVideoSettings {disabled} />
{/if}
