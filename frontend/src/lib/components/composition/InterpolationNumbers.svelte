<script lang="ts">
  import type { CompositionInterpolation } from '$lib/composition/types'
  let { value,label,onChange }: { value: CompositionInterpolation;label: string;onChange: (value: CompositionInterpolation) => void } = $props()
  function bezier(control: 'p1' | 'p2',axis: 0 | 1,number: number) {
    if (typeof value !== 'object' || value.kind !== 'bezier') return
    const point: [number,number] = [...value[control]]
    point[axis] = number
    onChange({ ...value,[control]: point })
  }
  function elastic(field: 'amplitude' | 'period',number: number) {
    if (typeof value === 'object' && value.kind === 'elastic') onChange({ ...value,[field]: number })
  }
</script>
{#if typeof value === 'object' && value.kind === 'bezier'}
  <div class="composition-form-grid">
    {#each ['p1','p2'] as point (point)}
      {#each [0,1] as axis (axis)}
        <label>{point}{axis === 0 ? 'X' : 'Y'}<input aria-label={`${label} ${point}${axis === 0 ? 'X' : 'Y'}`} type="number" step="0.01" min={axis === 0 ? 0 : undefined} max={axis === 0 ? 1 : undefined} value={value[point as 'p1' | 'p2'][axis]} onchange={event => bezier(point as 'p1' | 'p2',axis as 0 | 1,Number(event.currentTarget.value))} /></label>
      {/each}
    {/each}
  </div>
{:else if typeof value === 'object' && value.kind === 'elastic'}
  <div class="composition-form-grid">
    <label>Амплитуда<input aria-label={`${label} амплитуда`} type="number" min="1" step="0.1" value={value.amplitude} onchange={event => elastic('amplitude',Number(event.currentTarget.value))} /></label>
    <label>Период<input aria-label={`${label} период`} type="number" min="0.01" step="0.01" value={value.period} onchange={event => elastic('period',Number(event.currentTarget.value))} /></label>
  </div>
{/if}
