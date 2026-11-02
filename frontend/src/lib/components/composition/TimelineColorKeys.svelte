<script lang="ts">
  import type { Composition,CompositionTrack,TextClip,ColorClip } from '$lib/composition/types'
  import { COMPOSITION_TIME_BASE } from '$lib/composition/types'
  import { moveTextColorKeys } from '$lib/composition/colorCommands'
  import { evaluateAgentBatch } from '$lib/agent/operations'
  import { shortcutCommandForEvent,shortcutState } from '$lib/state/shortcuts.svelte'
  import { executeCompositionCommand } from '$lib/composition/commandRegistry'
  import { colorKeyDragPreview,clearColorKeyDragPreview } from '$lib/state/colorKeyDragPreview.svelte'
  import { colorCurvePreview,colorCurvePreviewOwner,clearColorCurvePreview } from '$lib/state/colorCurvePreview.svelte'
  import { compositionState,commitDocument,setCompositionPlayhead } from '$lib/state/composition.svelte'
  let { track,width }: { track: CompositionTrack;width: number } = $props()
  const clips = $derived((track.kind === 'text' || track.kind === 'color') ? track.clips.filter(clip => clip.colorAnimation?.mode === 'keyframes') : [])
  const allClips = $derived(compositionState.document.tracks.flatMap<TextClip | ColorClip>(track => (track.kind === 'text' || track.kind === 'color') ? track.clips : []))
  const selected = $derived(compositionState.ui.selectedColorKeys.filter(subject => allClips.some(clip => clip.id === subject.clipId && clip.colorAnimation?.mode === 'keyframes' && clip.colorAnimation.track.keyframes.some(key => key.tick === subject.tick))))
  function isSelected(clipId: string,tick: number): boolean { return selected.some(subject => subject.clipId === clipId && subject.tick === tick) }
  function select(clipId: string,tick: number,extend: boolean): void {
    const subject = { clipId,tick }
    compositionState.ui.selectedColorKeys = extend ? isSelected(clipId,tick) ? selected.filter(value => value.clipId !== clipId || value.tick !== tick) : [...selected,subject] : [subject]
  }
  const owner = colorCurvePreviewOwner()
  let drag = $state<{ source: Composition;clipId: string;tick: number;next: number;startX: number;timeBase: number;zoom: number;pointerId: number;button: HTMLElement;active: boolean;deltaSeconds: number;scale: boolean;pivot: number;span: number;subjects: { clipId: string;tick: number;timeBase: number;startSeconds: number;seconds: number }[] } | null>(null)
  let suppressClick = false
  function begin(event: PointerEvent,clip: TextClip | ColorClip,tick: number): void {
    if (event.button !== 0 || track.locked || clip.locked || clip.colorAnimation?.mode !== 'keyframes') return
    event.preventDefault();event.stopPropagation()
    suppressClick = false
    if (event.shiftKey || !isSelected(clip.id,tick)) select(clip.id,tick,event.shiftKey)
    if (!isSelected(clip.id,tick)) return
    const subjects = selected.map(subject => {
      const target = allClips.find(value => value.id === subject.clipId)!
      const timeBase = target.colorAnimation!.mode === 'keyframes' ? target.colorAnimation!.track.timeBase : COMPOSITION_TIME_BASE
      const startSeconds = target.timelineStartTicks/COMPOSITION_TIME_BASE
      return { ...subject,timeBase,startSeconds,seconds: startSeconds+subject.tick/timeBase }
    })
    const pivot = Math.min(...subjects.map(subject => subject.seconds))
    const span = subjects.find(subject => subject.clipId === clip.id && subject.tick === tick)!.seconds-pivot
    if (event.altKey && span <= 0) { compositionState.ui.message = 'Для масштаба перетащите ключ после начала выделения';return }
    const button = event.currentTarget as HTMLElement
    button.focus()
    drag = { source: compositionState.document,clipId: clip.id,tick,next: tick,startX: event.clientX,timeBase: clip.colorAnimation.track.timeBase,zoom: compositionState.ui.zoomPxPerSecond,pointerId: event.pointerId,button,active: false,deltaSeconds: 0,subjects,scale: event.altKey,pivot,span }
    if (Number.isInteger(event.pointerId)) button.setPointerCapture?.(event.pointerId)
  }
  function dragEdits(captured: NonNullable<typeof drag>) {
    const factor = captured.scale ? (captured.span+captured.deltaSeconds)/captured.span : 1
    if (!Number.isFinite(factor) || factor <= 0) throw new Error('Color key time scale must stay positive')
    return captured.subjects.map(subject => ({ clipId: subject.clipId,tick: subject.tick,newTick: captured.scale
      ? Math.round((captured.pivot+(subject.seconds-captured.pivot)*factor-subject.startSeconds)*subject.timeBase)
      : subject.tick+Math.round(captured.deltaSeconds*subject.timeBase) }))
  }
  function shownTick(clipId: string,tick: number): number {
    if (colorKeyDragPreview.source !== compositionState.document) return tick
    return colorKeyDragPreview.edits.find(subject => subject.clipId === clipId && subject.tick === tick)?.newTick ?? tick
  }
  function move(event: PointerEvent): void {
    if (!drag || event.pointerId !== drag.pointerId) return
    event.preventDefault()
    const delta = event.clientX-drag.startX
    if (!drag.active && Math.abs(delta) < 3) return
    drag.active = true
    drag.deltaSeconds = delta/drag.zoom
    drag.next = drag.tick+Math.round(drag.deltaSeconds*drag.timeBase)
    if (compositionState.document !== drag.source) { end(undefined,true);return }
    try {
      const edits = dragEdits(drag)
      colorKeyDragPreview.owner = owner;colorKeyDragPreview.source = drag.source;colorKeyDragPreview.edits = edits
      const candidate = moveTextColorKeys(drag.source,edits)
      colorCurvePreview.owner = owner;colorCurvePreview.source = drag.source;colorCurvePreview.document = candidate
    } catch { clearColorCurvePreview(owner);clearColorKeyDragPreview(owner) }
  }
  function end(event?: PointerEvent,cancel = false): void {
    const captured = drag
    if (!captured || event && event.pointerId !== captured.pointerId) return
    drag = null
    clearColorCurvePreview(owner)
    clearColorKeyDragPreview(owner)
    suppressClick = captured.active
    if (captured.button.hasPointerCapture?.(captured.pointerId)) captured.button.releasePointerCapture(captured.pointerId)
    if (cancel || !captured.active || compositionState.document !== captured.source) return
    try {
      const edits = dragEdits(captured)
      if (edits.every(edit => edit.tick === edit.newTick)) return
      commitDocument(moveTextColorKeys(captured.source,edits))
      compositionState.ui.selectedColorKeys = edits.map(subject => ({ clipId: subject.clipId,tick: subject.newTick }))
    }
    catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
  $effect(() => () => { clearColorCurvePreview(owner);clearColorKeyDragPreview(owner) })
  function edit(event: KeyboardEvent,clip: TextClip | ColorClip,tick: number): void {
    const command = shortcutState.capturingCommandId ? null : shortcutCommandForEvent('composition',event)
    if (command === 'composition.copy' || command === 'composition.paste') {
      if (executeCompositionCommand(command)) { event.preventDefault();event.stopPropagation() }
      return
    }
    if (!['ArrowLeft','ArrowRight','Delete','Backspace'].includes(event.key)) return
    event.preventDefault();event.stopPropagation()
    if (track.locked || clip.locked) return
    try {
      const subjects = isSelected(clip.id,tick) ? selected : [{ clipId: clip.id,tick }]
      if (event.key === 'Delete' || event.key === 'Backspace') {
        commitDocument(evaluateAgentBatch(compositionState.document,subjects.map(subject => ({ op: 'delete_color_key' as const,...subject }))))
        compositionState.ui.selectedColorKeys = []
      }
      else {
        const value = clip.colorAnimation
        if (value?.mode !== 'keyframes') return
        const edits = subjects.map(subject => {
          const target = allClips.find(value => value.id === subject.clipId)!
          if (target.colorAnimation?.mode !== 'keyframes') throw new Error('Color keys required')
          const frame = Math.max(1,Math.round(target.colorAnimation.track.timeBase/compositionState.document.canvas.fps))
          return { ...subject,newTick: subject.tick+(event.key === 'ArrowLeft' ? -1 : 1)*frame*(event.shiftKey ? 10 : 1) }
        })
        const next = edits.find(subject => subject.clipId === clip.id && subject.tick === tick)!.newTick
        const row = (event.currentTarget as HTMLElement).closest('.composition-color-key-row')
        commitDocument(moveTextColorKeys(compositionState.document,edits))
        compositionState.ui.selectedColorKeys = edits.map(subject => ({ clipId: subject.clipId,tick: subject.newTick }))
        queueMicrotask(() => row?.querySelector<HTMLButtonElement>(`[data-color-key-tick="${next}"]`)?.focus())
      }
    } catch (error) { compositionState.ui.message = error instanceof Error ? error.message : String(error) }
  }
</script>

<svelte:window onpointermove={move} onpointerup={(event) => end(event)} onpointercancel={(event) => end(event,true)} onkeydown={(event) => { if (drag && event.key === 'Escape') { event.preventDefault();event.stopPropagation();end(undefined,true) } }} />

{#each clips as clip (clip.id)}
  {#if clip.colorAnimation?.mode === 'keyframes'}
    <div class="composition-track-row composition-color-key-row" data-color-key-clip={clip.id}>
      <div class="composition-track-head"><small title={clip.kind === 'text' ? clip.text : clip.color}>Цвет · {clip.kind === 'text' ? clip.text : clip.color}</small></div>
      <div class="color-key-lane" style:width={`${width}px`}>
        {#each clip.colorAnimation.track.keyframes as key (key.tick)}
          {@const timelineTick = clip.timelineStartTicks+Math.round(shownTick(clip.id,key.tick)*COMPOSITION_TIME_BASE/clip.colorAnimation.track.timeBase)}
          <button class="color-key" class:selected={isSelected(clip.id,key.tick)} aria-pressed={isSelected(clip.id,key.tick)} type="button" data-color-key-tick={key.tick} disabled={track.locked || clip.locked} style:left={`${timelineTick/COMPOSITION_TIME_BASE*compositionState.ui.zoomPxPerSecond}px`} style:color={key.value} aria-label={`Ключ цвета ${clip.id} ${key.tick}`} title={`${key.value} · ${timelineTick/COMPOSITION_TIME_BASE} с; ←/→: кадр, Shift: 10 кадров, Delete: удалить`} onpointerdown={(event) => begin(event,clip,key.tick)} onlostpointercapture={() => end(undefined,true)} onclick={(event) => { event.stopPropagation();if (suppressClick && event.detail !== 0) { suppressClick = false;return } if (event.detail === 0) select(clip.id,key.tick,event.shiftKey);setCompositionPlayhead(timelineTick) }} onkeydown={(event) => edit(event,clip,key.tick)}>◆</button>
        {/each}
      </div>
    </div>
  {/if}
{/each}

<style>
  .color-key-lane { position:relative; height:24px; flex:none; }
  .color-key { position:absolute; top:2px; transform:translateX(-50%); border:0; background:transparent; padding:2px; font-size:14px; line-height:16px; cursor:pointer; text-shadow:0 0 2px #888; }
  .color-key.selected { outline:1px solid currentColor; background:color-mix(in srgb,currentColor 15%,transparent); }
  .color-key:focus-visible { outline:1px solid currentColor; }
  .color-key:disabled { opacity:.5; cursor:default; }
</style>
