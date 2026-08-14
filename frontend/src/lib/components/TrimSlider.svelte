<script lang="ts">
  import { onDestroy } from 'svelte'

  interface Props {
    min: number
    max: number
    start: number
    end: number
    step?: number
    gap?: number
    onstartchange?: (value: number) => void
    onendchange?: (value: number) => void
    oninteractionstart?: () => void
    oninteractionend?: () => void
  }

  let {
    min,
    max,
    start,
    end,
    step = 0.1,
    gap = 0.1,
    onstartchange,
    onendchange,
    oninteractionstart,
    oninteractionend,
  }: Props = $props()

  let track: HTMLElement
  let active: 'start' | 'end' | null = null
  const pct = (value: number) => max <= min ? 0 : Math.max(0, Math.min(100, ((value - min) / (max - min)) * 100))
  let startPct = $derived(pct(start))
  let endPct = $derived(pct(end))

  const snap = (value: number) => Math.round(value / step) * step
  const clampStart = (value: number) => Math.max(min, Math.min(snap(value), end - gap))
  const clampEnd = (value: number) => Math.min(max, Math.max(snap(value), start + gap))

  function valueFromClientX(clientX: number): number {
    if (!track) return min
    const rect = track.getBoundingClientRect()
    const ratio = rect.width <= 0 ? 0 : (clientX - rect.left) / rect.width
    return min + Math.max(0, Math.min(1, ratio)) * (max - min)
  }

  function onMove(event: PointerEvent): void {
    if (!active) return
    const value = valueFromClientX(event.clientX)
    if (active === 'start') onstartchange?.(clampStart(value))
    else onendchange?.(clampEnd(value))
  }

  function stopDrag(): void {
    const wasActive = active !== null
    active = null
    window.removeEventListener('pointermove', onMove)
    window.removeEventListener('pointerup', stopDrag)
    window.removeEventListener('pointercancel', stopDrag)
    if (wasActive) oninteractionend?.()
  }

  function startDrag(which: 'start' | 'end', event: PointerEvent): void {
    event.stopPropagation()
    if (!active) oninteractionstart?.()
    active = which
    ;(event.currentTarget as HTMLElement).setPointerCapture?.(event.pointerId)
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', stopDrag)
    window.addEventListener('pointercancel', stopDrag)
  }

  function onTrackDown(event: PointerEvent): void {
    if ((event.target as HTMLElement).classList.contains('trim-handle')) return
    const value = valueFromClientX(event.clientX)
    const which = Math.abs(value - start) <= Math.abs(value - end) ? 'start' : 'end'
    startDrag(which, event)
    if (which === 'start') onstartchange?.(clampStart(value))
    else onendchange?.(clampEnd(value))
  }

  function onKey(which: 'start' | 'end', event: KeyboardEvent): void {
    const amount = event.shiftKey ? 1 : step
    let delta = 0
    if (event.key === 'ArrowLeft' || event.key === 'ArrowDown') delta = -amount
    else if (event.key === 'ArrowRight' || event.key === 'ArrowUp') delta = amount
    else return
    event.preventDefault()
    if (which === 'start') onstartchange?.(clampStart(start + delta))
    else onendchange?.(clampEnd(end + delta))
  }

  onDestroy(stopDrag)
</script>

<div class="trim">
  <div bind:this={track} class="trim-track" role="group" aria-label="Диапазон обрезки" onpointerdown={onTrackDown}>
    <div class="trim-fill" style:left={`${startPct}%`} style:width={`${endPct - startPct}%`}></div>
    <div
      class="trim-handle"
      style:left={`${startPct}%`}
      tabindex="0"
      role="slider"
      aria-label="Начало обрезки"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={start}
      onpointerdown={(event) => startDrag('start', event)}
      onkeydown={(event) => onKey('start', event)}
    ></div>
    <div
      class="trim-handle"
      style:left={`${endPct}%`}
      tabindex="0"
      role="slider"
      aria-label="Конец обрезки"
      aria-valuemin={min}
      aria-valuemax={max}
      aria-valuenow={end}
      onpointerdown={(event) => startDrag('end', event)}
      onkeydown={(event) => onKey('end', event)}
    ></div>
  </div>
</div>
