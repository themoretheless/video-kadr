<script lang="ts">
  import { editFilmcraftClips, type FilmcraftClipEditCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, selection, timeFrames, timeTicks, tracks = [], linked = true, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; selection: string[]; timeFrames: string; timeTicks?: string; tracks?: NonNullable<FilmcraftImportResult['sequences'][number]['tracks']>; linked?: boolean; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let speed = $state('100'), reverse = $state(false), ripple = $state(false), interpolation = $state('current'), duration = $state('2')
  let customTracks = $state(false), trackIds = $state<string[]>([])
  let busy = $state(false), error = $state(''), status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function context() { return JSON.stringify([document, sequenceId, clipId, token, selection, timeFrames, timeTicks, linked, disabled, speed, reverse, ripple, interpolation, duration, customTracks, trackIds]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => { void document; void sequenceId; void clipId; void token; void selection; void timeFrames; void timeTicks; void linked; void disabled; return () => controller?.abort() })
  async function run(command: FilmcraftClipEditCommand) {
    const hold = command === 'clip.frameHold' || command === 'clip.insertFrameHoldSegment'
    if (!token || busy || disabled || !sequenceId || !hold && !selection.length) return
    const expected = { document, sequenceId, clipId, token, timelineSelection: [...selection], timeFrames, timeTicks: timeTicks || null }
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''; status = ''
    try {
      const input: Parameters<typeof editFilmcraftClips>[2] = { document, sequence: sequenceId, context: { activeSequence: sequenceId, selection: [...selection], linkedSelection: linked, ...(timeTicks ? { playhead: timeTicks } : { playheadFrames: timeFrames }), ...(hold && customTracks ? { targetedTrackIds: [...trackIds] } : {}) } }
      if (!hold && !['clip.group', 'clip.ungroup', 'clip.scaleToFrameSize'].includes(command)) input.clips = [...selection]
      if (command === 'clip.speedDuration') { if (!speed.trim() || !Number.isFinite(Number(speed)) || Number(speed) <= 0) throw new Error('Скорость должна быть положительным числом'); input.speed = Number(speed); input.reverse = reverse; input.ripple = ripple; if (interpolation !== 'current') input.interpolation = interpolation }
      if (hold) { if (timeTicks) input.time = timeTicks; else input.frame = timeFrames }
      if (command === 'clip.insertFrameHoldSegment') { if (!duration.trim() || !Number.isFinite(Number(duration))) throw new Error('Введите длительность в секундах'); input.seconds = Number(duration) }
      const result = await editFilmcraftClips(token, command, input, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : 'Правка клипов применена'
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Скорость, связи и стоп-кадр</summary>
  <fieldset disabled={disabled || busy || !token || !selection.length}>
    <legend>Скорость и выделенные клипы</legend>
    <label>Скорость, % <input aria-label="Скорость клипов Filmcraft" bind:value={speed} /></label>
    <label><input type="checkbox" bind:checked={reverse} /> Обратное воспроизведение</label>
    <label><input type="checkbox" bind:checked={ripple} /> Сдвигать последующие клипы</label>
    <label>Интерполяция <select aria-label="Интерполяция скорости Filmcraft" value={interpolation} onchange={(event) => { interpolation = event.currentTarget.value }}><option value="current">Оставить текущую</option><option value="frameSampling">Выборка кадров</option><option value="frameBlending">Смешивание кадров</option><option value="opticalFlow">Оптический поток</option></select></label>
    <button type="button" onclick={() => run('clip.speedDuration')}>Применить скорость</button>
    <button type="button" onclick={() => run('clip.enable')}>Включить / выключить клипы</button>
    <button type="button" onclick={() => run('clip.link')}>Связать / разорвать связь</button>
    <button type="button" onclick={() => run('clip.group')}>Сгруппировать клипы</button>
    <button type="button" onclick={() => run('clip.ungroup')}>Разгруппировать клипы</button>
    <button type="button" onclick={() => run('clip.scaleToFrameSize')}>Переключить масштаб по кадру</button>
  </fieldset>
  <fieldset disabled={disabled || busy || !token || !sequenceId}>
    <legend>Стоп-кадр от позиции курсора</legend>
    <span>Используется выбранный видеоклип, а если подходящего нет — клип под курсором на целевой дорожке.</span>
    <label><input type="checkbox" bind:checked={customTracks} /> Ограничить поиск дорожками</label>
    {#if customTracks}<select multiple aria-label="Дорожки стоп-кадра Filmcraft" value={trackIds} onchange={(event) => { trackIds = [...event.currentTarget.selectedOptions].map(option => option.value) }}>{#each tracks as track (track.id)}<option value={track.id}>{track.name}</option>{/each}</select>{/if}
    <button type="button" onclick={() => run('clip.frameHold')}>Стоп-кадр от курсора до конца</button>
    <label>Длительность вставки, секунд <input aria-label="Длительность стоп-кадра Filmcraft" bind:value={duration} /></label>
    <button type="button" onclick={() => run('clip.insertFrameHoldSegment')}>Вставить сегмент стоп-кадра</button>
  </fieldset>
  {#if busy}<span role="status">Обновляются клипы…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
