<script lang="ts">
  import { editFilmcraftVideoOptions, type FilmcraftVideoOptionsCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, selection, timeFrames, timeTicks, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; selection: string[]; timeFrames: string; timeTicks?: string; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let enabled = $state(true), holdFilters = $state(false), reverseFieldDominance = $state(false)
  let holdOn = $state('in'), timeUnit = $state('time'), value = $state('0'), processing = $state('none'), mode = $state('frameSampling')
  let busy = $state(false), error = $state(''), status = $state('')
  let controller: AbortController | undefined
  let alive = true
  function context() { return JSON.stringify([document, sequenceId, clipId, token, selection, timeFrames, timeTicks, disabled, enabled, holdFilters, reverseFieldDominance, holdOn, timeUnit, value, processing, mode]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => { void document; void sequenceId; void clipId; void token; void selection; void timeFrames; void timeTicks; void disabled; return () => controller?.abort() })
  async function run(command: FilmcraftVideoOptionsCommand) {
    if (!token || busy || disabled || !sequenceId || !selection.length) return
    const expected = { document, sequenceId, clipId, token, timelineSelection: [...selection], timeFrames, timeTicks: timeTicks || null }
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''; status = ''
    try {
      const time: { time?: string; frame?: string; seconds?: number; timecode?: string } = {}
      if (command === 'clip.frameHoldOptions' && enabled && ['sequenceTime', 'sourceTimecode'].includes(holdOn)) {
        if (timeUnit === 'timecode') { if (!value.trim()) throw new Error('Введите таймкод'); time.timecode = value }
        else if (timeUnit === 'seconds') { if (!value.trim() || !Number.isFinite(Number(value))) throw new Error('Введите число секунд'); time.seconds = Number(value) }
        else { if (!/^(0|-?[1-9]\d*)$/.test(value) || BigInt(value) < -9223372036854775808n || BigInt(value) > 9223372036854775807n) throw new Error('Введите целое время'); if (timeUnit === 'frame') time.frame = value; else time.time = value }
      }
      const result = await editFilmcraftVideoOptions(token, command, { document, sequence: sequenceId, clips: [...selection], ...(command === 'clip.frameHoldOptions' ? { enabled, holdFilters, holdOn, ...time } : command === 'clip.fieldOptions' ? { processing, reverseFieldDominance } : command === 'clip.setTimeInterpolation' ? { mode } : {}), context: { activeSequence: sequenceId, selection: [...selection], ...(timeTicks ? { playhead: timeTicks } : { playheadFrames: timeFrames }) } }, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : `Обработано видеоклипов: ${result.videoOptionsClipCount}`
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Параметры видеоклипов</summary>
  <fieldset disabled={disabled || busy || !token || !selection.length}>
    <legend>Стоп-кадр</legend>
    <label><input type="checkbox" bind:checked={enabled} /> Включить стоп-кадр</label>
    <label>Удерживать <select aria-label="Режим стоп-кадра Filmcraft" value={holdOn} onchange={(event) => { holdOn = event.currentTarget.value; if (holdOn === 'sourceTimecode' && !['time', 'timecode'].includes(timeUnit)) timeUnit = 'time' }}><option value="in">Первый кадр клипа</option><option value="out">Последний кадр клипа</option><option value="playhead">Кадр под курсором</option><option value="sequenceTime">Время таймлайна</option><option value="sourceTimecode">Таймкод исходника</option></select></label>
    {#if enabled && ['sequenceTime', 'sourceTimecode'].includes(holdOn)}
      <label>Формат времени <select aria-label="Формат времени стоп-кадра Filmcraft" value={timeUnit} onchange={(event) => { timeUnit = event.currentTarget.value }}><option value="time">Тики</option>{#if holdOn === 'sequenceTime'}<option value="frame">Кадры таймлайна</option><option value="seconds">Секунды таймлайна</option>{/if}<option value="timecode">Таймкод</option></select></label>
      <label>Время <input aria-label="Время стоп-кадра Filmcraft" bind:value={value} /></label>
    {/if}
    <label><input type="checkbox" bind:checked={holdFilters} /> Удерживать эффекты на том же кадре</label>
    <button type="button" onclick={() => run('clip.frameHoldOptions')}>Применить стоп-кадр</button>
  </fieldset>
  <fieldset disabled={disabled || busy || !token || !selection.length}>
    <legend>Поля и интерполяция</legend>
    <label>Обработка полей <select aria-label="Обработка полей Filmcraft" value={processing} onchange={(event) => { processing = event.currentTarget.value }}><option value="none">Без обработки</option><option value="alwaysDeinterlace">Деинтерлейс</option><option value="flickerRemoval">Устранение мерцания</option></select></label>
    <label><input type="checkbox" bind:checked={reverseFieldDominance} /> Обратный порядок полей</label>
    <button type="button" onclick={() => run('clip.fieldOptions')}>Применить параметры полей</button>
    <label>Интерполяция <select aria-label="Интерполяция времени Filmcraft" value={mode} onchange={(event) => { mode = event.currentTarget.value }}><option value="frameSampling">Выборка кадров</option><option value="frameBlending">Смешивание кадров</option><option value="opticalFlow">Оптический поток</option></select></label>
    <button type="button" onclick={() => run('clip.setTimeInterpolation')}>Применить интерполяцию</button>
    <button type="button" onclick={() => run('clip.fitToFrame')}>Вписать в кадр</button>
    <button type="button" onclick={() => run('clip.fillFrame')}>Заполнить кадр</button>
  </fieldset>
  {#if busy}<span role="status">Обновляются параметры видео…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
