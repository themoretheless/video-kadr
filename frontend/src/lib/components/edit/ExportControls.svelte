<script lang="ts">
  import {
    doExport,
    hasMeaningfulChanges,
    selectedExportUnavailableReason,
    state as appState,
  } from '$lib/state/store.svelte.js'

  type Platform = 'telegram' | 'shorts' | 'reels' | 'youtube'
  interface Props { onplatform?: (platform: Platform) => void }
  let { onplatform }: Props = $props()
  let showNoopWarning = $state(false)
  let previousEditKey = ''

  const formats = [
    { value: 'mp4', label: 'MP4' }, { value: 'webm', label: 'WebM' },
    { value: 'av1', label: 'AV1' }, { value: 'prores', label: 'ProRes' },
    { value: 'gif', label: 'GIF' }, { value: 'png', label: 'Кадр PNG' },
    { value: 'jpg', label: 'Кадр JPG' }, { value: 'mp3', label: 'Аудио MP3' },
  ]
  const qualityTiers = [
    { value: '', label: 'Авто' }, { value: 'high', label: 'Высокое' },
    { value: 'medium', label: 'Среднее' }, { value: 'compact', label: 'Компактное' },
  ]
  let formatHint = $derived.by(() => ({
    gif: 'GIF без звука, по умолчанию 12 fps. Лучше выбрать размер и короткий отрезок.',
    png: 'Один кадр на позиции начала обрезки, без звука.',
    jpg: 'Один кадр JPG на позиции начала обрезки, без звука.',
    mp3: 'Только звук, видеоэффекты игнорируются.',
    webm: 'VP9 + Opus: меньше размер, дольше кодируется.',
    av1: 'AV1 даёт компактный файл, но кодируется медленно.',
    prores: 'ProRes 422 HQ для монтажа: крупный MOV-файл со звуком PCM.',
  }[appState.edit.format] ?? ''))
  let showQuality = $derived(['mp4', 'webm', 'av1'].includes(appState.edit.format))
  let exportUnavailable = $derived(selectedExportUnavailableReason())
  const formatCapability = (id: string) => appState.capabilities?.formats.find((option) => option.id === id)
  const codecCapability = (id: string) => appState.capabilities?.codecs.find((option) => option.id === id)
  function unavailableReason(kind: 'format' | 'codec', id: string): string | undefined {
    const option = kind === 'format' ? formatCapability(id) : codecCapability(id)
    return option && !option.available ? option.reason || 'Недоступно в текущей сборке' : undefined
  }
  function selectFormat(id: string): void {
    if (!unavailableReason('format', id)) appState.edit.format = id
  }
  function selectCodec(id: string): void {
    if (!unavailableReason('codec', id)) appState.edit.codec = id
  }
  function requestExport(): void {
    if (!hasMeaningfulChanges()) showNoopWarning = true
    else void doExport()
  }
  function exportUnchangedCopy(): void {
    showNoopWarning = false
    void doExport()
  }
  $effect(() => {
    const editKey = JSON.stringify(appState.edit)
    if (previousEditKey && previousEditKey !== editKey) showNoopWarning = false
    previousEditKey = editKey
  })
</script>

<section class="group export-group">
  <div class="group-title">Экспорт</div>
  <div class="field">
    <span class="field-label">Формат</span>
    <div class="chips" role="group" aria-label="Формат экспорта">
      {#each formats as format (format.value)}
        <button
          type="button"
          class:active={appState.edit.format === format.value}
          class="chip"
          aria-pressed={appState.edit.format === format.value}
          aria-disabled={formatCapability(format.value)?.available === false}
          aria-label={unavailableReason('format', format.value) ? `${format.label}. ${unavailableReason('format', format.value)}` : format.label}
          title={unavailableReason('format', format.value)}
          onclick={() => selectFormat(format.value)}
        >{format.label}</button>
      {/each}
    </div>
  </div>
  {#if appState.edit.format === 'mp4'}
    <div class="field">
      <span class="field-label">Кодек</span>
      <div class="chips" role="group" aria-label="Кодек экспорта">
        {#each [{ id: 'h264', label: 'H.264' }, { id: 'h265', label: 'H.265' }] as codec (codec.id)}
          <button
            type="button"
            class:active={appState.edit.codec === codec.id}
            class="chip"
            aria-pressed={appState.edit.codec === codec.id}
            aria-disabled={codecCapability(codec.id)?.available === false}
            aria-label={unavailableReason('codec', codec.id) ? `${codec.label}. ${unavailableReason('codec', codec.id)}` : codec.label}
            title={unavailableReason('codec', codec.id)}
            onclick={() => selectCodec(codec.id)}
          >{codec.label}</button>
        {/each}
      </div>
    </div>
  {/if}
  {#if showQuality}
    <div class="field">
      <span class="field-label">Качество</span>
      <div class="chips" role="group" aria-label="Качество экспорта">
        {#each qualityTiers as quality (quality.value)}
          <button type="button" class:active={appState.edit.qualityTier === quality.value} class="chip" aria-pressed={appState.edit.qualityTier === quality.value} onclick={() => { appState.edit.qualityTier = quality.value }}>{quality.label}</button>
        {/each}
      </div>
    </div>
  {/if}
  <div class="field">
    <span class="field-label">Под платформу</span>
    <div class="chips" role="group" aria-label="Платформа публикации">
      <button type="button" class="chip" onclick={() => onplatform?.('telegram')}>Telegram</button>
      <button type="button" class="chip" onclick={() => onplatform?.('shorts')}>Shorts</button>
      <button type="button" class="chip" onclick={() => onplatform?.('reels')}>Reels</button>
      <button type="button" class="chip" onclick={() => onplatform?.('youtube')}>YouTube</button>
    </div>
  </div>
  {#if formatHint}<p class="hint">{formatHint}</p>{/if}
</section>

{#if showNoopWarning}
  <div class="export-warning" role="alert" aria-live="assertive">
    <div><strong>Изменений пока нет</strong><p>Экспорт создаст копию исходного файла с повторным кодированием.</p></div>
    <div class="export-warning-actions">
      <button type="button" class="btn ghost sm" onclick={() => { showNoopWarning = false }}>Вернуться</button>
      <button type="button" class="btn primary sm" onclick={exportUnchangedCopy}>Создать копию</button>
    </div>
  </div>
{/if}
<button type="button" class="btn primary big export-submit" disabled={appState.exporting || Boolean(exportUnavailable)} title={exportUnavailable || undefined} onclick={requestExport}>
  {appState.exporting ? 'Обработка…' : 'Экспортировать'}
</button>
