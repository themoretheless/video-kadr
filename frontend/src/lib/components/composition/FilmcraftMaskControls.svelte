<script lang="ts">
  import FilmcraftPathValueEditor from './FilmcraftPathValueEditor.svelte'
  import type { FilmcraftPath, FilmcraftMaskInfo, FilmcraftMaskCommand, FilmcraftMaskMode, FilmcraftMaskTrackMethod } from '$lib/api'
  let { masks, maskIndex, disabled = false, maskable = true, oncommand }: { masks: FilmcraftMaskInfo[]; maskIndex: number; disabled?: boolean; maskable?: boolean; oncommand: (command: FilmcraftMaskCommand) => void } = $props()
  let shape = $state<'rectangle' | 'ellipse' | 'freeDraw' | 'path'>('rectangle')
  let newName = $state('Маска')
  let customPath = $state<FilmcraftPath>({ closed: true, vertices: [{ p: { x: 0, y: 0 } }, { p: { x: 100, y: 0 } }, { p: { x: 0, y: 100 } }] })
  let x0 = $state('0'), y0 = $state('0'), x1 = $state('100'), y1 = $state('100')
  let name = $state('')
  let mode = $state<FilmcraftMaskMode>('Add')
  let inverted = $state(false)
  let trackMethod = $state<FilmcraftMaskTrackMethod>('PositionScaleRotation')
  let error = $state('')
  const selected = $derived(masks.find(mask => mask.index === maskIndex))
  $effect(() => { const mask = selected; name = mask?.name ?? ''; mode = mask?.mode ?? 'Add'; inverted = mask?.inverted ?? false; trackMethod = mask?.trackMethod ?? 'PositionScaleRotation'; error = '' })
  function create() {
    if (disabled) return
    error = ''
    if (new TextEncoder().encode(newName).length > 4096) { error = 'Имя маски превышает 4096 байт'; return }
    if (shape === 'freeDraw') { oncommand({ kind: 'create', name: newName, shape: { kind: 'freeDraw' } }); return }
    if (shape === 'path') {
      if (customPath.closed !== false && customPath.vertices.length < 3) { error = 'Замкнутая маска требует минимум три вершины'; return }
      oncommand({ kind: 'create', name: newName, shape: { kind: 'path', path: $state.snapshot(customPath) } }); return
    }
    const values = [x0, y0, x1, y1].map(Number)
    if ([x0, y0, x1, y1].some(s => !s.trim()) || values.some(n => !Number.isFinite(n)) || values[2]! <= values[0]! || values[3]! <= values[1]!) { error = 'Границы маски должны быть конечными и возрастающими'; return }
    oncommand({ kind: 'create', name: newName, shape: { kind: shape, x0: values[0]!, y0: values[1]!, x1: values[2]!, y1: values[3]! } })
  }
  function settings() {
    if (disabled || !selected) return
    if (new TextEncoder().encode(name).length > 4096) { error = 'Имя маски превышает 4096 байт'; return }
    error = ''
    oncommand({ kind: 'settings', maskIndex, name, mode, inverted, trackMethod })
  }
</script>
<details>
  <summary>Инструменты масок Filmcraft</summary>
  <fieldset {disabled}>
    <legend>Маски</legend>
    {#if maskable}
    <label>Имя <input aria-label="Имя новой маски Filmcraft" bind:value={newName} /></label>
    <label>Форма <select aria-label="Форма новой маски Filmcraft" value={shape} onchange={event => shape = event.currentTarget.value as typeof shape}><option value="rectangle">Прямоугольник</option><option value="ellipse">Эллипс</option><option value="freeDraw">Свободный контур</option><option value="path">Заданный Bézier-контур</option></select></label>
    {#if shape === 'path'}
      <FilmcraftPathValueEditor bind:path={customPath} />
    {:else if shape !== 'freeDraw'}
      <label>X0 <input aria-label="Граница маски X0 Filmcraft" bind:value={x0} /></label><label>Y0 <input aria-label="Граница маски Y0 Filmcraft" bind:value={y0} /></label>
      <label>X1 <input aria-label="Граница маски X1 Filmcraft" bind:value={x1} /></label><label>Y1 <input aria-label="Граница маски Y1 Filmcraft" bind:value={y1} /></label>
      <p>Границы в пикселях исходного клипа.</p>
    {/if}
    <button disabled={disabled || masks.length >= 1024} onclick={create}>Создать маску Filmcraft</button>
    {:else}<p>Этот эффект не поддерживает создание масок.</p>{/if}
    {#if selected}
      <label>Имя маски <input aria-label="Имя маски Filmcraft" bind:value={name} /></label>
      <label>Режим <select aria-label="Режим маски Filmcraft" value={mode} onchange={event => mode = event.currentTarget.value as FilmcraftMaskMode}>{#each ['None', 'Add', 'Subtract', 'Intersect', 'Lighten', 'Darken', 'Difference'] as option (option)}<option value={option}>{option}</option>{/each}</select></label>
      <label>Инверсия <input aria-label="Инверсия маски Filmcraft" type="checkbox" bind:checked={inverted} /></label>
      <label>Метод трекинга <select aria-label="Метод трекинга маски Filmcraft" value={trackMethod} onchange={event => trackMethod = event.currentTarget.value as FilmcraftMaskTrackMethod}>{#each ['Position', 'PositionRotation', 'PositionScaleRotation'] as method (method)}<option value={method}>{method}</option>{/each}</select></label>
      <button onclick={settings}>Сохранить настройки маски Filmcraft</button>
      <button disabled={disabled || maskIndex <= 0} onclick={() => oncommand({ kind: 'move', maskIndex, destination: maskIndex - 1 })}>Маску Filmcraft выше</button>
      <button disabled={disabled || maskIndex >= masks.length - 1} onclick={() => oncommand({ kind: 'move', maskIndex, destination: maskIndex + 1 })}>Маску Filmcraft ниже</button>
      <button onclick={() => oncommand({ kind: 'delete', maskIndex })}>Удалить маску Filmcraft</button>
    {/if}
  </fieldset>
  {#if error}<p role="alert">{error}</p>{/if}
</details>
