<script lang="ts">
  import { getFilmcraftPreviewFrame, type FilmcraftExportSummary, type FilmcraftPreviewFrame } from '$lib/api'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'

  let { document, sequenceId, sources, timeFrames, timeTicks, token, video }: {
    document: string; sequenceId: string; sources: Record<string, string>
    timeFrames: string; timeTicks?: string; token: string | null
    video: NonNullable<FilmcraftExportSummary['video']>
  } = $props()

  let frame = $state<FilmcraftPreviewFrame | null>(null)
  let pending = $state(false)
  let error = $state('')
  const fit = { scaleToFit: 'contain', scaleToFill: 'cover', stretchToFill: 'fill' }

  $effect(() => {
    const owner = token
    const clock = timeTicks ?? timeFrames
    frame = null
    error = ''
    pending = false
    if (!/^-?\d{1,19}$/.test(clock)) return
    const position = BigInt(clock)
    if (position < -9223372036854775808n || position > 9223372036854775807n) return
    const input = {
      document, sequenceId, sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)),
      ...(timeTicks !== undefined ? { timeTicks } : { timeFrames }),
      effects: true, captions: true,
    }
    const controller = new AbortController()
    pending = true
    const timer = setTimeout(() => {
      void getFilmcraftPreviewFrame(input, controller.signal, owner).then(value => {
        if (!controller.signal.aborted) frame = value
      }).catch(failure => {
        if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
      }).finally(() => {
        if (!controller.signal.aborted) pending = false
      })
    }, 100)
    return () => {
      clearTimeout(timer)
      controller.abort()
    }
  })
</script>

{#if frame}
  <div class="preview" style:aspect-ratio={`${video.width} / ${video.height}`} style:--export-fit={fit[video.scaling]}>
    <FilmcraftMonitorFrame {frame} label="Кадр экспорта Filmcraft" fluid metadata={false} />
  </div>
{:else if pending}
  <small role="status">Подготовка кадра экспорта…</small>
{:else if error}
  <small role="alert">{error}</small>
{/if}

<style>
  .preview { width: min(100%, 480px); background: black; position: relative; }
  .preview :global(canvas) {
    position: absolute; inset: 0; width: 100%; height: 100%;
    object-fit: var(--export-fit); background: transparent;
  }
</style>
