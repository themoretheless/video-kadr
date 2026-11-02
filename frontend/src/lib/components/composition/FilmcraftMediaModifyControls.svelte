<script lang="ts">
  import { modifyFilmcraftMedia, type FilmcraftMediaModifyCommand, type FilmcraftImportResult, type FilmcraftSourceEditFence } from '$lib/api'
  let { document, sequenceId, clipId, token, selection, selected, items, sourceBindings, disabled = false, onedit }: {
    document: string; sequenceId: string; clipId: string; token: string | null; selection: string[]; selected: string[]; items: NonNullable<FilmcraftImportResult['projectItems']>; sourceBindings?: Record<string, string>; disabled?: boolean
    onedit: (result: FilmcraftImportResult, expected: FilmcraftSourceEditFence) => boolean | void
  } = $props()
  let target = $state(''), format = $state('current'), custom = $state(false), mapping = $state('0,1'), channels = $state('0,1')
  let tcMode = $state('unchanged'), tcValue = $state('0'), tapeEnabled = $state(false), tapeName = $state('')
  let busy = $state(false), error = $state(''), status = $state('')
  let controller: AbortController | undefined
  let alive = true
  const media = $derived(items.filter(item => item.kind === 'Media'))
  const targets = $derived(target ? [target] : selected)
  const info = $derived(media.find(item => item.id === (target || selected[0]))?.mediaInterpretation)
  function context() { return JSON.stringify([document, sequenceId, clipId, token, selection, selected, sourceBindings, disabled, target, format, custom, mapping, channels, tcMode, tcValue, tapeEnabled, tapeName]) }
  $effect(() => { alive = true; return () => { alive = false; controller?.abort() } })
  $effect(() => { void document; void sequenceId; void clipId; void token; void selection; void selected; void sourceBindings; void disabled; return () => controller?.abort() })
  function channelList(text: string): number[] {
    if (!text.trim()) return []
    return text.split(',').map(part => { const v = part.trim(); if (!/^(0|[1-9]\d*)$/.test(v) || BigInt(v) > 65535n) throw new Error('Каналы должны быть целыми числами от 0 до 65535'); return Number(v) })
  }
  async function run(command: FilmcraftMediaModifyCommand, timeline = false) {
    if (!token || busy || disabled || (timeline ? !sequenceId || !selection.length : !targets.length)) return
    const expected = { document, sequenceId, clipId, token, timelineSelection: [...selection], projectSelection: [...selected], ...(sourceBindings ? { sourceBindings: { ...sourceBindings } } : {}) }
    const captured = context(), current = new AbortController(); controller = current; busy = true; error = ''; status = ''
    try {
      const input: Parameters<typeof modifyFilmcraftMedia>[2] = { document, context: { activeSequence: sequenceId || null, selection: [...selection], projectSelection: [...selected] } }
      if (command === 'clip.audioChannels') {
        if (timeline) { input.clips = [...selection]; input.channels = channelList(channels) }
        else { input.items = [...targets]; if (format !== 'current') input.format = format; if (custom) { input.clips = mapping.split(/[;\n]/).map(channelList); if (!input.clips.length || input.clips.some(row => !row.length)) throw new Error('Для каждого клипа укажите хотя бы один канал') } }
      } else if (command === 'clip.breakoutToMono') input.items = [...targets]
      else {
        if (target) input.item = target
        if (tcMode === 'frame') { if (!/^(0|[1-9]\d*)$/.test(tcValue) || BigInt(tcValue) > 9223372036854775807n) throw new Error('Введите неотрицательный номер кадра'); input.frame = tcValue }
        if (tcMode === 'timecode') { if (!tcValue.trim()) throw new Error('Введите таймкод'); input.timecode = tcValue }
        if (tcMode === 'reset') input.reset = true
        if (tapeEnabled) input.tapeName = tapeName
      }
      const result = await modifyFilmcraftMedia(token, command, input, current.signal)
      if (!alive || current.signal.aborted || captured !== context()) return
      status = onedit(result, expected) === false ? 'Результат не применён: контекст изменился' : `Обработано элементов: ${result.modificationCount}`
    } catch (failure) { if (alive && !current.signal.aborted && captured === context()) error = failure instanceof Error ? failure.message : String(failure) }
    finally { if (controller === current) { controller = undefined; busy = false } }
  }
</script>
<details>
  <summary>Каналы и таймкод исходников</summary>
  <label>Исходник <select aria-label="Исходник Modify Filmcraft" value={target} onchange={(event) => { target = event.currentTarget.value }} disabled={disabled || busy || !token}><option value="">Из выделения проекта</option>{#each media as entry (entry.id)}<option value={entry.id}>{entry.name}</option>{/each}</select></label>
  {#if info}<p>Каналов в исходнике: {info.audioChannels ?? 'нет'}. Начальный кадр таймкода: {info.startTimecode ?? 'не задан'}. Лента: {info.tapeName ?? 'не задана'}.</p>{#if info.channelMap}<p>Схема: {info.channelMap.format}; {info.channelMap.clips.map(row => row.join(',')).join('; ')}</p>{/if}{/if}
  <fieldset disabled={disabled || busy || !token || !targets.length}>
    <legend>Каналы исходника</legend>
    <label>Формат <select aria-label="Формат каналов исходника Filmcraft" value={format} onchange={(event) => { format = event.currentTarget.value }}><option value="current">Текущий формат</option><option value="mono">Моно</option><option value="stereo">Стерео</option><option value="5.1">5.1</option><option value="adaptive">Адаптивный</option></select></label>
    <label><input type="checkbox" bind:checked={custom} /> Пользовательская схема каналов</label>
    {#if custom}<label>Каналы каждого клипа, с нуля; клипы разделяются точкой с запятой <textarea aria-label="Схема каналов исходника Filmcraft" bind:value={mapping}></textarea></label>{/if}
    <button type="button" onclick={() => run('clip.audioChannels')}>Применить каналы исходника</button>
    <button type="button" onclick={() => run('clip.breakoutToMono')}>Создать моно-элементы</button>
    <span>Каждый канал станет отдельным элементом проекта.</span>
  </fieldset>
  <fieldset disabled={disabled || busy || !token || !sequenceId || !selection.length}>
    <legend>Каналы клипов таймлайна</legend>
    <label>Номера каналов с нуля, через запятую <input aria-label="Каналы клипов Filmcraft" bind:value={channels} /></label>
    <span>Пустое поле возвращает стандартную пару каналов.</span>
    <button type="button" onclick={() => run('clip.audioChannels', true)}>Применить каналы клипов</button>
  </fieldset>
  <fieldset disabled={disabled || busy || !token || !targets.length}>
    <legend>Таймкод и лента</legend>
    <label>Таймкод <select aria-label="Правка таймкода Filmcraft" value={tcMode} onchange={(event) => { tcMode = event.currentTarget.value }}><option value="unchanged">Оставить текущий</option><option value="frame">Номер начального кадра</option><option value="timecode">Таймкод</option><option value="reset">Сбросить к файловому</option></select></label>
    {#if tcMode === 'frame' || tcMode === 'timecode'}<label>Значение <input aria-label="Новый таймкод Filmcraft" bind:value={tcValue} /></label>{/if}
    <label><input type="checkbox" bind:checked={tapeEnabled} /> Изменить имя ленты</label>
    {#if tapeEnabled}<label>Имя ленты; пустое удаляет имя <input aria-label="Имя ленты Filmcraft" bind:value={tapeName} /></label>{/if}
    <button type="button" disabled={tcMode === 'unchanged' && !tapeEnabled} onclick={() => run('clip.modifyTimecode')}>Применить таймкод</button>
  </fieldset>
  {#if busy}<span role="status">Обновляются исходники…</span>{/if}
  {#if status}<span role="status">{status}</span>{/if}
  {#if error}<span role="alert">{error}</span>{/if}
</details>
