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
  const markerColors: Array<{ name: string; swatch: string; labels: FilmcraftLabel[] }> = [
    { name: 'Зелёные', swatch: '#758542', labels: ['Green', 'Forest'] },
    { name: 'Красные', swatch: '#c13c3d', labels: ['Rose', 'Magenta'] },
    { name: 'Фиолетовые', swatch: '#a98caf', labels: ['Purple', 'Violet', 'Lavender'] },
    { name: 'Оранжевые', swatch: '#da7639', labels: ['Mango', 'Brown', 'Tan'] },
    { name: 'Жёлтые', swatch: '#c9a344', labels: ['Yellow'] },
    { name: 'Синие', swatch: '#568bf4', labels: ['Blue', 'Iris', 'Cerulean'] },
    { name: 'Бирюзовые', swatch: '#72f0d7', labels: ['Teal', 'Caribbean'] },
  ]
  let hiddenColors = $state<string[]>([])
  const markerColor = (label: FilmcraftLabel) => markerColors.find(group => group.labels.includes(label)) ?? markerColors[0]
  const visibleMarkers = $derived(markers.filter(item => !hiddenColors.includes(markerColor(item.color).name)))
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

  function toggleColor(group: string) {
    hiddenColors = hiddenColors.includes(group)
      ? hiddenColors.filter(name => name !== group)
      : [...hiddenColors, group]
  }

  function seekMarker(item: FilmcraftSequenceMarker) {
    onseek({ sequence: sequenceId, time: item.start, timeFrames: item.frame, timecode: item.timecode })
  }

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
  <div class="row" role="group" aria-label="Фильтр цветов маркеров">
    {#each markerColors as group (group.name)}
      <button type="button" class="color-chip" style:--marker-color={group.swatch}
        aria-label={`${group.name} маркеры`} title={`${group.name} маркеры`}
        aria-pressed={!hiddenColors.includes(group.name)} onclick={() => toggleColor(group.name)}></button>
    {/each}
    <button type="button" disabled={!hiddenColors.length} onclick={() => { hiddenColors = [] }}>Показать все цвета</button>
  </div>
  {#if visibleMarkers.length}
    <ul class="marker-list" aria-label="Видимые маркеры таймлайна">
      {#each visibleMarkers as item (item.id)}
        <li>
          <button type="button" style:--marker-color={markerColor(item.color).swatch} disabled={disabled || busy}
            onclick={() => { selected = item.id; seekMarker(item) }}>
            {item.timecode} · {item.name || item.kind}
          </button>
        </li>
      {/each}
    </ul>
  {:else if markers.length}
    <p role="status">Все маркеры скрыты фильтром по цвету.</p>
  {/if}
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
      if (marker) seekMarker(marker)
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
  .color-chip { inline-size: 1.5rem; block-size: 1.5rem; padding: 0; border: 2px solid var(--marker-color); background: transparent; }
  .color-chip[aria-pressed="true"] { background: var(--marker-color); }
  .marker-list { display: grid; gap: 0.25rem; max-block-size: 12rem; overflow: auto; padding: 0.2rem; list-style: none; }
  .marker-list button { border-inline-start: 0.3rem solid var(--marker-color); text-align: start; }
  summary { cursor: pointer; }
</style>
