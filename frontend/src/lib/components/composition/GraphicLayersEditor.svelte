<script lang="ts">
  import { editGraphicTextRuns, mapGraphicClock } from '$lib/api'
  import GraphicLayerPins from './GraphicLayerPins.svelte'
  import GraphicLayerAnimation from './GraphicLayerAnimation.svelte'
  import GraphicLayerProperties from './GraphicLayerProperties.svelte'
  import type { GraphicLayer, GraphicResponsiveTime } from '$lib/composition/types'
  import { appendGraphicLayer, moveGraphicLayer, cloneGraphicValue } from '$lib/composition/graphicLayers'
  let { layers, width = 1920, height = 1080, localUs = 0, responsiveTime, relativeUs = 0, durationUs = 1, animationOffsetUs = 0, onchange }: { layers: readonly GraphicLayer[]; width?: number; height?: number; localUs?: number; responsiveTime?: GraphicResponsiveTime; relativeUs?: number; durationUs?: number; animationOffsetUs?: number; onchange: (layers: readonly GraphicLayer[]) => void } = $props()
  let textError = $state('')
  let mappedLocalUs = $state(0)
  let clockPending = $state(false)
  let clockError = $state('')
  $effect(() => {
    const meta = responsiveTime
    if (!meta || (meta.intro <= 0 && meta.outro <= 0) || meta.designDuration <= 0) {
      mappedLocalUs = localUs; clockPending = false; clockError = ''; return
    }
    const controller = new AbortController()
    clockPending = true; clockError = ''
    void mapGraphicClock(meta,relativeUs,durationUs,animationOffsetUs,controller.signal).then(time => {
      if (!controller.signal.aborted) {mappedLocalUs=time;clockPending=false}
    }).catch(reason => {if (!controller.signal.aborted) {clockError=String(reason);clockPending=false}})
    return () => controller.abort()
  })
  async function replaceText(uid: number, text: string) {
    const original = layers.find(layer => layer.uid === uid)
    if (original?.content.kind !== 'text') return
    const snapshot = JSON.stringify(original.content)
    const parameters = cloneGraphicValue(original.content.parameters)
    try {
      const runs = parameters.runs?.length ? await editGraphicTextRuns(parameters.text ?? 'Текст', parameters.runs, {kind:'adjust',newText:text}) : parameters.runs
      if (JSON.stringify(layers.find(layer => layer.uid === uid)?.content) !== snapshot) throw new Error('Слой изменился во время форматирования; повторите ввод')
      edit(uid, layer => layer.content.kind === 'text' ? {...layer,content:{...layer.content,parameters:{...layer.content.parameters,text,...(runs ? {runs} : {})}}} : layer)
      textError = ''
    } catch (reason) { textError = String(reason) }
  }
  function edit(uid: number, change: (layer: GraphicLayer) => GraphicLayer) {
    onchange(layers.map(layer => layer.uid === uid ? change(cloneGraphicValue(layer)) : layer))
  }
</script>
<fieldset disabled={clockPending || !!clockError}>
  <legend>Графические слои</legend>
  <div class="actions">
    <button class="btn ghost sm" type="button" onclick={() => onchange(appendGraphicLayer(layers, 'text'))}>+ Текстовый слой</button>
    <button class="btn ghost sm" type="button" onclick={() => onchange(appendGraphicLayer(layers, 'shape'))}>+ Фигура</button>
  </div>
  {#each layers as layer, index (layer.uid)}
    <div class="layer">
      <div class="actions">
        <label><input type="checkbox" aria-label={`Видимость слоя ${layer.uid}`} checked={layer.enabled} onchange={event => edit(layer.uid, value => ({ ...value, enabled: event.currentTarget.checked }))} /> {layer.content.kind === 'text' ? 'Текст' : 'Фигура'} · {layer.uid}</label>
        <button class="btn ghost sm" type="button" aria-label={`Слой ${layer.uid} назад`} disabled={index === 0} onclick={() => onchange(moveGraphicLayer(layers, layer.uid, -1))}>↓</button>
        <button class="btn ghost sm" type="button" aria-label={`Слой ${layer.uid} вперёд`} disabled={index === layers.length - 1} onclick={() => onchange(moveGraphicLayer(layers, layer.uid, 1))}>↑</button>
        <button class="btn ghost sm" type="button" aria-label={`Удалить слой ${layer.uid}`} onclick={() => onchange(layers.filter(value => value.uid !== layer.uid))}>Удалить</button>
      </div>
      {#if layer.content.kind === 'text'}
        <label>Текст<textarea aria-label={`Текст слоя ${layer.uid}`} value={layer.content.parameters.text ?? 'Текст'} onchange={event => void replaceText(layer.uid, event.currentTarget.value)}></textarea></label>
      {:else}
        <label>Фигура<select aria-label={`Фигура слоя ${layer.uid}`} value={String(layer.content.parameters.shape ?? 0)} onchange={event => edit(layer.uid, value => value.content.kind === 'shape' ? { ...value, content: { ...value.content, parameters: { ...value.content.parameters, shape: Number(event.currentTarget.value) as 0 | 1 | 2 | 3 } } } : value)}>
          <option value="0">Прямоугольник</option><option value="1">Эллипс</option><option value="2">Многоугольник</option><option value="3">Контур</option>
        </select></label>
      {/if}
      <GraphicLayerPins {layer} {layers} {width} {height} localUs={mappedLocalUs} onchange={pin => edit(layer.uid, value => { const { pin: previous, ...rest } = value; return pin ? { ...rest, pin } : rest })} />
      <GraphicLayerProperties content={layer.content} label={`Слой ${layer.uid}`} {width} {height} onchange={content => edit(layer.uid, value => ({ ...value, content }))} />
      <GraphicLayerAnimation content={layer.content} label={`Слой ${layer.uid}`} {width} {height} localUs={mappedLocalUs} onchange={content => edit(layer.uid, value => ({ ...value, content }))} />
    </div>
  {/each}
  {#if clockError}<p role="alert">{clockError}</p>{/if}
  {#if textError}<p role="alert">{textError}</p>{/if}
</fieldset>
<style>
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; }
  legend { font-size: inherit; font-weight: 600; margin-bottom: .4rem; }
  .actions { display: flex; align-items: center; gap: .3rem; flex-wrap: wrap; }
  .layer { padding: .5rem 0; border-bottom: 1px solid var(--border, #444); }
  label { display: flex; gap: .4rem; align-items: center; }
  textarea { width: 100%; min-height: 3rem; }
</style>
