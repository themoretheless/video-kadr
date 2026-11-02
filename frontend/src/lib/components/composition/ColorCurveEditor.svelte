<script lang="ts">
  import type { CompositionBezierInterpolation,Composition } from '$lib/composition/types'
  import { configureTextColor } from '$lib/composition/colorCommands'
  import { compositionState,commitDocument } from '$lib/state/composition/core.svelte'
  import { colorCurvePreview,colorCurvePreviewOwner,clearColorCurvePreview } from '$lib/state/colorCurvePreview.svelte'
  import BezierControlEditor from './BezierControlEditor.svelte'
  let { clipId,tick,value,disabled = false,label }: { clipId: string;tick?: number;value: CompositionBezierInterpolation;disabled?: boolean;label: string } = $props()
  let captured: { document: Composition;clipId: string;tick?: number } | null = null
  const owner = colorCurvePreviewOwner()
  function cancel() { captured = null;clearColorCurvePreview(owner) }
  $effect(() => () => clearColorCurvePreview(owner))
  function preview(interpolation: CompositionBezierInterpolation) {
    if (disabled) return
    captured ??= { document: compositionState.document,clipId,tick }
    if (captured.document !== compositionState.document || captured.clipId !== clipId || captured.tick !== tick) { clearColorCurvePreview(owner);return }
    try {
      const document = configureTextColor(captured.document,captured.clipId,{ tick: captured.tick,interpolation })
      colorCurvePreview.owner = owner;colorCurvePreview.source = captured.document;colorCurvePreview.document = document
    } catch (error) { clearColorCurvePreview(owner);compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  function commit(interpolation: CompositionBezierInterpolation) {
    const source = captured
    cancel()
    if (disabled) return
    if (source && (source.document !== compositionState.document || source.clipId !== clipId || source.tick !== tick)) {
      compositionState.ui.message = 'Документ или выбранная кривая изменились во время жеста'
      return
    }
    try { commitDocument(configureTextColor(compositionState.document,clipId,{ tick,interpolation })) }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
</script>
<div role="group" aria-label={label}>
  <BezierControlEditor {value} {disabled} onpreview={preview} oncommit={commit} oncancel={cancel} />
</div>
