<script lang="ts">
  import { sliceWaveformBuckets, waveformPath } from './waveform.js'
  import { localWaveformCache, type LocalWaveformCache } from './waveformCache.js'

  interface Props {
    url: string
    sourceInSeconds?: number
    sourceOutSeconds?: number
    label?: string
    cache?: LocalWaveformCache
  }

  let {
    url,
    sourceInSeconds = 0,
    sourceOutSeconds = Number.POSITIVE_INFINITY,
    label = 'Форма звуковой волны',
    cache = localWaveformCache,
  }: Props = $props()

  let path = $state('')
  let unavailable = $state(false)
  let requestVersion = 0

  $effect(() => {
    const version = ++requestVersion
    const requestedUrl = url
    const requestedStart = sourceInSeconds
    const requestedEnd = sourceOutSeconds
    path = ''
    unavailable = false
    void Promise.resolve().then(() => cache.load(requestedUrl)).then((summary) => {
      if (version !== requestVersion) return
      const start = Math.max(0, requestedStart) / summary.durationSeconds
      const end = Math.min(summary.durationSeconds, requestedEnd) / summary.durationSeconds
      path = waveformPath(sliceWaveformBuckets(summary.buckets, start, end, 128), 100, 32)
      unavailable = !path
    }).catch(() => {
      if (version === requestVersion) unavailable = true
    })
    return () => { requestVersion += 1 }
  })
</script>

<svg
  class="audio-waveform"
  class:unavailable
  viewBox="0 0 100 32"
  preserveAspectRatio="none"
  role="img"
  aria-label={unavailable ? `${label}: недоступна` : label}
>
  <line x1="0" y1="16" x2="100" y2="16"></line>
  {#if path}<path d={path}></path>{/if}
</svg>

<style>
  .audio-waveform {
    display: block;
    width: 100%;
    height: 100%;
    min-height: 18px;
    color: currentColor;
    opacity: 0.68;
    pointer-events: none;
  }

  line {
    stroke: currentColor;
    stroke-width: 0.35;
    opacity: 0.35;
  }

  path { fill: currentColor; }
  .unavailable { opacity: 0.25; }
</style>
