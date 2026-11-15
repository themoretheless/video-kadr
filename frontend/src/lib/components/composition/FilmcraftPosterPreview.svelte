<script lang="ts">
  import { getFilmcraftPosterPreview, type FilmcraftMonitorFrame as Frame } from '$lib/api'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'

  let { document, item, name, sources, token, card = false }: {
    document: string; item: string; name: string; sources: Record<string, string>; token: string | null; card?: boolean
  } = $props()
  let frame = $state<Frame>()
  let unavailable = $state(false)

  $effect(() => {
    const input = { document, item, ...(card ? { view: 'card' as const } : {}), sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)) }
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

<figure class:card aria-label={`Обложка: ${name || item}`}>
  {#if frame}
    <FilmcraftMonitorFrame {frame} label={`Кадр обложки: ${name || item}`} metadata={false} fluid={card} />
  {:else if unavailable}
    <span>Кадр обложки недоступен</span>
  {:else}
    <span role="status">Загрузка кадра обложки…</span>
  {/if}
</figure>

<style>
  figure { margin: 0; }
  figure.card { display: grid; place-items: center; width: 100%; aspect-ratio: 16 / 9; overflow: hidden; background: var(--panel-2); }
  figure.card :global(canvas) { width: 100%; height: 100%; object-fit: contain; }
</style>
