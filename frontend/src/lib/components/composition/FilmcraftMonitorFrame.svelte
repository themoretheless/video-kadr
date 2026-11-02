<script lang="ts">
  import type { FilmcraftMonitorFrame } from '$lib/api'
  import { decodeMonitorFrame } from '$lib/filmcraft/monitor-frame'
  let { frame, label, fluid = false, metadata = true }: { frame: FilmcraftMonitorFrame; label: string; fluid?: boolean; metadata?: boolean } = $props()
  let error = $state('')
  function paint(canvas: HTMLCanvasElement, initial: FilmcraftMonitorFrame) {
    function update(value: FilmcraftMonitorFrame) {
      error = ''
      try {
        const bytes = decodeMonitorFrame(value)
        canvas.width = value.width; canvas.height = value.height
        const context = canvas.getContext('2d')
        if (!context) throw new Error('Просмотр кадра недоступен')
        const image = context.createImageData(value.width, value.height)
        image.data.set(bytes); context.putImageData(image, 0, 0)
      } catch (failure) { canvas.width = 1; canvas.height = 1; error = failure instanceof Error ? failure.message : String(failure) }
    }
    update(initial)
    return { update }
  }
</script>
<canvas class:fluid use:paint={frame} aria-label={label}></canvas>
{#if error}<small role="alert">{error}</small>{:else if metadata}<small>{frame.kind === 'sequence' ? 'Кадр композиции' : frame.kind === 'image' ? 'Изображение' : frame.kind === 'video' ? 'Видеокадр' : 'Кадр генератора'} · время: {frame.kind === 'image' ? frame.sampleTicks ?? frame.ptsTicks : frame.ptsTicks}</small>{/if}
<style>
  canvas { display: block; width: 240px; max-width: 100%; height: auto; background: #333; }
  canvas.fluid { width: 100%; }
  small { display: block; }
</style>
