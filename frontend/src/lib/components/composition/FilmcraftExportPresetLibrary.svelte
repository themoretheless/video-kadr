<script lang="ts">
  import { listFilmcraftExportPresets, editFilmcraftExportPresets, type FilmcraftLibraryPreset, type FilmcraftPresetFile, type FilmcraftScaling } from '$lib/api'
  let { token, disabled = false, onapply }: { token: string | null; disabled?: boolean; onapply: (name: string, settings?: Record<string, unknown>) => void } = $props()
  let opened = $state(false)
  let entries = $state<FilmcraftLibraryPreset[]>([])
  let selected = $state('')
  let query = $state('')
  let favorites = $state(false)
  let name = $state('')
  let width = $state('')
  let height = $state('')
  let bitrate = $state('')
  let bitrateMode = $state('')
  let maxBitrate = $state('')
  let adaptiveBitrate = $state('')
  let keyframes = $state('')
  let profile = $state('')
  let multiplexer = $state('')
  let matchSource = $state(false)
  let scale = $state('1')
  let rateMode = $state('preset')
  let rateNum = $state('30000')
  let rateDen = $state('1001')
  let scaling = $state<FilmcraftScaling | ''>('')
  let audio = $state('preset')
  let audioCodec = $state('')
  let audioRate = $state('')
  let audioChannels = $state('')
  let audioBits = $state('')
  let audioBitrate = $state('')
  const audioRates = [32000, 44100, 48000, 96000]
  const audioBitrates = [128, 160, 192, 256, 320]
  let captions = $state<'preset' | 'burn' | 'none'>('preset')
  let h264Level = $state('preset')
  let sidecar = $state('preset')
  const sidecarFormats = ['srt', 'vtt', 'scc', 'mcc', 'stl', 'ttml', 'dfxp']
  const levels = [10, 11, 12, 13, 20, 21, 22, 30, 31, 32, 40, 41, 42, 50, 51, 52, 60, 61, 62]
  let busy = $state(false)
  let error = $state('')
  let controller: AbortController | null = null
  let generation = 0
  const picked = $derived(entries.find(p => p.name === selected))
  const audioOnly = $derived(picked?.settings.format === 'wav' || picked?.settings.format === 'aiff')
  const silent = $derived(['gif', 'png', 'tiff', 'bmp'].some(format => picked?.settings.format === format))
  const h26x = $derived(picked?.settings.format === 'h264' || picked?.settings.format === 'hevc')
  const h264 = $derived(picked?.settings.format === 'h264' || picked?.settings.mxfVideoCodec === 'h264')
  const videoBitrate = $derived(h26x || h264)
  const selectedBitrateMode = $derived(!h264 && bitrateMode === 'vbr2Pass' ? '' : bitrateMode)
  const effectiveBitrateMode = $derived(selectedBitrateMode || picked?.settings.bitrateMode)
  const forcedPcm = $derived(audioOnly || picked?.settings.format === 'mxf-op1a' || picked?.settings.format === 'mxf-opatom')
  const forcedAac = $derived(h26x && (multiplexer || picked?.settings.multiplexer) !== 'mov')
  const audioSettings = $derived(picked?.settings.audio as { codec?: string } | undefined)
  const effectiveAac = $derived(forcedAac || (!forcedPcm && (
    (audioCodec || audioSettings?.codec) === 'aac'
    || (h26x && (audioCodec || audioSettings?.codec || 'auto') === 'auto')
  )))
  const canEditAudio = $derived(!silent && (audioOnly || audio !== 'none'))
  const filtered = $derived(entries.filter(p => (!favorites || p.favorite) && [p.name, p.category, p.description].some(v => v.toLowerCase().includes(query.toLowerCase()))))
  $effect(() => { if (opened && !filtered.some(p => p.name === selected)) selected = filtered[0]?.name ?? '' })
  const positive = (value: string) => /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0
  const valid = $derived((matchSource ? Number.isFinite(Number(scale)) && Number(scale) > 0 : ((!width && !height) || (positive(width) && positive(height)))) && (!bitrate || (videoBitrate && !!adaptiveBitrate) || positive(bitrate))
    && (!videoBitrate || (
      (!maxBitrate || effectiveBitrateMode === 'cbr' || (positive(maxBitrate) && Number(maxBitrate) <= 1000000))
      && (!keyframes || (positive(keyframes) && Number(keyframes) <= 4294967295))
      && (!adaptiveBitrate || (Number.isFinite(Number(adaptiveBitrate)) && Number(adaptiveBitrate) > 0))
    ))
    && (audioOnly || rateMode !== 'custom' || (
      positive(rateNum) && positive(rateDen) && Number(rateNum) <= 4294967295
      && Number(rateDen) <= 4294967295 && Number(rateNum) >= Number(rateDen)
      && Number(rateNum) / Number(rateDen) <= 60
    )))
  $effect(() => {
    const owner = token
    ++generation; controller?.abort(); controller = null
    opened = false; entries = []; selected = ''; busy = false; error = ''; query = ''; favorites = false; name = ''; width = ''; height = ''; bitrate = ''; bitrateMode = ''; maxBitrate = ''; adaptiveBitrate = ''; keyframes = ''; profile = ''; multiplexer = ''; matchSource = false; scale = '1'; rateMode = 'preset'; rateNum = '30000'; rateDen = '1001'; scaling = ''; audio = 'preset'; audioCodec = ''; audioRate = ''; audioChannels = ''; audioBits = ''; audioBitrate = ''; captions = 'preset'; h264Level = 'preset'; sidecar = 'preset'
    return () => { void owner; ++generation; controller?.abort() }
  })
  async function run(action: (owner: string, signal: AbortSignal) => Promise<void>) {
    if (!token || busy || disabled) return
    const owner = token; const version = ++generation
    controller?.abort(); const current = new AbortController(); controller = current
    busy = true; error = ''
    try {
      await action(owner, current.signal)
    } catch (failure) {
      if (generation === version && !current.signal.aborted) error = failure instanceof Error ? failure.message : String(failure)
    } finally { if (generation === version) busy = false }
  }
  async function refresh(owner: string, signal: AbortSignal) {
    const result = await listFilmcraftExportPresets(owner, {}, signal)
    if (signal.aborted) return
    entries = result.presets
    if (!entries.some(p => p.name === selected)) selected = entries[0]?.name ?? ''
  }
  function open() { opened = true; void run(refresh) }
  function draftSettings() {
    const settings: Record<string, unknown> = {}
    if (matchSource) { settings.frameSize = null; settings.scale = Number(scale) }
    else if (width && height) settings.frameSize = [Number(width), Number(height)]
    if (!audioOnly) {
      if (rateMode === 'source') settings.frameRate = null
      if (rateMode === 'custom') settings.frameRate = { num: Number(rateNum), den: Number(rateDen) }
      if (scaling) settings.scaling = scaling
    }
    if (!audioOnly && !silent && audio !== 'preset') settings.includeAudio = audio === 'include'
    if (canEditAudio) {
      const audioPatch: Record<string, unknown> = {}
      if (audioCodec && !forcedPcm && !forcedAac) audioPatch.codec = audioCodec
      if (audioRate) audioPatch.sample_rate = audioRate === 'source' ? null : Number(audioRate)
      if (audioChannels) audioPatch.channels = Number(audioChannels)
      if (effectiveAac && audioBitrate) audioPatch.bitrate_kbps = Number(audioBitrate)
      if (!effectiveAac && audioBits) audioPatch.bits = Number(audioBits)
      if (Object.keys(audioPatch).length) settings.audio = audioPatch
    }
    if (h26x && multiplexer) settings.multiplexer = multiplexer
    if (h264 && profile) settings.h264Profile = profile
    if (videoBitrate) {
      if (selectedBitrateMode) settings.bitrateMode = selectedBitrateMode
      if (maxBitrate && effectiveBitrateMode !== 'cbr') settings.maxBitrateKbps = Number(maxBitrate)
      if (keyframes) settings.keyframeDistance = Number(keyframes)
    }
    if (videoBitrate && adaptiveBitrate) settings.adaptiveBitrate = Number(adaptiveBitrate)
    else if (bitrate) { settings.bitrateKbps = Number(bitrate); settings.adaptiveBitrate = null }
    if (captions !== 'preset') settings.burnCaptions = captions === 'burn'
    if (sidecar !== 'preset') settings.captionSidecar = sidecar === 'none' ? null : sidecar
    if (h264Level !== 'preset' && (picked?.settings.format === 'h264' || picked?.settings.mxfVideoCodec === 'h264')) settings.h264Level = h264Level === 'auto' ? null : Number(h264Level)
    return settings
  }
  function save() {
    if (!valid || !name.trim() || !picked) return
    const settings = draftSettings()
    void run(async (owner, signal) => {
      const result = await editFilmcraftExportPresets<{ name: string }>(owner, { action: 'save', name, from: selected, settings, overwrite: false }, signal)
      if (signal.aborted) return
      await refresh(owner, signal)
      if (!signal.aborted) { query = ''; favorites = false; selected = result.name }
    })
  }
  function favorite() { if (picked) void run(async (owner, signal) => { await editFilmcraftExportPresets(owner, { action: 'favorite', name: selected }, signal); await refresh(owner, signal) }) }
  function remove() { if (picked && !picked.builtin) void run(async (owner, signal) => { await editFilmcraftExportPresets(owner, { action: 'delete', name: selected }, signal); await refresh(owner, signal) }) }
  function importFile(file: File | undefined) {
    if (!file) return
    void run(async (owner, signal) => {
      if (file.size > 1024 * 1024) throw new Error('Файл пресетов должен быть не больше 1 МБ')
      const contents = await file.text(); if (signal.aborted) return
      await editFilmcraftExportPresets(owner, { action: 'import', file: JSON.parse(contents) as FilmcraftPresetFile }, signal)
      await refresh(owner, signal)
    })
  }
  function exportFile() {
    if (!picked) return
    void run(async (owner, signal) => {
      const file = await editFilmcraftExportPresets<FilmcraftPresetFile>(owner, { action: 'export', names: [selected] }, signal)
      if (signal.aborted) return
      const url = URL.createObjectURL(new Blob([JSON.stringify(file, null, 2)], { type: 'application/json' }))
      const anchor = window.document.createElement('a'); anchor.href = url; anchor.download = 'filmcraft-export-presets.json'
      window.document.body.append(anchor); anchor.click(); anchor.remove(); URL.revokeObjectURL(url)
    })
  }
</script>

<section aria-label="Библиотека пресетов Filmcraft">
  <button type="button" onclick={open} disabled={!token || busy || disabled}>Библиотека пресетов</button>
  {#if opened}
    <label>Поиск <input aria-label="Поиск пресетов Filmcraft" bind:value={query} /></label>
    <label><input type="checkbox" bind:checked={favorites} /> Только избранные</label>
    <label>Сохранённый пресет <select aria-label="Сохранённый пресет Filmcraft" bind:value={selected} disabled={busy || disabled}>
      {#each filtered as item (item.name)}<option value={item.name}>{item.favorite ? '★ ' : ''}{item.name}</option>{/each}
    </select></label>
    {#if picked}
      <p>{picked.category} · {picked.description}</p>
      <button type="button" disabled={busy || disabled} onclick={() => onapply(selected)}>Применить пресет</button>
      <button type="button" disabled={busy || disabled} onclick={favorite}>{picked.favorite ? 'Убрать из избранного' : 'В избранное'}</button>
      <button type="button" disabled={busy || disabled || picked.builtin} onclick={remove}>Удалить пресет</button>
      <button type="button" disabled={busy || disabled} onclick={exportFile}>Скачать пресет</button>
      <label>Название копии <input aria-label="Название нового пресета Filmcraft" bind:value={name} disabled={busy || disabled} /></label>
      <label><input aria-label="Размер секвенции нового пресета Filmcraft" type="checkbox" bind:checked={matchSource} disabled={busy || disabled} /> Размер секвенции × масштаб</label>
      {#if matchSource}<label>Масштаб <input aria-label="Масштаб нового пресета Filmcraft" inputmode="decimal" bind:value={scale} disabled={busy || disabled} /></label>{/if}
      <label>Ширина <input aria-label="Ширина нового пресета Filmcraft" inputmode="numeric" bind:value={width} disabled={matchSource || busy || disabled} /></label>
      <label>Высота <input aria-label="Высота нового пресета Filmcraft" inputmode="numeric" bind:value={height} disabled={matchSource || busy || disabled} /></label>
      {#if !audioOnly}
        <label>Частота кадров <select aria-label="Частота кадров пресета Filmcraft" value={rateMode} onchange={event => { rateMode = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="preset">Как в исходном пресете</option>
          <option value="source">Как в секвенции</option>
          <option value="custom">Своя частота</option>
        </select></label>
        {#if rateMode === 'custom'}
          <label>Числитель <input aria-label="Числитель частоты кадров пресета Filmcraft" inputmode="numeric" bind:value={rateNum} disabled={busy || disabled} /></label>
          <label>Знаменатель <input aria-label="Знаменатель частоты кадров пресета Filmcraft" inputmode="numeric" bind:value={rateDen} disabled={busy || disabled} /></label>
          <small>От 1 до 60 кадров/с, например 30000 / 1001.</small>
        {/if}
        <label>Масштабирование <select aria-label="Масштабирование пресета Filmcraft" value={scaling} onchange={event => { scaling = event.currentTarget.value as FilmcraftScaling | '' }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="scaleToFit">Вписать целиком</option>
          <option value="scaleToFill">Заполнить с обрезкой</option>
          <option value="stretchToFill">Растянуть</option>
        </select></label>
        {#if !silent}
          <label>Звук <select aria-label="Звук пресета Filmcraft" value={audio} onchange={event => { audio = event.currentTarget.value }} disabled={busy || disabled}>
            <option value="preset">Как в исходном пресете</option>
            <option value="include">Включить звук</option>
            <option value="none">Без звука</option>
          </select></label>
        {/if}
      {/if}
      {#if h26x}
        <label>Контейнер <select aria-label="Контейнер пресета Filmcraft" value={multiplexer} onchange={event => { multiplexer = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="mp4">MP4</option><option value="mov">MOV</option>
        </select></label>
      {/if}
      {#if h264}
        <label>Профиль H.264 <select aria-label="Профиль H.264 пресета Filmcraft" value={profile} onchange={event => { profile = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="baseline">Baseline</option><option value="main">Main</option><option value="high">High</option>
        </select></label>
      {/if}
      {#if videoBitrate}
        <label>Режим битрейта <select aria-label="Режим битрейта пресета Filmcraft" value={selectedBitrateMode} onchange={event => { bitrateMode = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="cbr">CBR</option><option value="vbr1Pass">VBR · 1 проход</option>
          {#if h264}<option value="vbr2Pass">VBR · 2 прохода</option>{/if}
        </select></label>
        {#if effectiveBitrateMode !== 'cbr'}
          <label>Максимум, кбит/с <input aria-label="Максимальный битрейт пресета Filmcraft" inputmode="numeric" bind:value={maxBitrate} disabled={busy || disabled} /></label>
        {/if}
        <label>Адаптивный битрейт, бит на пиксель <input aria-label="Адаптивный битрейт пресета Filmcraft" inputmode="decimal" bind:value={adaptiveBitrate} disabled={busy || disabled} /></label>
        <label>Интервал ключевых кадров <input aria-label="Интервал ключевых кадров пресета Filmcraft" inputmode="numeric" bind:value={keyframes} disabled={busy || disabled} /></label>
      {/if}
      {#if canEditAudio}
        {#if !forcedPcm && !forcedAac}
          <label>Аудиокодек <select aria-label="Аудиокодек пресета Filmcraft" value={audioCodec} onchange={event => { audioCodec = event.currentTarget.value }} disabled={busy || disabled}>
            <option value="">Как в исходном пресете</option>
            <option value="aac">AAC</option>
            <option value="pcm">PCM</option>
          </select></label>
        {/if}
        <label>Частота аудио <select aria-label="Частота аудио пресета Filmcraft" value={audioRate} onchange={event => { audioRate = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="source">Как в секвенции</option>
          {#each audioRates as rate (rate)}<option value={String(rate)}>{rate} Гц</option>{/each}
        </select></label>
        <label>Каналы <select aria-label="Каналы пресета Filmcraft" value={audioChannels} onchange={event => { audioChannels = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="1">Моно</option><option value="2">Стерео</option><option value="6">5.1</option>
        </select></label>
        {#if effectiveAac}
          <label>Битрейт AAC <select aria-label="Битрейт AAC пресета Filmcraft" value={audioBitrate} onchange={event => { audioBitrate = event.currentTarget.value }} disabled={busy || disabled}>
            <option value="">Как в исходном пресете</option>
            {#each audioBitrates as rate (rate)}<option value={String(rate)}>{rate} кбит/с</option>{/each}
          </select></label>
        {:else}
          <label>Разрядность PCM <select aria-label="Разрядность PCM пресета Filmcraft" value={audioBits} onchange={event => { audioBits = event.currentTarget.value }} disabled={busy || disabled}>
            <option value="">Как в исходном пресете</option>
            <option value="16">16 бит</option><option value="24">24 бит</option>
          </select></label>
        {/if}
      {/if}
      <label>Битрейт, кбит/с <input aria-label="Битрейт нового пресета Filmcraft" inputmode="numeric" bind:value={bitrate} disabled={busy || disabled || (videoBitrate && !!adaptiveBitrate)} /></label>
      <label>Субтитры <select aria-label="Субтитры нового пресета Filmcraft" value={captions} onchange={(event) => { const value = event.currentTarget.value; if (value === 'preset' || value === 'burn' || value === 'none') captions = value }} disabled={busy || disabled}><option value="preset">Как в исходном пресете</option><option value="burn">Включить в кадр</option><option value="none">Не включать в кадр</option></select></label>
      <label>Отдельный файл субтитров <select aria-label="Файл субтитров нового пресета Filmcraft" value={sidecar} onchange={(event) => { const value = event.currentTarget.value; if (value === 'preset' || value === 'none' || sidecarFormats.includes(value)) sidecar = value }} disabled={busy || disabled}>
        <option value="preset">Как в исходном пресете</option><option value="none">Не создавать</option>
        {#each sidecarFormats as format (format)}<option value={format}>{format.toUpperCase()}</option>{/each}
      </select></label>
      {#if sidecar !== 'none' && (sidecar !== 'preset' || typeof picked.settings.captionSidecar === 'string')}
        <p>Файл содержит первый трек субтитров со сдвигом к началу экспорта. Реплики за концом диапазона сохраняются.</p>
      {/if}
      {#if picked.settings.format === 'h264' || picked.settings.mxfVideoCodec === 'h264'}
        <label>Уровень H.264 <select aria-label="Уровень H.264 нового пресета Filmcraft" value={h264Level} onchange={(event) => { const value = event.currentTarget.value; if (value === 'preset' || value === 'auto' || levels.includes(Number(value))) h264Level = value }} disabled={busy || disabled}>
          <option value="preset">Как в исходном пресете</option><option value="auto">Автоматически</option>
          {#each levels as level (level)}<option value={String(level)}>{(level / 10).toFixed(1)}</option>{/each}
        </select></label>
        <p>Если выбранный уровень слишком низкий для видео, энкодер повысит его автоматически.</p>
      {/if}
      <button type="button" disabled={busy || disabled || !valid || !Object.keys(draftSettings()).length}
        onclick={() => onapply(selected, draftSettings())}>Применить правки к экспорту</button>
      <button type="button" disabled={busy || disabled || !valid || !name.trim()} onclick={save}>Сохранить копию пресета</button>
    {/if}
    <label>Импорт пресетов <input aria-label="Импорт пресетов Filmcraft" type="file" accept="application/json,.json" disabled={busy || disabled} onchange={(event) => { importFile(event.currentTarget.files?.[0]); event.currentTarget.value = '' }} /></label>
    {#if busy}<span role="status">Загрузка пресетов…</span>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  {/if}
</section>
