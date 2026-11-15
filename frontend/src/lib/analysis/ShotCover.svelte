<script lang="ts">
  import { getLibraryAnalysisFrame } from '../api.js'

  let { sourceId, sourceTick, token, spaceId, label, onseek }: {
    sourceId: string
    sourceTick: number
    token?: string | null
    spaceId?: string | null
    label: string
    onseek?: () => void
  } = $props()

  let figure = $state<HTMLDivElement>()
  let visible = $state(false)
  let url = $state('')
  let failed = $state(false)

  $effect(() => {
    const node = figure
    if (!node || typeof IntersectionObserver === 'undefined') return
    const observer = new IntersectionObserver(entries => {
      visible = entries.some(entry => entry.isIntersecting)
    })
    observer.observe(node)
    return () => observer.disconnect()
  })

  $effect(() => {
    if (!visible) return
    const controller = new AbortController()
    const signal = controller.signal
    let ownedUrl: string | undefined
    failed = false
    void getLibraryAnalysisFrame(sourceId, sourceTick, token, spaceId, signal).then(frame => {
      if (signal.aborted) return
      if (frame.type !== 'image/png') throw new Error('Некорректный формат кадра')
      ownedUrl = URL.createObjectURL(frame)
      url = ownedUrl
    }).catch(() => {
      if (!signal.aborted) failed = true
    })
    return () => {
      controller.abort()
      if (ownedUrl) URL.revokeObjectURL(ownedUrl)
      url = ''
    }
  })
</script>

<div class="shot-cover" bind:this={figure}>
  {#if url}
    {#if onseek}
      <button type="button" aria-label={`Перейти к кадру · ${label}`} onclick={onseek}>
        <img src={url} alt="" />
      </button>
    {:else}
      <img src={url} alt={label} />
    {/if}
  {:else if visible && failed}
    <small>Кадр недоступен</small>
  {/if}
</div>

<style>
  .shot-cover { aspect-ratio: 16 / 9; width: min(100%, 10rem); background: var(--panel-2); }
  button { display: block; width: 100%; height: 100%; padding: 0; border: 0; background: transparent; cursor: pointer; }
  img { display: block; width: 100%; height: 100%; object-fit: contain; }
</style>
