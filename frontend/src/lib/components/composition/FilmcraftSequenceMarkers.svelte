<script lang="ts">
  import { editFilmcraftSequenceMarker, navigateFilmcraftSequenceMarker, FILMCRAFT_LABELS, type FilmcraftImportResult, type FilmcraftLabel, type FilmcraftMarkerPosition, type FilmcraftSequenceMarker, type FilmcraftSequenceMarkerEditCommand, type FilmcraftSourceEditFence } from '$lib/api'

  let { document, sequenceId, clipId, token, markers = [], timeTicks, timeFrames, disabled, onedit, onseek }: {
    document: string; sequenceId: string; clipId: string; token: string | null
    markers?: FilmcraftSequenceMarker[]; timeTicks?: string; timeFrames: string; disabled: boolean
    onedit: (project: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean
    onseek: (position: FilmcraftMarkerPosition) => void
  } = $props()
  const modes = [
    ['markers.add', 'Обычный'], ['markers.addRange', 'Диапазон'], ['markers.addRangeInOut', 'Диапазон In/Out'],
    ['markers.addChapter', 'Глава'], ['markers.addFlashCue', 'Flash Cue'],
  ] as const
  let mode = $state<FilmcraftSequenceMarkerEditCommand>('markers.add')
  let selected = $state('')
  let name = $state('')
  let comment = $state('')
  let color = $state<FilmcraftLabel>('Green')
  let duration = $state('')
  let busy = $state(false)
  let error = $state('')
  let request: AbortController | undefined
  const marker = $derived(markers.find(candidate => candidate.id === selected))
  const ready = $derived(token && sequenceId && !disabled && !busy)
  const validDuration = $derived(duration === '' || /^\d{1,19}$/.test(duration))
  $effect(() => () => request?.abort())
  $effect(() => {
    name = marker?.name ?? ''
    comment = marker?.comment ?? ''
    color = marker?.color ?? 'Green'
    duration = marker?.durationFrames ?? ''
  })

  async function run(command: FilmcraftSequenceMarkerEditCommand | 'markers.goNext' | 'markers.goPrev') {
    if (!token || !ready) return
    const owner = token
    const expected = { document, sequenceId, clipId, token: owner, timeFrames, timeTicks: timeTicks ?? null }
    const input = { document, sequence: sequenceId, ...(timeTicks ? { time: timeTicks } : { frame: timeFrames }) }
    const controller = new AbortController()
    request = controller
    busy = true
    error = ''
    try {
      if (command === 'markers.goNext' || command === 'markers.goPrev') {
        const result = await navigateFilmcraftSequenceMarker(owner, command, input, controller.signal)
        if (result && !controller.signal.aborted && document === expected.document && sequenceId === expected.sequenceId
          && token === owner && timeFrames === expected.timeFrames && (timeTicks ?? null) === expected.timeTicks) onseek(result)
      } else {
        const fields = command === 'markers.clearAll' || command === 'markers.clearCurrent' ? {}
          : { name: command === 'markers.edit' || name ? name : undefined, comment, color,
            durationFrames: duration && (command !== 'markers.edit' || duration !== marker?.durationFrames) ? duration : undefined }
        const result = await editFilmcraftSequenceMarker(owner, command, {
          ...input, ...fields, marker: command === 'markers.edit' || command === 'markers.clearCurrent' ? marker?.id : undefined,
        }, controller.signal)
        if (!controller.signal.aborted && onedit(result.project, expected)) selected = result.marker ?? ''
      }
    } catch (failure) {
      if (!controller.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
    } finally {
      if (request === controller) { request = undefined; busy = false }
    }
  }
</script>

<details>
  <summary>Маркеры таймлайна Filmcraft</summary>
  <div class="row">
    <label>Маркер
      <select aria-label="Маркер таймлайна" bind:value={selected} disabled={busy}>
        <option value="">Новый маркер</option>
        {#each markers as item (item.id)}
          <option value={item.id}>{item.timecode} · {item.name || item.kind}</option>
        {/each}
      </select>
    </label>
    <button type="button" disabled={!marker} onclick={() => {
      if (marker) onseek({ sequence: sequenceId, time: marker.start, timeFrames: marker.frame, timecode: marker.timecode })
    }}>К выбранному маркеру</button>
    <button type="button" disabled={!ready || !markers.length} onclick={() => run('markers.goPrev')}>Предыдущий маркер</button>
    <button type="button" disabled={!ready || !markers.length} onclick={() => run('markers.goNext')}>Следующий маркер</button>
  </div>
  <div class="row">
    {#if !marker}
      <label>Тип маркера
        <select aria-label="Тип маркера" bind:value={mode} disabled={busy}>
          {#each modes as [value, label] (value)}<option {value}>{label}</option>{/each}
        </select>
      </label>
    {/if}
    <label>Имя маркера <input bind:value={name} disabled={busy} /></label>
    <label>Комментарий маркера <input bind:value={comment} disabled={busy} /></label>
    <label>Цвет маркера
      <select aria-label="Цвет маркера" bind:value={color} disabled={busy}>
        {#each FILMCRAFT_LABELS as label (label)}<option value={label}>{label}</option>{/each}
      </select>
    </label>
    {#if marker || mode !== 'markers.addChapter' && mode !== 'markers.addRangeInOut'}
      <label>Длительность маркера, кадров <input inputmode="numeric" bind:value={duration} disabled={busy} /></label>
    {/if}
    {#if marker}
      <button type="button" disabled={!ready || !validDuration} onclick={() => run('markers.edit')}>Сохранить маркер</button>
      <button type="button" disabled={!ready} onclick={() => run('markers.clearCurrent')}>Удалить маркер</button>
    {:else}
      <button type="button" disabled={!ready || !validDuration} onclick={() => run(mode)}>Добавить маркер</button>
    {/if}
    <button type="button" disabled={!ready || !markers.length} onclick={() => run('markers.clearAll')}>Удалить все маркеры</button>
  </div>
  {#if error}<p role="alert">{error}</p>{/if}
</details>

<style>
  .row { display: flex; flex-wrap: wrap; align-items: center; gap: 0.5rem; margin-block: 0.5rem; }
  summary { cursor: pointer; }
</style>
