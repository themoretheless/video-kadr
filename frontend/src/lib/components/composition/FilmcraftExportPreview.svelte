<script lang="ts">
  import { previewFilmcraftExport, type FilmcraftExportSummary, type FilmcraftVideoRequest } from '$lib/api'

  let { input, timeFrames, timeTicks, token, video }: {
    input: FilmcraftVideoRequest; timeFrames: string; timeTicks?: string; token: string | null
    video: NonNullable<FilmcraftExportSummary['video']>
  } = $props()

  let url = $state('')
  let pending = $state(false)
  let error = $state('')

  $effect(() => {
    const owner = token
    const clock = timeTicks ?? timeFrames
    url = ''
    error = ''
    pending = false
    if (!owner || !/^-?\d{1,19}$/.test(clock)) return
    const position = BigInt(clock)
    if (position < -9223372036854775808n || position > 9223372036854775807n) return
    const request = input
    const time = timeTicks !== undefined ? { timeTicks } : { timeFrames }
    let frameUrl = ''
    const controller = new AbortController()
    pending = true
    const timer = setTimeout(() => {
      void previewFilmcraftExport(owner, request, time, controller.signal).then(blob => {
        if (!controller.signal.aborted) url = frameUrl = URL.createObjectURL(blob)
      }).catch(failure => {
        if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
      }).finally(() => {
        if (!controller.signal.aborted) pending = false
      })
    }, 100)
    return () => {
      clearTimeout(timer)
      controller.abort()
      if (frameUrl) URL.revokeObjectURL(frameUrl)
    }
  })
</script>

{#if url}
  <img src={url} alt="Кадр экспорта Filmcraft" style:aspect-ratio={`${video.width} / ${video.height}`} />
{:else if pending}
  <small role="status">Подготовка кадра экспорта…</small>
{:else if error}
  <small role="alert">{error}</small>
{/if}

<style>
  img { display: block; width: min(100%, 480px); height: auto; background: black; }
</style>
