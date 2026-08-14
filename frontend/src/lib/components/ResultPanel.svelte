<script lang="ts">
  import ProgressBar from './ProgressBar.svelte'
  import { cancelExport, state } from '$lib/state/store.svelte.js'

  let downloadName = $derived.by(() => {
    const extension = state.result?.filename?.split('.').pop() || 'mp4'
    const title = state.video?.title?.trim()
    if (!title) return `edited.${extension}`
    const safe = title.replace(/[\\/:*?"<>|]+/g, ' ').replace(/\s+/g, ' ').trim().slice(0, 60)
    return `${safe || 'edited'}.${extension}`
  })
</script>

<div class="card result">
  <h2>Результат</h2>
  {#if state.exporting}
    <ProgressBar progress={state.exportProgress} stage={state.exportStage} cancellable oncancel={() => void cancelExport()} />
  {/if}
  {#if state.exportError}<p class="error">Ошибка: {state.exportError}</p>{/if}
  {#if state.result && !state.exporting}
    <video class="player" src={state.result.url} controls playsinline><track kind="captions" /></video>
    <a class="btn primary big" href={state.result.url} download={downloadName}>Скачать результат</a>
  {/if}
</div>
