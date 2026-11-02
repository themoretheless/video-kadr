<script lang="ts">
  import { compositionState, restyleCompositionCaptions } from '$lib/state/composition.svelte'
  import { nestedComposition, nestedHistory, restyleNestedProjectCaptions } from '$lib/state/nestedComposition.svelte'
  import type { TextStyle } from '$lib/composition/types'
  let selectedOnly = $state(false), useSize = $state(true), useColor = $state(false), useOutline = $state(false), useFont = $state(false)
  let size = $state(48), color = $state('#ffffff'), outline = $state(2)
  let font = $state<NonNullable<TextStyle['fontFamily']>>('Noto Sans')
  let error = $state(''), message = $state('')
  async function apply() {
    error = ''; message = ''
    const patch = {
      ...(useSize ? {font_size_px:size} : {}), ...(useFont ? {font_family:font} : {}),
      ...(useColor ? {color:{r:parseInt(color.slice(1,3),16),g:parseInt(color.slice(3,5),16),b:parseInt(color.slice(5,7),16),a:255}} : {}),
      ...(useOutline ? {outline_width:outline} : {}),
    }
    try {
      if (nestedComposition.document) {
        await restyleNestedProjectCaptions(patch)
        if (nestedHistory.error) throw new Error(nestedHistory.error)
      } else restyleCompositionCaptions(patch, selectedOnly ? compositionState.ui.selectedClipIds : undefined)
      message = 'Оформление субтитров применено'
    } catch (cause) { error = cause instanceof Error ? cause.message : String(cause) }
  }
</script>
<details>
  <summary>Оформление субтитров проекта</summary>
  {#if !nestedComposition.document}<label><input type="checkbox" bind:checked={selectedOnly} />Только выбранные реплики</label>{/if}
  <label><input type="checkbox" bind:checked={useSize} />Изменить размер</label>
  <label>Размер субтитров<input type="number" min="1" max="512" bind:value={size} disabled={!useSize} /></label>
  <label><input type="checkbox" bind:checked={useFont} />Изменить шрифт</label>
  <label>Шрифт субтитров<select bind:value={font} disabled={!useFont}>{#each ['Noto Sans','Arial Unicode MS','DejaVu Sans','Arial'] as name (name)}<option>{name}</option>{/each}</select></label>
  <label><input type="checkbox" bind:checked={useColor} />Изменить цвет</label>
  <label>Цвет субтитров<input type="color" bind:value={color} disabled={!useColor} /></label>
  <label><input type="checkbox" bind:checked={useOutline} />Изменить контур</label>
  <label>Контур субтитров<input type="number" min="0" max="100" bind:value={outline} disabled={!useOutline} /></label>
  <button type="button" class="btn ghost sm" disabled={nestedHistory.busy} onclick={() => void apply()}>Применить оформление субтитров</button>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if message}<p role="status">{message}</p>{/if}
</details>
