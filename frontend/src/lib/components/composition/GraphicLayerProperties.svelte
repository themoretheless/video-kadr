<script lang="ts">
  import type { GraphicContent } from '$lib/composition/types'
  import { graphicShapeChannels, graphicTextChannels, graphicShapeFlags, graphicTextFlags, graphicShapeChoices, graphicTextChoices, graphicNumericValue, setGraphicNumeric } from '$lib/composition/graphicLayerProperties'
  import GraphicCharacterFormatting from './GraphicCharacterFormatting.svelte'
  let { content, label, width, height, onchange }: { content: GraphicContent; label: string; width: number; height: number; onchange: (content: GraphicContent) => void } = $props()
  const channels = $derived(content.kind === 'text' ? graphicTextChannels : graphicShapeChannels)
  const flags = $derived(content.kind === 'text' ? graphicTextFlags : graphicShapeFlags)
  const choices = $derived(content.kind === 'text' ? graphicTextChoices : graphicShapeChoices)
  const parameters = $derived(content.parameters as Record<string, unknown>)
  function patch(key: string, value: unknown) { onchange({ ...content, parameters: { ...content.parameters, [key]: value } } as GraphicContent) }
  let error = $state('')
  function path(text: string) {
    try {
      const value: unknown = JSON.parse(text)
      if (!Array.isArray(value) || !value.every(p => Array.isArray(p) && p.length === 2 && p.every(n => typeof n === 'number' && Number.isFinite(n)))) throw new Error('Нужен массив точек [x,y]')
      patch('points', value); error = ''
    } catch (reason) { error = String(reason) }
  }
</script>
<details>
  <summary>Настройки слоя</summary>
  <div class="grid">
    {#each channels as [channel, title, min, max, fallback]}
      <label>{title}<input type="number" aria-label={`${label} ${channel}`} {min} {max} step="any" value={graphicNumericValue(content, channel, channel === 'positionX' ? width / 2 : channel === 'positionY' ? height / 2 : fallback)} onchange={event => onchange(setGraphicNumeric(content, channel, Number(event.currentTarget.value), width, height))} /></label>
    {/each}
    {#each flags as [key, title, fallback]}
      <label><input type="checkbox" aria-label={`${label} ${key}`} checked={Boolean(parameters[key] ?? fallback)} onchange={event => patch(key, event.currentTarget.checked)} />{title}</label>
    {/each}
    {#each choices as [key, title, fallback, options]}
      <label>{title}<select aria-label={`${label} ${key}`} value={Number(parameters[key] ?? fallback)} onchange={event => patch(key, Number(event.currentTarget.value))}>
        {#each options as option, index}<option value={index}>{option}</option>{/each}
      </select></label>
    {/each}
    {#if content.kind === 'text'}
      <label>Шрифт<input aria-label={`${label} font`} value={content.parameters.font ?? 'Inter'} onchange={event => patch('font', event.currentTarget.value)} /></label>
      <label>Начертание<input aria-label={`${label} fontStyle`} value={content.parameters.fontStyle ?? 'Regular'} onchange={event => patch('fontStyle', event.currentTarget.value)} /></label>
    {:else}
      <label>Точки контура<textarea aria-label={`${label} points`} value={JSON.stringify(content.parameters.points ?? [])} onchange={event => path(event.currentTarget.value)}></textarea></label>
    {/if}
  </div>
  {#if content.kind === 'text'}
    <GraphicCharacterFormatting text={content.parameters.text ?? 'Текст'} runs={content.parameters.runs ?? []} {label} onchange={runs => patch('runs', runs)} />
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</details>
<style>
  summary { cursor: pointer; margin: .4rem 0; }
  .grid { display: grid; grid-template-columns: repeat(2,minmax(0,1fr)); gap: .35rem; }
  label { display: flex; flex-direction: column; min-width: 0; font-size: .8rem; }
  input, select, textarea { max-width: 100%; min-width: 0; }
</style>
