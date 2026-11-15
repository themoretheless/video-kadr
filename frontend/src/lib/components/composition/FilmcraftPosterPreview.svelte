<script lang="ts">
  import { getFilmcraftPosterPreview, type FilmcraftMonitorFrame as Frame } from '$lib/api'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'

  let { document, item, name, sources, token }: {
    document: string; item: string; name: string; sources: Record<string, string>; token: string | null
  } = $props()
  let frame = $state<Frame>()
  let unavailable = $state(false)

  $effect(() => {
    const input = { document, item, sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)) }
    const owner = token
    const request = new AbortController()
    frame = undefined
    unavailable = false
    void getFilmcraftPosterPreview(input, request.signal, owner).then(result => {
      if (!request.signal.aborted) frame = result
    }).catch(() => {
      if (!request.signal.aborted) unavailable = true
    })
    return () => request.abort()
  })
</script>

<figure aria-label={`Обложка: ${name || item}`}>
  {#if frame}
    <FilmcraftMonitorFrame {frame} label={`Кадр обложки: ${name || item}`} metadata={false} />
  {:else if unavailable}
    <span>Кадр обложки недоступен</span>
  {:else}
    <span role="status">Загрузка кадра обложки…</span>
  {/if}
</figure>

<style>
  figure { margin: 0; }
</style>
