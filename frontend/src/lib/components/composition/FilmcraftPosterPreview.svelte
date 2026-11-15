<script lang="ts">
  import { getFilmcraftPosterPreview, type FilmcraftMonitorFrame as Frame } from '$lib/api'
  import FilmcraftMonitorFrame from './FilmcraftMonitorFrame.svelte'

  let { document, item, name, sources, token, card = false, hoverFraction, markRange, lazy = false }: {
    document: string; item: string; name: string; sources: Record<string, string>; token: string | null; card?: boolean; hoverFraction?: number; markRange?: { start: number; end: number } | null; lazy?: boolean
  } = $props()
  let preview = $state<HTMLElement>()
  let visible = $state(false)
  let frame = $state<Frame>()
  let unavailable = $state(false)
  let hoverFrame = $state<Frame>()
  const input = $derived({ document, item, ...(card ? { view: 'card' as const } : {}), sources: Object.fromEntries(Object.entries(sources).filter(([, id]) => id)) })
  const shown = $derived(hoverFraction === undefined ? frame : hoverFrame ?? frame)

  $effect(() => {
    const element = preview
    if (!lazy || !element || typeof IntersectionObserver === 'undefined') {
      visible = true
      return
    }
    const observer = new IntersectionObserver(([entry]) => { visible = entry?.isIntersecting ?? false })
    observer.observe(element)
    return () => observer.disconnect()
  })

  $effect(() => {
    if (lazy && !visible) return
    const posterInput = input
    const owner = token
    const request = new AbortController()
    frame = undefined
    hoverFrame = undefined
    unavailable = false
    void getFilmcraftPosterPreview(posterInput, request.signal, owner).then(result => {
      if (!request.signal.aborted) frame = result
    }).catch(() => {
      if (!request.signal.aborted) unavailable = true
    })
    return () => request.abort()
  })

  $effect(() => {
    const fraction = hoverFraction
    if (!card || fraction === undefined) {
      hoverFrame = undefined
      return
    }
    const hoverInput = { ...input, hoverFraction: fraction }
    const owner = token
    const request = new AbortController()
    const timer = setTimeout(() => {
      void getFilmcraftPosterPreview(hoverInput, request.signal, owner).then(result => {
        if (!request.signal.aborted) hoverFrame = result
      }).catch(() => {})
    }, 60)
    return () => { clearTimeout(timer); request.abort() }
  })
</script>

<figure bind:this={preview} class:card aria-label={`Обложка: ${name || item}`}>
  {#if shown}
    <FilmcraftMonitorFrame frame={shown} label={`Кадр обложки: ${name || item}`} metadata={false} fluid={card} />
  {:else if unavailable}
    <span>Кадр обложки недоступен</span>
  {:else}
    <span role="status">Загрузка кадра обложки…</span>
  {/if}
  {#if card && markRange}
    <span class="mark-range" role="img" aria-label="Диапазон In/Out" style:left={`${markRange.start * 100}%`} style:width={`${Math.max(0, markRange.end - markRange.start) * 100}%`}></span>
  {/if}
  {#if hoverFraction !== undefined}
    <span class="hover-position" aria-hidden="true" style:left={`${hoverFraction * 100}%`}></span>
  {/if}
</figure>

<style>
  figure { margin: 0; }
  figure.card { position: relative; display: grid; place-items: center; width: 100%; aspect-ratio: 16 / 9; overflow: hidden; background: var(--panel-2); }
  .mark-range { position: absolute; bottom: 0; height: 4px; min-width: 2px; background: var(--accent); pointer-events: none; }
  .hover-position { position: absolute; inset-block: 0; width: 1px; background: var(--accent); pointer-events: none; transform: translateX(-1px); }
  figure.card :global(canvas) { width: 100%; height: 100%; object-fit: contain; }
</style>
