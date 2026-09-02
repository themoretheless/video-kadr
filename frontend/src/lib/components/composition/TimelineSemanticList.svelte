<script lang="ts">
  import { clipDurationTicks, COMPOSITION_TIME_BASE, type CompositionTrack } from '$lib/composition/types.js'

  interface Props {
    tracks: readonly CompositionTrack[]
    selectedClipId: string | null
    clipLabel: (clip: CompositionTrack['clips'][number]) => string
    onselect: (trackId: string, clipId: string) => void
  }

  let { tracks, selectedClipId, clipLabel, onselect }: Props = $props()
</script>

<section class="timeline-semantic-list" aria-label="Список клипов монтажной линии">
  <h3>Клипы по дорожкам</h3>
  <p>Семантическое представление синхронизировано с визуальной монтажной линией.</p>
  {#each tracks as track (track.id)}
    <h4>{track.name} · {track.kind}</h4>
    <ol>
      {#each track.clips as clip (clip.id)}
        <li>
          <button
            type="button"
            aria-current={selectedClipId === clip.id ? 'true' : undefined}
            onclick={() => onselect(track.id, clip.id)}
          >
            {clipLabel(clip)} — {(clip.timelineStartTicks / COMPOSITION_TIME_BASE).toFixed(2)}s,
            длительность {(clipDurationTicks(clip) / COMPOSITION_TIME_BASE).toFixed(2)}s
          </button>
        </li>
      {:else}
        <li>Нет клипов</li>
      {/each}
    </ol>
  {:else}
    <p>Нет дорожек</p>
  {/each}
</section>
