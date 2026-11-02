<script lang="ts">
  import type { NativeNestedProject } from '$lib/api'
  import { sourceCurveParticipants, validateSourceSpeedPoints } from '$lib/composition/concatSourceCurve'
  import { setNestedSourceSpeedCurve } from '$lib/state/nestedComposition.svelte'
  let { project, compositionId, clipIds, disabled = false }: { project: NativeNestedProject; compositionId: string; clipIds: readonly string[]; disabled?: boolean } = $props()
  let rows = $state<{ at: number; speed: number }[]>([])
  const participants = $derived.by(() => { try { return sourceCurveParticipants(project, compositionId, clipIds) } catch { return [] } })
  const original = $derived(JSON.stringify(participants[0]?.placement.concatRetime?.documentClock?.speedCurve ?? null))
  const selection = $derived(`${compositionId}/${clipIds.join('/')}/${original}`)
  const points = $derived(rows.map(row => ({ at: row.at / 100, speed: row.speed })))
  // Document positions are normalized; the form displays percentages.
  $effect(() => { void selection; const curve = JSON.parse(original) as { at: number; speed: number }[] | null; rows = curve?.map(point => ({ at: point.at * 100, speed: point.speed })) ?? [] })
  const invalid = $derived.by(() => { try { validateSourceSpeedPoints(points); return false } catch { return true } })
  function addPoint() {
    const last = rows.at(-1)
    rows = [...rows, { at: last ? Math.min(100, last.at + 25) : 0, speed: last?.speed ?? 1 }]
  }
</script>
{#if participants.length}
  <details>
    <summary>Кривая скорости</summary>
    <p>Изменение скорости сохраняет исходный объём материала и меняет длительность клипа.</p>
    {#each rows as row, index (index)}
      <div class="speed-point">
        <label>Позиция, % <input aria-label={`Позиция точки скорости ${index + 1}`} type="number" min="0" max="100" step="0.1" bind:value={row.at} {disabled} /></label>
        <label>Скорость, × <input aria-label={`Скорость точки ${index + 1}`} type="number" min="0.0625" max="16" step="0.0625" bind:value={row.speed} {disabled} /></label>
        <button class="btn ghost sm" type="button" {disabled} onclick={() => rows = rows.filter((_, i) => i !== index)}>Удалить точку {index + 1}</button>
      </div>
    {/each}
    <button class="btn ghost sm" type="button" disabled={disabled || rows.length >= 4096} onclick={addPoint}>Добавить точку скорости</button>
    <button class="btn ghost sm" type="button" disabled={disabled || invalid || !rows.length} onclick={() => void setNestedSourceSpeedCurve(compositionId, clipIds, points)}>Применить кривую скорости</button>
    <button class="btn ghost sm" type="button" {disabled} onclick={() => void setNestedSourceSpeedCurve(compositionId, clipIds, null)}>Убрать кривую скорости</button>
  </details>
{/if}
