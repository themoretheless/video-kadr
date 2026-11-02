<script lang="ts">
  import type { VisualClip,CompositionPositionAnimation,Composition } from '$lib/composition/types'
  import { clipDurationTicks } from '$lib/composition/types'
  import { previewPositionConversion,type NativePositionConversionResult } from '$lib/api'
  import { packPositionConversion,unpackPositionConversion } from '$lib/composition/positionConversion'
  import { applyPositionConversion } from '$lib/composition/motionPathCommands'
  import { evaluateAgentBatch } from '$lib/agent/operations'
  import { compositionState,commitDocument } from '$lib/state/composition/core.svelte'
  import { positionConversionPreview } from '$lib/state/positionConversionPreview.svelte'
  let { clip }: { clip: VisualClip } = $props()
  let opened = $state(false),tolerance = $state(0.5),interval = $state(1),mode = $state<'editable' | 'bake'>('editable')
  let startFrame = $state(0),endFrame = $state<number | null>(null)
  let result = $state<NativePositionConversionResult | null>(null),message = $state(''),busy = $state(false)
  let source = $state.raw<Composition | null>(null)
  const locked = $derived(Boolean(clip.locked || compositionState.document.tracks.some(track => track.locked && track.clips.some(item => item.id === clip.id))))
  $effect(() => {
    const captured = compositionState.document
    const selected = clip
    const parameters = { tolerance,interval,mode,startFrame,endFrame }
    const enabled = opened
    const controller = new AbortController()
    let live = true
    result = null;source = null;positionConversionPreview.source = null;positionConversionPreview.document = null
    if (!enabled) { busy = false;return }
    busy = true;message = 'Расчёт…'
    const timer = setTimeout(async () => {
      try {
        const fallback = selected.kind === 'text' ? { x: selected.x,y: selected.y } : selected.transform
        const position: CompositionPositionAnimation = selected.position ?? { mode: 'xy',x: selected.animation?.x ?? { mode: 'constant',value: fallback.x },y: selected.animation?.y ?? { mode: 'constant',value: fallback.y } }
        const rate = captured.canvas.fpsRate ?? { numerator: captured.canvas.fps,denominator: 1 }
        const lastFrame = Math.round(clipDurationTicks(selected)/captured.timeBase*rate.numerator/rate.denominator)
        const finalFrame = parameters.endFrame ?? lastFrame
        if (!Number.isSafeInteger(parameters.startFrame) || !Number.isSafeInteger(finalFrame) || parameters.startFrame < 0 || finalFrame <= parameters.startFrame || finalFrame > lastFrame) throw new Error('Выберите диапазон кадров внутри клипа')
        const options = { fpsNum: rate.numerator,fpsDen: rate.denominator,startFrame: parameters.startFrame,endFrame: finalFrame,tolerancePx: parameters.tolerance,everyFrames: parameters.mode === 'bake' ? parameters.interval : 1,xyMode: parameters.mode }
        const answer = await previewPositionConversion(packPositionConversion(position,options),controller.signal)
        if (!live || compositionState.document !== captured) return
        const authored = unpackPositionConversion(answer,index => `conversion-${selected.id}-${index}`,true)
        const document = evaluateAgentBatch(captured,[{ op: 'update_layer',clipId: selected.id,patch: { position: authored } }])
        result = answer;source = captured
        positionConversionPreview.source = captured;positionConversionPreview.document = document
        message = `Ошибка ${answer.maxErrorPx.toFixed(3)} px · ${answer.nodeCount} узлов · ${answer.checkCount} проверок${answer.limit ? ` · ${answer.limit}` : ''}`
      } catch (error) { if (live && !controller.signal.aborted) message = error instanceof Error ? error.message : String(error) }
      finally { if (live) busy = false }
    },150)
    return () => { live = false;clearTimeout(timer);controller.abort();positionConversionPreview.source = null;positionConversionPreview.document = null }
  })
  function apply() {
    if (!result || !source || source !== compositionState.document || locked) return
    try { commitDocument(applyPositionConversion(source,clip.id,result,index => `conversion-${clip.id}-${index}`));opened = false }
    catch (error) { message = error instanceof Error ? error.message : String(error) }
  }
</script>
<button class="btn ghost sm" disabled={locked} onclick={() => { opened = !opened }}>{opened ? 'Отменить конвертацию' : clip.position?.mode === 'path' ? 'Конвертировать в XY' : 'Конвертировать в Path'}</button>
{#if opened}
  <div class="composition-form-grid">
    <label>Начальный кадр<input aria-label="Начальный кадр конвертации" type="number" min="0" step="1" bind:value={startFrame} /></label>
    <label>Конечный кадр<input aria-label="Конечный кадр конвертации" type="number" min="1" step="1" placeholder="Конец клипа" bind:value={endFrame} /></label>
    <label>Допуск, px<input aria-label="Допуск конвертации позиции" type="number" min="0.05" step="0.05" bind:value={tolerance} /></label>
    {#if clip.position?.mode === 'path'}
      <label>Режим<select bind:value={mode}><option value="editable">Редактируемые ключи</option><option value="bake">Frame baking</option></select></label>
      <label>Интервал кадров<input type="number" min="1" step="1" bind:value={interval} /></label>
    {/if}
    <p aria-live="polite">{message}</p>
    <button class="btn ghost sm" disabled={busy || !result?.withinTolerance || locked || source !== compositionState.document} onclick={apply}>Применить конвертацию</button>
  </div>
{/if}
