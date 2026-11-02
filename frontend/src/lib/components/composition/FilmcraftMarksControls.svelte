<script lang="ts">
  import { editFilmcraftSource, type FilmcraftSourcePlacementCommand, navigateFilmcraft, type FilmcraftNavigationCommand, type FilmcraftNavigationResult, editFilmcraftMarks, type FilmcraftImportResult, type FilmcraftMarks, type FilmcraftMarksCommand, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, items, marks, timeFrames, timeTicks, stillDuration = $bindable('5'), stillUnit = $bindable<'seconds' | 'frames'>('seconds'), tracks = [], rippleMarkers = $bindable(false), item = $bindable(''), sourcePlayhead = '0', selection = [], projectSelection = [], linkedSelection = true, disabled = false, onedit, oncontext }: {
    document: string; sequenceId: string; clipId: string; token: string | null; items: NonNullable<FilmcraftImportResult['projectItems']>
    stillDuration?: string; stillUnit?: 'seconds' | 'frames'
    tracks?: NonNullable<FilmcraftImportResult['sequences'][number]['tracks']>; rippleMarkers?: boolean
    item?: string; sourcePlayhead?: string; selection?: string[]; projectSelection?: string[]; linkedSelection?: boolean
    oncontext?: (result: FilmcraftNavigationResult, expected: FilmcraftSourceEditFence) => boolean
    marks?: FilmcraftMarks; timeFrames: string; timeTicks?: string; disabled?: boolean; onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let target = $state<'program' | 'source'>('program')
  let follows = $state(false)
  let position = $state('0')
  let useMonitorPosition = $state(false)
  let unit = $state<'ticks' | 'frames' | 'seconds' | 'timecode'>('ticks')
  let seekPosition = $state('0')
  let seekUnit = $state<'ticks' | 'frames' | 'seconds'>('ticks')
  const validSeek = $derived(seekUnit === 'seconds' ? seekPosition.trim() !== '' && Number.isFinite(Number(seekPosition)) : validTick(seekPosition))
  let videoDest = $state('')
  let audioDest = $state('')
  let videoPatch = $state(true)
  let audioPatch = $state(true)
  let sequencesAsClips = $state(false)
  let mediaScaling = $state('none')
  const validStill = $derived(stillDuration.trim() !== '' && Number.isFinite(Number(stillDuration)))
  $effect(() => {
    if (videoDest && !tracks.some(track => track.kind === 'video_tracks' && track.id === videoDest)) videoDest = ''
    if (audioDest && !tracks.some(track => track.kind === 'audio_tracks' && track.id === audioDest)) audioDest = ''
  })
  let busy = $state(false)
  let busyLabel = $state('')
  let error = $state('')
  let status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function validTick(value: string) { return /^-?\d{1,19}$/.test(value) && BigInt(value) >= -9223372036854775808n && BigInt(value) <= 9223372036854775807n }
  const validPosition = $derived(unit === 'seconds' && target === 'program' ? position.trim() !== '' && Number.isFinite(Number(position)) : unit === 'timecode' && target === 'program' ? position.length > 0 : validTick(position))
  const validMark = $derived(useMonitorPosition && oncontext ? validTick(target === 'source' ? sourcePlayhead : timeTicks || timeFrames) : validPosition)
  const shown = $derived(target === 'source' ? items.find(source => source.id === item)?.sourceMarks : marks)
  const ready = $derived(!!token && !disabled && !busy && (target === 'program' ? !!sequenceId : items.some(i => i.id === item)))
  function context() { return JSON.stringify([document, sequenceId, clipId, token, items.map(i => i.id), timeFrames, timeTicks, disabled, target, item, position, unit, sourcePlayhead, selection, projectSelection, linkedSelection, follows, seekPosition, seekUnit, useMonitorPosition, tracks, videoDest, audioDest, videoPatch, audioPatch, sequencesAsClips, mediaScaling, stillDuration, stillUnit, rippleMarkers]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  async function run(command: FilmcraftMarksCommand, clear?: 'in' | 'out' | 'both') {
    if (!ready || !clear && !validMark) return
    const owner = token!
    const captured = context(); const current = new AbortController()
    const expected = { document, sequenceId, clipId, token: owner, timeFrames, timeTicks: timeTicks || null, ...(oncontext ? { sourceItem: item || null, sourcePlayhead } : {}) }
    const time = useMonitorPosition && oncontext ? (target === 'source' ? { time: sourcePlayhead } : timeTicks ? { time: timeTicks } : { frame: timeFrames }) : target === 'source' || unit === 'ticks' ? { time: position } : unit === 'frames' ? { frame: position } : unit === 'seconds' ? { seconds: Number(position) } : { timecode: position }
    const params = target === 'source' && clear ? { item, ...(clear !== 'out' ? { in: null } : {}), ...(clear !== 'in' ? { out: null } : {}) } : { target, ...time }
    controller = current; busy = true; busyLabel = 'Обновляются точки…'; error = ''; status = ''
    try {
      const result = await editFilmcraftMarks(owner, target === 'source' && clear ? 'project.setMarks' : command, { document, ...params, context: { activeSequence: sequenceId || null, sourceItem: target === 'source' ? item : null, ...(oncontext ? { sourcePlayhead } : {}) } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Точки In/Out обновлены'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
  async function navigate(command: FilmcraftNavigationCommand) {
    if (!token || disabled || busy || !oncontext || command.startsWith('source.') && !validStill || command === 'source.setPlayhead' && (!item || !validSeek)) return
    const captured = context(); const current = new AbortController()
    const expected = { document, sequenceId, clipId, token, timeFrames, timeTicks: timeTicks || null, sourceItem: item || null, sourcePlayhead, timelineSelection: [...selection], projectSelection: [...projectSelection] }
    const params = command === 'source.open' ? (item ? { item } : {}) : command === 'source.setPlayhead' ? (seekUnit === 'ticks' ? { time: seekPosition } : seekUnit === 'frames' ? { frame: seekPosition } : { seconds: Number(seekPosition) }) : { target }
    controller = current; busy = true; busyLabel = 'Обновляется позиция…'; error = ''; status = ''
    try {
      const result = await navigateFilmcraft(token, command, { document, ...params, context: { activeSequence: sequenceId || null, sourceItem: item || null, sourcePlayhead, ...(timeTicks ? { playhead: timeTicks } : { playheadFrames: timeFrames }), selection: [...selection], projectSelection: [...projectSelection], linkedSelection, selectionFollowsPlayhead: follows, stillImageDuration: Number(stillDuration), stillImageUnit: stillUnit } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = oncontext(result, expected) ? 'Позиция обновлена' : 'Результат не применён: контекст изменился'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }

  async function place(command: FilmcraftSourcePlacementCommand) {
    if (!token || disabled || busy || !sequenceId || !item || !items.some(source => source.id === item) || !validStill) return
    const captured = context(); const current = new AbortController()
    const expected = { document, sequenceId, clipId, token, timeFrames, timeTicks: timeTicks || null, sourceItem: item, sourcePlayhead, timelineSelection: [...selection], projectSelection: [...projectSelection] }
    const targeting = tracks.length ? { videoDest: videoPatch ? videoDest || tracks.find(track => track.kind === 'video_tracks')?.id || null : null, audioDest: audioPatch ? audioDest || tracks.find(track => track.kind === 'audio_tracks')?.id || null : null } : undefined
    controller = current; busy = true; busyLabel = command === 'source.insert' ? 'Вставляется исходник…' : 'Записывается исходник…'; error = ''; status = ''
    try {
      const result = await editFilmcraftSource(token, command, { document, context: { activeSequence: sequenceId, sourceItem: item, sourcePlayhead, ...(timeTicks ? { playhead: timeTicks } : { playheadFrames: timeFrames }), ...(targeting ? { targeting } : {}), sequencesAsClips, rippleSequenceMarkers: rippleMarkers, selectionFollowsPlayhead: follows, linkedSelection, defaultMediaScaling: mediaScaling, stillImageDuration: Number(stillDuration), stillImageUnit: stillUnit } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : command === 'source.insert' ? 'Исходник вставлен' : 'Исходник записан поверх'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }

</script>
<details>
  <summary>Точки In/Out</summary>
  <label>Монитор <select aria-label="Монитор точек Filmcraft" value={target} disabled={disabled || busy} onchange={event => { target = event.currentTarget.value as typeof target }}><option value="program">Таймлайн</option><option value="source">Исходник</option></select></label>
  {#if target === 'source'}<label>Элемент <select aria-label="Исходник точек Filmcraft" value={item} disabled={disabled || busy} onchange={event => { item = event.currentTarget.value }}><option value="">Выберите элемент</option>{#each items as source (source.id)}<option value={source.id}>{source.name || source.id}</option>{/each}</select></label>{/if}
  {#if shown}<dl aria-label="Текущие точки Filmcraft"><dt>In</dt><dd>{shown.in ?? '—'}</dd><dt>Out</dt><dd>{shown.out ?? '—'}</dd><dt>Видео In / Out</dt><dd>{shown.split.videoIn ?? '—'} / {shown.split.videoOut ?? '—'}</dd><dt>Аудио In / Out</dt><dd>{shown.split.audioIn ?? '—'} / {shown.split.audioOut ?? '—'}</dd></dl>{/if}
  {#if oncontext}<label><input type="checkbox" aria-label="По позиции монитора Filmcraft" bind:checked={useMonitorPosition} disabled={disabled || busy} />Ставить точки по текущей позиции монитора</label>{/if}
  <label>Позиция <input aria-label="Позиция точки Filmcraft" bind:value={position} disabled={disabled || busy} /></label>
  {#if target === 'program'}<label>Единицы <select aria-label="Единицы точки Filmcraft" value={unit} onchange={event => { unit = event.currentTarget.value as typeof unit }} disabled={disabled || busy}><option value="ticks">Тики</option><option value="frames">Кадры</option><option value="seconds">Секунды</option><option value="timecode">Таймкод</option></select></label>{:else}<span>Позиция в тиках</span>{/if}
  <button type="button" disabled={!ready || !validMark} onclick={() => run('markers.markIn')}>Поставить In</button>
  <button type="button" disabled={!ready || !validMark} onclick={() => run('markers.markOut')}>Поставить Out</button>
  <button type="button" disabled={!ready} onclick={() => run('markers.clearIn', 'in')}>Очистить In</button>
  <button type="button" disabled={!ready} onclick={() => run('markers.clearOut', 'out')}>Очистить Out</button>
  <button type="button" disabled={!ready} onclick={() => run('markers.clearInOut', 'both')}>Очистить In/Out</button>
  {#each [['markers.markSplitVideoIn', 'Видео In'], ['markers.markSplitVideoOut', 'Видео Out'], ['markers.markSplitAudioIn', 'Аудио In'], ['markers.markSplitAudioOut', 'Аудио Out']] as [command, label] (command)}
    <button type="button" disabled={!ready || !validMark} onclick={() => run(command as FilmcraftMarksCommand)}>{label}</button>
  {/each}
  {#if oncontext}
    <span aria-label="Позиция исходника Filmcraft">Исходник: {sourcePlayhead} тиков</span>
    {#if target === 'source'}
      <button type="button" disabled={disabled || busy || !token || !item && !projectSelection.length || !validStill} onclick={() => navigate('source.open')}>Открыть исходник</button>
      <label>Позиция перехода исходника <input aria-label="Позиция перехода исходника Filmcraft" bind:value={seekPosition} disabled={disabled || busy} /></label>
      <label>Единицы перехода исходника <select aria-label="Единицы перехода исходника Filmcraft" value={seekUnit} onchange={event => { seekUnit = event.currentTarget.value as typeof seekUnit }} disabled={disabled || busy}><option value="ticks">Тики</option><option value="frames">Кадры</option><option value="seconds">Секунды</option></select></label>
      <button type="button" disabled={disabled || busy || !token || !item || !validSeek || !validStill} onclick={() => navigate('source.setPlayhead')}>Перейти в исходнике</button>
      {#if tracks.length}
        <label><input type="checkbox" aria-label="Видео исходника Filmcraft" bind:checked={videoPatch} disabled={disabled || busy} />Видео</label>
        <label>Видеодорожка <select aria-label="Видеодорожка исходника Filmcraft" value={videoDest} disabled={disabled || busy || !videoPatch} onchange={event => { videoDest = event.currentTarget.value }}><option value="">Первая видеодорожка</option>{#each tracks.filter(track => track.kind === 'video_tracks') as track (track.id)}<option value={track.id}>{track.name}{track.locked ? ' (заблокирована)' : ''}</option>{/each}</select></label>
        <label><input type="checkbox" aria-label="Аудио исходника Filmcraft" bind:checked={audioPatch} disabled={disabled || busy} />Аудио</label>
        <label>Аудиодорожка <select aria-label="Аудиодорожка исходника Filmcraft" value={audioDest} disabled={disabled || busy || !audioPatch} onchange={event => { audioDest = event.currentTarget.value }}><option value="">Первая аудиодорожка</option>{#each tracks.filter(track => track.kind === 'audio_tracks') as track (track.id)}<option value={track.id}>{track.name}{track.locked ? ' (заблокирована)' : ''}</option>{/each}</select></label>
      {/if}
      <label><input type="checkbox" aria-label="Секвенция как клипы Filmcraft" bind:checked={sequencesAsClips} disabled={disabled || busy} />Секвенции как отдельные клипы</label>
      <label><input type="checkbox" aria-label="Сдвиг маркеров при вставке Filmcraft" bind:checked={rippleMarkers} disabled={disabled || busy} />Сдвигать маркеры при вставке</label>
      <label>Масштабирование <select aria-label="Масштабирование исходника Filmcraft" value={mediaScaling} onchange={event => { mediaScaling = event.currentTarget.value }} disabled={disabled || busy}><option value="none">Без изменения</option><option value="setToFrameSize">Подогнать масштаб</option><option value="scaleToFrameSize">Масштабировать к кадру</option></select></label>
      <label>Длительность неподвижного изображения <input aria-label="Длительность изображения Filmcraft" bind:value={stillDuration} disabled={disabled || busy} /></label>
      <label>Единицы длительности <select aria-label="Единицы длительности изображения Filmcraft" value={stillUnit} onchange={event => { stillUnit = event.currentTarget.value as typeof stillUnit }} disabled={disabled || busy}><option value="seconds">Секунды</option><option value="frames">Кадры</option></select></label>
      <button type="button" disabled={disabled || busy || !token || !sequenceId || !item || !validStill} onclick={() => place('source.insert')}>Вставить исходник</button>
      <button type="button" disabled={disabled || busy || !token || !sequenceId || !item || !validStill} onclick={() => place('source.overwrite')}>Записать исходник поверх</button>

    {/if}
    <label><input type="checkbox" aria-label="Выделение вслед за позицией Filmcraft" bind:checked={follows} disabled={disabled || busy} />Выделять клипы вслед за позицией</label>
    <button type="button" disabled={disabled || busy || !token || !sequenceId} onclick={() => navigate('markers.goToIn')}>К In таймлайна</button>
    <button type="button" disabled={disabled || busy || !token || !sequenceId} onclick={() => navigate('markers.goToOut')}>К Out таймлайна</button>
    {#each [['markers.goToSplitVideoIn', 'К видео In'], ['markers.goToSplitVideoOut', 'К видео Out'], ['markers.goToSplitAudioIn', 'К аудио In'], ['markers.goToSplitAudioOut', 'К аудио Out']] as [command, label] (command)}
      <button type="button" disabled={!ready} onclick={() => navigate(command as FilmcraftNavigationCommand)}>{label}</button>
    {/each}
  {/if}
  {#if busy}<span role="status">{busyLabel}</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
