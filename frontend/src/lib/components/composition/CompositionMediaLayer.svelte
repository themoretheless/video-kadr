<script lang="ts">
  interface Props {
    kind: 'video' | 'audio'
    url: string
    sourceTime: number
    playing: boolean
    muted?: boolean
    volume?: number
    playbackRate?: number
    class?: string
    style?: string
    onmediaerror?: () => void
    onseeklimited?: () => void
  }

  let {
    kind,
    url,
    sourceTime,
    playing,
    muted = false,
    volume = 1,
    playbackRate = 1,
    class: className = '',
    style = '',
    onmediaerror,
    onseeklimited,
  }: Props = $props()
  let element = $state<HTMLMediaElement | null>(null)
  let syncedUrl = ''
  const emptyCaptions = 'data:text/vtt;charset=utf-8,WEBVTT%0A%0A'

  function verifySeekTarget(): void {
    if (!element || !onseeklimited) return
    const target = Math.max(0, sourceTime)
    if (!Number.isFinite(target)) return
    if (Number.isFinite(element.duration) && target > element.duration + 0.12) {
      onseeklimited()
      return
    }
    const ranges = element.seekable
    if (!ranges.length) return
    for (let index = 0; index < ranges.length; index += 1) {
      if (target >= ranges.start(index) - 0.12 && target <= ranges.end(index) + 0.12) return
    }
    onseeklimited()
  }

  function syncMedia(forceSeek = false): void {
    if (!element) return
    const urlChanged = syncedUrl !== url
    syncedUrl = url
    const target = Math.max(0, sourceTime)
    if (Number.isFinite(target) && (forceSeek || urlChanged || Math.abs(element.currentTime - target) > 0.12)) {
      try {
        element.currentTime = target
        verifySeekTarget()
      } catch {
        // A fresh URL may reject seeks until metadata arrives; loadedmetadata
        // retries below. Only report a real limitation once metadata exists.
        if (element.readyState > 0) onseeklimited?.()
      }
    }
    element.muted = muted
    element.volume = Math.max(0, Math.min(volume, 1))
    element.playbackRate = Math.max(0.05, Math.min(playbackRate, 16))
    if (playing) void element.play().catch(() => undefined)
    else element.pause()
  }

  function handleLoadedMetadata(): void {
    syncMedia(true)
  }

  $effect(() => {
    syncMedia()
  })
</script>

{#if kind === 'video'}
  <video
    bind:this={element}
    class={className}
    {style}
    src={url}
    playsinline
    preload="metadata"
    aria-label="Предпросмотр видеоклипа"
    onerror={onmediaerror}
    onloadedmetadata={handleLoadedMetadata}
    onseeked={verifySeekTarget}
  ><track kind="captions" srclang="ru" label="Без субтитров" src={emptyCaptions} /></video>
{:else}
  <audio
    bind:this={element}
    src={url}
    preload="metadata"
    aria-label="Предпросмотр аудиоклипа"
    onerror={onmediaerror}
    onloadedmetadata={handleLoadedMetadata}
    onseeked={verifySeekTarget}
  ></audio>
{/if}
