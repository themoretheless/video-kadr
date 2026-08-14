<script lang="ts">
  import ProgressBar from './ProgressBar.svelte'
  import { cancelImport, doImport, doUpload, state as appState } from '$lib/state/store.svelte.js'

  let picker: HTMLInputElement
  let dragover = $state(false)

  function onPick(event: Event): void {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    if (file) void doUpload(file)
    input.value = ''
  }

  function onDrop(event: DragEvent): void {
    event.preventDefault()
    dragover = false
    const file = event.dataTransfer?.files?.[0]
    if (file) void doUpload(file)
  }
</script>

<div
  class:dragover
  class="card import"
  role="region"
  aria-label="Импорт видео"
  ondragover={(event) => { event.preventDefault(); dragover = true }}
  ondragleave={(event) => { event.preventDefault(); dragover = false }}
  ondrop={onDrop}
>
  <div class="row">
    <input
      bind:value={appState.url}
      class="url-input"
      type="url"
      placeholder="https://vkvideo.ru/video-220018529_456248395"
      disabled={appState.importing}
      onkeydown={(event) => { if (event.key === 'Enter') void doImport() }}
    />
    <button class="btn primary" disabled={appState.importing || !appState.url.trim()} onclick={() => void doImport()}>
      {appState.importing ? 'Загрузка…' : 'Импорт'}
    </button>
  </div>
  <div class="range-row">
    <label>с <input bind:value={appState.importStart} class="time-input" placeholder="0:30" disabled={appState.importing} /></label>
    <label>по <input bind:value={appState.importEnd} class="time-input" placeholder="2:00" disabled={appState.importing} /></label>
    <span class="hint">диапазон импорта (мм:сс). Пусто = всё видео, для длинных роликов укажи отрезок</span>
  </div>
  <div class="import-or"><span>или</span></div>
  <button type="button" class="dropzone" disabled={appState.importing} onclick={() => picker?.click()}>
    <input bind:this={picker} type="file" accept="video/*" class="hidden-file" onchange={onPick} />
    <span class="dropzone-icon">📁</span>
    <span>Перетащи видеофайл сюда или нажми, чтобы выбрать</span>
  </button>
  {#if appState.importing}
    <ProgressBar
      class="import-progress"
      progress={appState.importProgress}
      stage={appState.importStage}
      cancellable={appState.importStage !== 'uploading'}
      oncancel={() => void cancelImport()}
    />
  {/if}
  {#if appState.importError}<p class="error">Ошибка: {appState.importError}</p>{/if}
</div>
