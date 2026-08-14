<script lang="ts">
  import { onDestroy } from 'svelte'
  import {
    beginEditTransaction,
    clearLut,
    doUploadLut,
    endEditTransaction,
    state,
  } from '$lib/state/store.svelte.js'

  let picker: HTMLInputElement
  let intensityTransactionOpen = false
  let selected = $derived(Boolean(state.edit.lutId))
  let intensityPercent = $derived(Math.round(state.edit.lutIntensity * 100))
  let lutDimension = $derived(state.edit.lutSize ? `${state.edit.lutSize}×${state.edit.lutSize}×${state.edit.lutSize}` : '')
  let capability = $derived(state.capabilities?.filters?.find((option) => ['lut', 'lut3d', 'cube-lut'].includes(option.id.toLowerCase())))
  let unavailableReason = $derived(!state.capabilities ? '' : !capability
    ? 'Нужен обновлённый сервер с поддержкой 3D LUT'
    : capability.available ? '' : capability.reason || 'LUT недоступны в текущей сборке сервера')
  let controlsDisabled = $derived(state.lutUploading || Boolean(unavailableReason))

  function chooseFile(): void { if (!controlsDisabled) picker?.click() }
  async function onPick(event: Event): Promise<void> {
    const input = event.currentTarget as HTMLInputElement
    const file = input.files?.[0]
    input.value = ''
    if (file) await doUploadLut(file)
  }
  function setIntensity(event: Event): void {
    const value = Number((event.currentTarget as HTMLInputElement).value)
    state.edit.lutIntensity = Math.max(0, Math.min(100, value)) / 100
  }
  function beginIntensityTransaction(): void {
    if (controlsDisabled || intensityTransactionOpen) return
    intensityTransactionOpen = true
    beginEditTransaction('lut-intensity')
  }
  function endIntensityTransaction(): void {
    if (!intensityTransactionOpen) return
    intensityTransactionOpen = false
    endEditTransaction()
  }
  onDestroy(endIntensityTransaction)
</script>

<div class:is-unavailable={Boolean(unavailableReason)} class="color-tool lut-control" aria-busy={state.lutUploading}>
  <div class="color-tool-head">
    <div><h3>3D LUT</h3><p>Цветовой профиль в формате .cube</p></div>
    <span class="color-tool-badge">.cube</span>
  </div>
  <input bind:this={picker} class="hidden-file" type="file" accept=".cube,text/plain,application/octet-stream" disabled={controlsDisabled} aria-label="Выбрать LUT в формате CUBE" onchange={onPick} />
  {#if selected}
    <div class="lut-selected">
      <div class="lut-selected-copy">
        <strong title={state.edit.lutName || 'Загруженный LUT'}>{state.edit.lutName || 'Загруженный LUT'}</strong>
        <span>{lutDimension ? `Таблица ${lutDimension}` : 'Размер таблицы не указан'}</span>
      </div>
      <div class="lut-actions">
        <button type="button" class="btn ghost sm" disabled={controlsDisabled} onclick={chooseFile}>Заменить</button>
        <button type="button" class="btn ghost sm lut-remove" disabled={state.lutUploading} onclick={clearLut}>Удалить</button>
      </div>
    </div>
  {:else}
    <button type="button" class="btn ghost lut-upload" disabled={controlsDisabled} onclick={chooseFile}>{state.lutUploading ? 'Загружаю LUT…' : 'Загрузить .cube'}</button>
  {/if}
  {#if selected}
    <div class="lut-intensity">
      <div class="lut-intensity-head"><label for="lut-intensity">Интенсивность</label><output for="lut-intensity">{intensityPercent}%</output></div>
      <input
        id="lut-intensity"
        type="range"
        min="0"
        max="100"
        step="1"
        value={intensityPercent}
        disabled={controlsDisabled}
        aria-valuetext={`${intensityPercent}%`}
        oninput={setIntensity}
        onpointerdown={beginIntensityTransaction}
        onpointerup={endIntensityTransaction}
        onpointercancel={endIntensityTransaction}
        onfocus={beginIntensityTransaction}
        onblur={endIntensityTransaction}
      />
    </div>
  {/if}
  {#if state.lutUploading}<p class="lut-status" role="status">Проверяю и загружаю LUT…</p>{/if}
  {#if state.lutUploadError}<p class="error lut-error" role="alert">{state.lutUploadError}</p>{/if}
  {#if unavailableReason}<p class="lut-capability" role="status">{unavailableReason}</p>{:else}<p id="lut-preview-note" class="hint lut-note">LUT не отображается в предпросмотре; точный результат виден после экспорта.</p>{/if}
</div>
