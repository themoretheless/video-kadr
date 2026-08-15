<script lang="ts">
  import { detectWaveformBeats, type BeatDetectionResult } from '$lib/audio/beats.js'
  import { mapDetectedBeatsToTimeline, type TimelineBeat } from '$lib/audio/beatMarkers.js'
  import { localWaveformCache, type LocalWaveformCache } from '$lib/audio/waveformCache.js'
  import type { AudioClip, Composition, VideoClip } from '$lib/composition/types.js'

  type BeatSourceClip = AudioClip | VideoClip

  interface Props {
    document: Composition
    media: Readonly<Record<string, { readonly url: string }>>
    waveformCache?: Pick<LocalWaveformCache, 'load'>
    onapply: (beats: readonly TimelineBeat[], estimatedBpm: number | null) => void | Promise<void>
  }

  let { document, media, waveformCache = localWaveformCache, onapply }: Props = $props()
  let clipId = $state('')
  let sensitivity = $state(0.5)
  let minimumSpacingSeconds = $state(0.2)
  let detection = $state<BeatDetectionResult | null>(null)
  let timelineBeats = $state<readonly TimelineBeat[]>([])
  let busy = $state(false)
  let message = $state('')
  let error = $state('')

  const candidates = $derived.by(() => document.tracks.flatMap((track) => {
    if ((track.kind === 'audio' || track.kind === 'video') && track.muted) return []
    return track.clips.flatMap((clip): BeatSourceClip[] => {
      if (clip.kind === 'audio') return document.sources[clip.sourceId]?.hasAudio && media[clip.sourceId]?.url ? [clip] : []
      if (clip.kind === 'video' && clip.sourceAudioEnabled && (clip.playbackMode?.mode ?? 'forward') === 'forward' &&
        document.sources[clip.sourceId]?.hasAudio && media[clip.sourceId]?.url) return [clip]
      return []
    })
  }))

  $effect(() => {
    const available = candidates
    if (!available.some((clip) => clip.id === clipId)) clipId = available[0]?.id ?? ''
  })

  function reset(): void {
    detection = null
    timelineBeats = []
    message = ''
    error = ''
  }

  async function analyze(): Promise<void> {
    if (busy) return
    const clip = candidates.find((candidate) => candidate.id === clipId)
    if (!clip) return
    const url = media[clip.sourceId]?.url
    if (!url) return
    busy = true
    reset()
    try {
      const summary = await waveformCache.load(url)
      const nextDetection = detectWaveformBeats(summary, { sensitivity, minimumSpacingSeconds })
      const nextTimelineBeats = mapDetectedBeatsToTimeline(document, clip.id, nextDetection)
      detection = nextDetection
      timelineBeats = nextTimelineBeats
      message = nextTimelineBeats.length
        ? `Найдено маркеров: ${nextTimelineBeats.length}${nextDetection.estimatedBpm ? ` · ≈ ${nextDetection.estimatedBpm} BPM` : ''}`
        : 'В выбранном диапазоне не найдено надёжных атак.'
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught)
    } finally {
      busy = false
    }
  }

  async function apply(): Promise<void> {
    if (busy || !detection || !timelineBeats.length) return
    busy = true
    error = ''
    try {
      await onapply(timelineBeats, detection.estimatedBpm)
      message = `Добавлено Auto Beat markers: ${timelineBeats.length}.`
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught)
    } finally {
      busy = false
    }
  }
</script>

<details class="composition-tool-section auto-beat-panel" aria-label="Auto Beat markers">
  <summary>Auto Beat — локальные маркеры</summary>
  <div class="composition-tool-body" aria-busy={busy}>
    <p>Классический energy-flux detector анализирует локальную waveform без ASR, моделей и сети.</p>
    {#if candidates.length}
      <div class="auto-beat-controls">
        <label>
          Audio source clip
          <select bind:value={clipId} onchange={reset} disabled={busy}>
            {#each candidates as clip (clip.id)}<option value={clip.id}>{clip.id} · {clip.kind}</option>{/each}
          </select>
        </label>
        <label>
          Чувствительность
          <input aria-label="Чувствительность Auto Beat" type="range" min="0" max="1" step="0.05" bind:value={sensitivity} oninput={reset} disabled={busy} />
        </label>
        <label>
          Минимальный интервал, с
          <input aria-label="Минимальный интервал Auto Beat" type="number" min="0.1" max="2" step="0.05" bind:value={minimumSpacingSeconds} onchange={reset} disabled={busy} />
        </label>
      </div>
      <div class="composition-tool-actions">
        <button class="btn ghost sm" type="button" disabled={busy} onclick={() => void analyze()}>{busy && !detection ? 'Анализирую…' : 'Найти биты'}</button>
        <button class="btn primary sm" type="button" disabled={busy || !timelineBeats.length} onclick={() => void apply()}>Добавить markers</button>
      </div>
    {:else}
      <p>Добавьте локальный audio clip или forward video clip с подтверждённой source audio.</p>
    {/if}
    {#if message}<p role="status">{message}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
</details>

<style>
  .auto-beat-controls {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: 0.55rem;
  }

  .auto-beat-controls label,
  .auto-beat-controls select,
  .auto-beat-controls input {
    min-width: 0;
    width: 100%;
  }
</style>
