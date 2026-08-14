<script lang="ts">
  import { deleteFromLibrary, openFromLibrary, state } from '$lib/state/store.svelte.js'
  import type { MediaEntry } from '$lib/types.js'

  const label = (entry: MediaEntry) => entry.title?.trim() || entry.filename
  function fmtDuration(time?: number | null): string {
    if (!time || !Number.isFinite(time)) return ''
    return `${Math.floor(time / 60)}:${Math.floor(time % 60).toString().padStart(2, '0')}`
  }
  function fmtSize(bytes?: number | null): string {
    if (!bytes) return ''
    if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} ГБ`
    if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} МБ`
    return `${Math.max(1, Math.round(bytes / 1024))} КБ`
  }
  function meta(entry: MediaEntry): string {
    return [entry.width && entry.height ? `${entry.width}×${entry.height}` : '', fmtDuration(entry.duration), fmtSize(entry.sizeBytes)]
      .filter(Boolean).join(' · ')
  }
  const ext = (entry: MediaEntry) => entry.filename.split('.').pop()?.toUpperCase() || ''
</script>

{#if state.library.length}
  <div class="card library">
    <h2>Медиатека</h2>
    <ul class="lib-list">
      {#each state.library as entry (entry.id)}
        <li class="lib-item">
          <span class={`lib-badge ${entry.kind}`}>{entry.kind === 'output' ? 'результат' : 'источник'}</span>
          <div class="lib-info">
            <div class="lib-name" title={label(entry)}>{label(entry)}</div>
            <div class="lib-meta">{ext(entry)}{meta(entry) ? ` · ${meta(entry)}` : ''}</div>
          </div>
          <div class="lib-actions">
            {#if entry.kind === 'source'}
              <button class="btn ghost sm" onclick={() => openFromLibrary(entry)}>Открыть</button>
            {:else}
              <a class="btn ghost sm" href={entry.url} download={entry.filename}>Скачать</a>
            {/if}
            <button class="btn ghost sm danger" title="Удалить" onclick={() => void deleteFromLibrary(entry.id)}>✕</button>
          </div>
        </li>
      {/each}
    </ul>
  </div>
{/if}
