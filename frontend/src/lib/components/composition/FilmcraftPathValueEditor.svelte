<script lang="ts">
  import type { FilmcraftPath } from '$lib/api'
  let { path = $bindable() }: { path: FilmcraftPath } = $props()
  function add() {
    if (path.vertices.length >= 4096) return
    path = { ...path, vertices: [...path.vertices, { p: { x: 0, y: 0 } }] }
  }
  function remove(index: number) { path = { ...path, vertices: path.vertices.filter((_, i) => i !== index) } }
  function tangent(index: number, side: 't_in' | 't_out') {
    const vertex = path.vertices[index]
    if (!vertex) return
    path = { ...path, vertices: path.vertices.map((v, i) => i === index ? { ...v, [side]: { x: 0, y: 0 } } : v) }
  }
</script>
<label>Замкнутый путь <input aria-label="Замкнутый путь Filmcraft" type="checkbox" checked={path.closed !== false} onchange={event => path = { ...path, closed: event.currentTarget.checked }} /></label>
{#each path.vertices as vertex, index (index)}
  <fieldset>
    <legend>Вершина {index + 1}</legend>
    <label>X <input aria-label={`Вершина ${index + 1} X Filmcraft`} type="number" step="any" bind:value={vertex.p.x} /></label>
    <label>Y <input aria-label={`Вершина ${index + 1} Y Filmcraft`} type="number" step="any" bind:value={vertex.p.y} /></label>
    {#each ['t_in', 't_out'] as field (field)}
      {@const side = field as 't_in' | 't_out'}
      {#if vertex[side]}
        <label>{side} X <input aria-label={`Вершина ${index + 1} ${side} X Filmcraft`} type="number" step="any" bind:value={vertex[side]!.x} /></label>
        <label>{side} Y <input aria-label={`Вершина ${index + 1} ${side} Y Filmcraft`} type="number" step="any" bind:value={vertex[side]!.y} /></label>
      {:else}<button onclick={() => tangent(index, side)}>Добавить {side} вершины {index + 1}</button>{/if}
    {/each}
    <button onclick={() => remove(index)}>Удалить вершину {index + 1}</button>
  </fieldset>
{/each}
<button disabled={path.vertices.length >= 4096} onclick={add}>Добавить вершину Filmcraft</button>
<style>
  fieldset { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; }
  label { display: inline-flex; align-items: center; gap: 0.25rem; white-space: nowrap; }
  input[type='number'] { width: 7rem; }
</style>
