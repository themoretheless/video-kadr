<script lang="ts">
  import type { manualExportPreset } from '$lib/filmcraft/manual-export-preset'
  import type { MediaEntry } from '$lib/types'
  import { exportOverlayPlacements } from '$lib/filmcraft/export-overlay'
  import { exportMetadataFields, validExportMetadata } from '$lib/filmcraft/export-metadata'
  import { listFilmcraftExportPresets, editFilmcraftExportPresets, type FilmcraftLibraryPreset, type FilmcraftPresetFile, type FilmcraftScaling, type FilmcraftExportMetadata, type FilmcraftOverlayPlacement } from '$lib/api'
  let { token, imageSources = [], disabled = false, onapply, onlibrarychange, manualSnapshot }: { token: string | null; imageSources?: MediaEntry[]; disabled?: boolean; onapply: (name: string, settings?: Record<string, unknown>) => void; onlibrarychange?: (change: { saved?: string; deleted?: string }) => void; manualSnapshot?: () => ReturnType<typeof manualExportPreset> } = $props()
  let opened = $state(false)
  let entries = $state<FilmcraftLibraryPreset[]>([])
  let selected = $state('')
  let query = $state('')
  let favorites = $state(false)
  let manualName = $state('')
  let name = $state('')
  let format = $state('')
  let mxfCodec = $state('')
  const formats = [
    ['h264', 'H.264'], ['hevc', 'HEVC'], ['mjpeg', 'Motion JPEG'], ['prores', 'Apple ProRes'],
    ['dnxhr', 'Avid DNxHR'], ['apv', 'APV'], ['mxf-op1a', 'MXF OP1a'], ['mxf-opatom', 'MXF OP-Atom'],
    ['png', 'PNG-последовательность'], ['tiff', 'TIFF-последовательность'], ['bmp', 'BMP-последовательность'],
    ['gif', 'GIF'], ['wav', 'WAV'], ['aiff', 'AIFF'],
  ] as const
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
  let aspect = $state('preset')
  let aspectNum = $state('1')
  let aspectDen = $state('1')
  let renderQuality = $state('preset')
  let intraProfile = $state('')
  let jpegQuality = $state('')
  const overlayFields = [
    { key: 'name_overlay', label: 'Подпись', textLabel: 'Текст' },
    { key: 'timecode_overlay', label: 'Таймкод', textLabel: 'Префикс' },
  ] as const
  type OverlayKey = typeof overlayFields[number]['key']
  type PortableOverlay = {
    text: string; placement: FilmcraftOverlayPlacement; offset: [number, number]
    size_percent: number; opacity: number
  }
  let overlayModes = $state<Partial<Record<OverlayKey, string>>>({})
  let textOverlays = $state<Record<OverlayKey, Partial<PortableOverlay>>>({ name_overlay: {}, timecode_overlay: {} })
  type PortableImageOverlay = Omit<PortableOverlay, 'text'> & { source_id: string }
  let imageMode = $state('preset')
  let imageOverlay = $state<Partial<PortableImageOverlay>>({})
  let limiter = $state('preset')
  let limiterMin = $state<number>()
  let limiterMax = $state<number>()
  let loudness = $state('preset')
  let loudnessTarget = $state<number>()
  let loudnessPeak = $state<number>()
  let metadata = $state<FilmcraftExportMetadata>({})
  const intraProfiles = [
    { format: 'prores', field: 'proresProfile', options: [['proxy', '422 Proxy'], ['lt', '422 LT'], ['standard', '422'], ['hq', '422 HQ']] },
    { format: 'dnxhr', field: 'dnxProfile', options: [['lb', 'LB'], ['sq', 'SQ'], ['hq', 'HQ']] },
    { format: 'apv', field: 'apvProfile', options: [['422-10', '422-10'], ['422-12', '422-12']] },
  ] as const
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
  const effectiveFormat = $derived(format || picked?.settings.format)
  const mxf = $derived(effectiveFormat === 'mxf-op1a' || effectiveFormat === 'mxf-opatom')
  const effectiveMxfCodec = $derived(effectiveFormat === 'mxf-opatom' ? 'dnxhr' : (mxfCodec || picked?.settings.mxfVideoCodec || 'dnxhr'))
  const audioOnly = $derived(effectiveFormat === 'wav' || effectiveFormat === 'aiff')
  const silent = $derived(['gif', 'png', 'tiff', 'bmp'].some(item => effectiveFormat === item))
  const h26x = $derived(effectiveFormat === 'h264' || effectiveFormat === 'hevc')
  const h264 = $derived(effectiveFormat === 'h264' || (mxf && effectiveMxfCodec === 'h264'))
  const videoBitrate = $derived(h26x || h264)
  const selectedBitrateMode = $derived(!h264 && bitrateMode === 'vbr2Pass' ? '' : bitrateMode)
  const effectiveBitrateMode = $derived(selectedBitrateMode || picked?.settings.bitrateMode)
  const forcedPcm = $derived(audioOnly || mxf)
  const forcedAac = $derived(h26x && (multiplexer || picked?.settings.multiplexer) !== 'mov')
  const audioSettings = $derived(picked?.settings.audio as { codec?: string } | undefined)
  const effectiveAac = $derived(forcedAac || (!forcedPcm && (
    (audioCodec || audioSettings?.codec) === 'aac'
    || (h26x && (audioCodec || audioSettings?.codec || 'auto') === 'auto')
  )))
  const canEditAudio = $derived(!silent && (audioOnly || audio !== 'none'))
  const profileGroup = $derived(intraProfiles.find(group => group.format === effectiveFormat))
  const selectedIntraProfile = $derived(profileGroup?.options.some(([value]) => value === intraProfile) ? intraProfile : '')
  const mjpeg = $derived(effectiveFormat === 'mjpeg')
  const savedMetadata = $derived(picked?.settings.metadata as Record<string, unknown> | undefined)
  function metadataValue(key: keyof FilmcraftExportMetadata): string {
    const inherited = savedMetadata?.[key]
    return metadata[key] ?? (typeof inherited === 'string' ? inherited : '')
  }
  const metadataValid = $derived(validExportMetadata(Object.fromEntries(
    exportMetadataFields.map(({ key }) => [key, metadataValue(key)]),
  )))
  const savedEffects = $derived(picked?.settings.effects as {
    image_overlay?: PortableImageOverlay
    video_limiter?: { min_percent?: number; max_percent?: number }
    loudness?: { target_lufs?: number; true_peak_dbtp?: number }
  } & Partial<Record<OverlayKey, PortableOverlay>> | undefined)
  function overlayValue(key: OverlayKey): PortableOverlay {
    return {
      text: '', placement: 'bottomCenter', offset: [0, 0], size_percent: 5, opacity: 100,
      ...savedEffects?.[key], ...textOverlays[key],
    }
  }
  const imageValue = $derived<PortableImageOverlay>({
    source_id: '', placement: 'topRight', offset: [0, 0], size_percent: 15, opacity: 100,
    ...savedEffects?.image_overlay, ...imageOverlay,
  })
  const imageValid = $derived(imageMode !== 'on' || (
    imageSources.some(source => source.id === imageValue.source_id)
    && imageValue.offset.every(value => Number.isFinite(value) && Math.abs(value) <= 1000000)
    && imageValue.size_percent >= 0.5 && imageValue.size_percent <= 100
    && imageValue.opacity >= 0 && imageValue.opacity <= 100
  ))
  const overlaysValid = $derived(overlayFields.every(({ key }) => {
    if (overlayModes[key] !== 'on') return true
    const { text, offset, size_percent, opacity } = overlayValue(key)
    return !text.includes('\0') && new TextEncoder().encode(text).length <= 16000
      && offset.every(item => Number.isFinite(item) && Math.abs(item) <= 1000000)
      && size_percent >= 1 && size_percent <= 50 && opacity >= 0 && opacity <= 100
  }))
  const limiterValid = $derived(limiter !== 'on'
    || (limiterMin !== undefined && limiterMax !== undefined && limiterMin >= 0 && limiterMin <= 100 && limiterMax >= 0 && limiterMax <= 100))
  const loudnessValid = $derived(loudness !== 'on'
    || (loudnessTarget !== undefined && Number.isFinite(loudnessTarget)
      && loudnessPeak !== undefined && loudnessPeak >= -24 && loudnessPeak <= 0))
  const filtered = $derived(entries.filter(p => (!favorites || p.favorite) && [p.name, p.category, p.description].some(v => v.toLowerCase().includes(query.toLowerCase()))))
  $effect(() => { if (opened && !filtered.some(p => p.name === selected)) selected = filtered[0]?.name ?? '' })
  const positive = (value: string) => /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0
  const valid = $derived((matchSource ? Number.isFinite(Number(scale)) && Number(scale) > 0 : ((!width && !height) || (positive(width) && positive(height)))) && (!bitrate || (videoBitrate && !!adaptiveBitrate) || positive(bitrate))
    && (!videoBitrate || (
      (!maxBitrate || effectiveBitrateMode === 'cbr' || (positive(maxBitrate) && Number(maxBitrate) <= 1000000))
      && (!keyframes || (positive(keyframes) && Number(keyframes) <= 4294967295))
      && (!adaptiveBitrate || (Number.isFinite(Number(adaptiveBitrate)) && Number(adaptiveBitrate) > 0))
    ))
    && (audioOnly || silent || metadataValid)
    && (audioOnly || (limiterValid && overlaysValid && imageValid)) && (!canEditAudio || loudnessValid)
    && (audioOnly || aspect !== 'custom' || (
      positive(aspectNum) && positive(aspectDen)
      && Number(aspectNum) <= 2147483647 && Number(aspectDen) <= 2147483647
    ))
    && (!mjpeg || !jpegQuality || (positive(jpegQuality) && Number(jpegQuality) <= 100))
    && (audioOnly || rateMode !== 'custom' || (
      positive(rateNum) && positive(rateDen) && Number(rateNum) <= 4294967295
      && Number(rateDen) <= 4294967295 && Number(rateNum) >= Number(rateDen)
      && Number(rateNum) / Number(rateDen) <= 60
    )))
  $effect(() => {
    const owner = token
    ++generation; controller?.abort(); controller = null
    opened = false
    entries = []
    selected = ''
    busy = false
    error = ''
    query = ''
    favorites = false
    resetDraft()
    return () => { void owner; ++generation; controller?.abort() }
  })
  function resetDraft() {
    name = ''
    manualName = ''
    format = ''
    mxfCodec = ''
    width = ''
    height = ''
    bitrate = ''
    bitrateMode = ''
    maxBitrate = ''
    adaptiveBitrate = ''
    keyframes = ''
    profile = ''
    multiplexer = ''
    matchSource = false
    scale = '1'
    rateMode = 'preset'
    rateNum = '30000'
    rateDen = '1001'
    scaling = ''
    aspect = 'preset'
    aspectNum = '1'
    aspectDen = '1'
    renderQuality = 'preset'
    intraProfile = ''
    jpegQuality = ''
    metadata = {}
    imageMode = 'preset'
    imageOverlay = {}
    overlayModes = {}
    textOverlays = { name_overlay: {}, timecode_overlay: {} }
    limiter = 'preset'
    limiterMin = undefined
    limiterMax = undefined
    loudness = 'preset'
    loudnessTarget = undefined
    loudnessPeak = undefined
    audio = 'preset'
    audioCodec = ''
    audioRate = ''
    audioChannels = ''
    audioBits = ''
    audioBitrate = ''
    captions = 'preset'
    h264Level = 'preset'
    sidecar = 'preset'
  }
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
    if (format) settings.format = format
    if (mxf && (format || mxfCodec)) {
      settings.mxfVideoCodec = effectiveMxfCodec
      if (effectiveMxfCodec === 'dnxhr') settings.dnxProfile = 'hq'
      if (effectiveMxfCodec === 'proRes') settings.proresProfile = 'hq'
    }
    if (effectiveFormat === 'hevc' && effectiveBitrateMode === 'vbr2Pass') settings.bitrateMode = 'vbr1Pass'
    if (matchSource) { settings.frameSize = null; settings.scale = Number(scale) }
    else if (width && height) settings.frameSize = [Number(width), Number(height)]
    if (!audioOnly) {
      if (rateMode === 'source') settings.frameRate = null
      if (rateMode === 'custom') settings.frameRate = { num: Number(rateNum), den: Number(rateDen) }
      if (scaling) settings.scaling = scaling
      if (aspect === 'square') settings.pixelAspect = null
      if (aspect === 'custom') settings.pixelAspect = [Number(aspectNum), Number(aspectDen)]
      if (renderQuality !== 'preset') settings.maxRenderQuality = renderQuality === 'maximum'
      if (selectedIntraProfile && profileGroup) settings[profileGroup.field] = selectedIntraProfile
      if (mjpeg && jpegQuality) settings.quality = Number(jpegQuality)
    }
    if (!audioOnly && !silent && audio !== 'preset') settings.includeAudio = audio === 'include'
    const effects: Record<string, unknown> = {}
    if (!audioOnly) {
      if (imageMode === 'on') effects.image_overlay = { ...imageOverlay, source_id: imageValue.source_id, path: '', enabled: true }
      if (imageMode === 'off') effects.image_overlay = { enabled: false }
      for (const { key } of overlayFields) {
        if (overlayModes[key] === 'on') effects[key] = { ...textOverlays[key], enabled: true }
        if (overlayModes[key] === 'off') effects[key] = { enabled: false }
      }
    }
    if (!audioOnly && limiter !== 'preset') {
      effects.video_limiter = limiter === 'on'
        ? { enabled: true, min_percent: limiterMin, max_percent: limiterMax }
        : { enabled: false }
    }
    if (canEditAudio && loudness !== 'preset') {
      effects.loudness = loudness === 'on'
        ? { enabled: true, target_lufs: loudnessTarget, true_peak_dbtp: loudnessPeak }
        : { enabled: false }
    }
    if (Object.keys(effects).length) settings.effects = effects
    if (!audioOnly && !silent && Object.keys(metadata).length) settings.metadata = { ...metadata }
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
    if (h264Level !== 'preset' && h264) settings.h264Level = h264Level === 'auto' ? null : Number(h264Level)
    return settings
  }
  function save(overwrite: boolean) {
    const preset = picked
    if (!valid || !preset) return
    const settings = draftSettings()
    if (overwrite ? preset.builtin || !Object.keys(settings).length : !name.trim()) return
    const target = overwrite ? preset.name : name
    void run(async (owner, signal) => {
      const result = await editFilmcraftExportPresets<{ name: string }>(owner, {
        action: 'save', name: target, from: preset.name, settings, overwrite,
        ...(overwrite ? { category: preset.category, description: preset.description } : {}),
      }, signal)
      if (signal.aborted) return
      onlibrarychange?.({ saved: overwrite ? result.name : undefined })
      await refresh(owner, signal)
      if (!signal.aborted) {
        query = ''
        favorites = false
        selected = result.name
        resetDraft()
      }
    })
  }
  function saveManual() {
    const snapshot = manualSnapshot?.()
    if (!snapshot || !manualName.trim()) return
    const name = manualName
    void run(async (owner, signal) => {
      const result = await editFilmcraftExportPresets<{ name: string }>(owner, {
        action: 'save', name, ...snapshot, overwrite: false,
      }, signal)
      if (signal.aborted) return
      onlibrarychange?.({})
      await refresh(owner, signal)
      if (signal.aborted) return
      query = ''
      favorites = false
      selected = result.name
      resetDraft()
    })
  }
  function favorite() {
    if (!picked) return
    const name = picked.name
    void run(async (owner, signal) => {
      await editFilmcraftExportPresets(owner, { action: 'favorite', name }, signal)
      if (signal.aborted) return
      onlibrarychange?.({})
      await refresh(owner, signal)
    })
  }
  function remove() {
    if (!picked || picked.builtin) return
    const name = picked.name
    void run(async (owner, signal) => {
      await editFilmcraftExportPresets(owner, { action: 'delete', name }, signal)
      if (signal.aborted) return
      onlibrarychange?.({ deleted: name })
      await refresh(owner, signal)
    })
  }
  function importFile(file: File | undefined) {
    if (!file) return
    void run(async (owner, signal) => {
      if (file.size > 1024 * 1024) throw new Error('Файл пресетов должен быть не больше 1 МБ')
      const contents = await file.text(); if (signal.aborted) return
      await editFilmcraftExportPresets(owner, { action: 'import', file: JSON.parse(contents) as FilmcraftPresetFile }, signal)
      if (signal.aborted) return
      onlibrarychange?.({})
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
    {#if manualSnapshot}
      <label>Имя пресета для текущих настроек <input aria-label="Имя пресета текущих настроек Filmcraft" bind:value={manualName} disabled={busy || disabled} /></label>
      <button type="button" disabled={busy || disabled || !manualName.trim()} onclick={saveManual}>Сохранить текущие настройки</button>
    {/if}
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
      <label>Формат <select aria-label="Формат пресета Filmcraft" value={format} onchange={event => { format = event.currentTarget.value }} disabled={busy || disabled}>
        <option value="">Как в исходном пресете</option>
        {#each formats as [value, label] (value)}<option {value}>{label}</option>{/each}
      </select></label>
      {#if effectiveFormat === 'mxf-op1a'}
        <label>Кодек MXF <select aria-label="Кодек MXF пресета Filmcraft" value={mxfCodec} onchange={event => { mxfCodec = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="">Как в исходном пресете</option>
          <option value="dnxhr">DNxHR HQ</option><option value="proRes">ProRes 422 HQ</option><option value="h264">H.264</option>
        </select></label>
      {/if}
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
        <label>Пропорции пикселя <select aria-label="Пропорции пикселя пресета Filmcraft" value={aspect} onchange={event => { aspect = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="preset">Как в исходном пресете</option>
          <option value="square">Квадратный пиксель</option>
          <option value="custom">Свои пропорции</option>
        </select></label>
        {#if aspect === 'custom'}
          <label>Числитель <input aria-label="Числитель пропорций пикселя пресета Filmcraft" inputmode="numeric" bind:value={aspectNum} disabled={busy || disabled} /></label>
          <label>Знаменатель <input aria-label="Знаменатель пропорций пикселя пресета Filmcraft" inputmode="numeric" bind:value={aspectDen} disabled={busy || disabled} /></label>
        {/if}
        <label>Качество рендера <select aria-label="Качество рендера пресета Filmcraft" value={renderQuality} onchange={event => { renderQuality = event.currentTarget.value }} disabled={busy || disabled}>
          <option value="preset">Как в исходном пресете</option>
          <option value="normal">Обычное</option><option value="maximum">Максимальное</option>
        </select></label>
        {#if profileGroup}
          <label>Профиль видео <select aria-label="Профиль видеокодека пресета Filmcraft" value={selectedIntraProfile} onchange={event => { intraProfile = event.currentTarget.value }} disabled={busy || disabled}>
            <option value="">Как в исходном пресете</option>
            {#each profileGroup.options as [value, label] (value)}<option {value}>{label}</option>{/each}
          </select></label>
        {/if}
        {#if mjpeg}
          <label>Качество Motion JPEG <input aria-label="Качество Motion JPEG пресета Filmcraft" inputmode="numeric" bind:value={jpegQuality} disabled={busy || disabled} /></label>
        {/if}
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
      {#if h264}
        <label>Уровень H.264 <select aria-label="Уровень H.264 нового пресета Filmcraft" value={h264Level} onchange={(event) => { const value = event.currentTarget.value; if (value === 'preset' || value === 'auto' || levels.includes(Number(value))) h264Level = value }} disabled={busy || disabled}>
          <option value="preset">Как в исходном пресете</option><option value="auto">Автоматически</option>
          {#each levels as level (level)}<option value={String(level)}>{(level / 10).toFixed(1)}</option>{/each}
        </select></label>
        <p>Если выбранный уровень слишком низкий для видео, энкодер повысит его автоматически.</p>
      {/if}
      <details>
        <summary>Эффекты пресета</summary>
        {#if !audioOnly}
          <fieldset disabled={busy || disabled}>
            <legend>Изображение поверх видео</legend>
            <select aria-label="Изображение в пресете Filmcraft" value={imageMode} onchange={event => { imageMode = event.currentTarget.value }}>
              <option value="preset">Как в исходном пресете</option><option value="on">Включить</option><option value="off">Выключить</option>
            </select>
            {#if imageMode === 'on'}
              <label>Изображение <select aria-label="Источник изображения пресета Filmcraft" value={imageValue.source_id}
                onchange={event => { imageOverlay.source_id = event.currentTarget.value }}>
                <option value="">Выберите изображение</option>
                {#each imageSources as source (source.id)}
                  <option value={source.id}>{source.title || source.originalFilename || source.filename}</option>
                {/each}
              </select></label>
              <label>Положение <select aria-label="Изображение: положение в пресете Filmcraft" value={imageValue.placement}
                onchange={event => { imageOverlay.placement = event.currentTarget.value as FilmcraftOverlayPlacement }}>
                {#each exportOverlayPlacements as [value, name] (value)}<option {value}>{name}</option>{/each}
              </select></label>
              {#each ['X', 'Y'] as axis, index (axis)}
                <label>Смещение {axis}, px <input aria-label={`Изображение: ${axis} в пресете Filmcraft`} type="number" step="0.1" value={imageValue.offset[index]}
                  oninput={event => {
                    const offset: [number, number] = [...imageValue.offset]
                    offset[index] = event.currentTarget.valueAsNumber
                    imageOverlay.offset = offset
                  }} /></label>
              {/each}
              <label>Размер, % ширины <input aria-label="Изображение: размер в пресете Filmcraft" type="number" min="0.5" max="100" step="0.1" value={imageValue.size_percent}
                oninput={event => { imageOverlay.size_percent = event.currentTarget.valueAsNumber }} /></label>
              <label>Непрозрачность, % <input aria-label="Изображение: непрозрачность в пресете Filmcraft" type="number" min="0" max="100" step="0.1" value={imageValue.opacity}
                oninput={event => { imageOverlay.opacity = event.currentTarget.valueAsNumber }} /></label>
              {#if !imageValid}<p role="alert">Выберите изображение из библиотеки и проверьте параметры наложения.</p>{/if}
            {/if}
          </fieldset>
          {#each overlayFields as { key, label, textLabel } (key)}
            <fieldset disabled={busy || disabled}>
              <legend>{label}</legend>
              <select aria-label={`${label} в пресете Filmcraft`} value={overlayModes[key] ?? 'preset'}
                onchange={event => { overlayModes[key] = event.currentTarget.value }}>
                <option value="preset">Как в исходном пресете</option><option value="on">Включить</option><option value="off">Выключить</option>
              </select>
              {#if overlayModes[key] === 'on'}
                {@const current = overlayValue(key)}
                <label>{textLabel}<textarea aria-label={`${label}: текст пресета Filmcraft`} maxlength="16000" value={current.text}
                  oninput={event => { textOverlays[key].text = event.currentTarget.value }}></textarea></label>
                <label>Положение <select aria-label={`${label}: положение в пресете Filmcraft`} value={current.placement}
                  onchange={event => { textOverlays[key].placement = event.currentTarget.value as FilmcraftOverlayPlacement }}>
                  {#each exportOverlayPlacements as [value, name] (value)}<option {value}>{name}</option>{/each}
                </select></label>
                {#each ['X', 'Y'] as axis, index (axis)}
                  <label>Смещение {axis}, px <input aria-label={`${label}: ${axis} в пресете Filmcraft`} type="number" step="0.1" value={current.offset[index]}
                    oninput={event => {
                      const offset: [number, number] = [...current.offset]
                      offset[index] = event.currentTarget.valueAsNumber
                      textOverlays[key].offset = offset
                    }} /></label>
                {/each}
                <label>Размер, % высоты <input aria-label={`${label}: размер в пресете Filmcraft`} type="number" min="1" max="50" step="0.1" value={current.size_percent}
                  oninput={event => { textOverlays[key].size_percent = event.currentTarget.valueAsNumber }} /></label>
                <label>Непрозрачность, % <input aria-label={`${label}: непрозрачность в пресете Filmcraft`} type="number" min="0" max="100" step="0.1" value={current.opacity}
                  oninput={event => { textOverlays[key].opacity = event.currentTarget.valueAsNumber }} /></label>
              {/if}
            </fieldset>
          {/each}
          {#if !overlaysValid}<p role="alert">Проверьте текст и числовые параметры наложений.</p>{/if}
          <label>Ограничитель яркости <select aria-label="Ограничитель яркости пресета Filmcraft" value={limiter} disabled={busy || disabled}
            onchange={event => {
              limiter = event.currentTarget.value
              if (limiter === 'on') {
                limiterMin ??= savedEffects?.video_limiter?.min_percent ?? 0
                limiterMax ??= savedEffects?.video_limiter?.max_percent ?? 100
              }
            }}>
            <option value="preset">Как в исходном пресете</option><option value="on">Включить</option><option value="off">Выключить</option>
          </select></label>
          {#if limiter === 'on'}
            <label>Нижний порог, % <input aria-label="Нижний порог яркости пресета Filmcraft" type="number" min="0" max="100" step="0.1" bind:value={limiterMin} disabled={busy || disabled} /></label>
            <label>Верхний порог, % <input aria-label="Верхний порог яркости пресета Filmcraft" type="number" min="0" max="100" step="0.1" bind:value={limiterMax} disabled={busy || disabled} /></label>
            {#if !limiterValid}<p role="alert">Укажите пороги от 0 до 100 %.</p>{/if}
          {/if}
        {/if}
        {#if canEditAudio}
          <label>Нормализация громкости <select aria-label="Нормализация громкости пресета Filmcraft" value={loudness} disabled={busy || disabled}
            onchange={event => {
              loudness = event.currentTarget.value
              if (loudness === 'on') {
                loudnessTarget ??= savedEffects?.loudness?.target_lufs ?? -23
                loudnessPeak ??= savedEffects?.loudness?.true_peak_dbtp ?? -2
              }
            }}>
            <option value="preset">Как в исходном пресете</option><option value="on">Включить</option><option value="off">Выключить</option>
          </select></label>
          {#if loudness === 'on'}
            <label>Целевая громкость, LUFS <input aria-label="Целевая громкость пресета Filmcraft" type="number" step="0.1" bind:value={loudnessTarget} disabled={busy || disabled} /></label>
            <label>Предел true peak, dBTP <input aria-label="Предел true peak пресета Filmcraft" type="number" min="-24" max="0" step="0.1" bind:value={loudnessPeak} disabled={busy || disabled} /></label>
            {#if !loudnessValid}<p role="alert">Укажите целевую громкость и предел true peak от −24 до 0 dBTP.</p>{/if}
          {/if}
        {/if}
      </details>
      {#if !audioOnly && !silent}
        <details>
          <summary>Метаданные пресета</summary>
          {#each exportMetadataFields as field (field.key)}
            <label>{field.label}
              {#if field.multiline}
                <textarea aria-label={`${field.label} в пресете Filmcraft`} rows="2" maxlength="16384"
                  value={metadataValue(field.key)} oninput={event => { metadata[field.key] = event.currentTarget.value }}
                  disabled={busy || disabled}></textarea>
              {:else}
                <input aria-label={`${field.label} в пресете Filmcraft`} type="text" maxlength="16384"
                  value={metadataValue(field.key)} oninput={event => { metadata[field.key] = event.currentTarget.value }}
                  disabled={busy || disabled} />
              {/if}
            </label>
          {/each}
          {#if Object.keys(metadata).length}
            <button type="button" onclick={() => { metadata = {} }} disabled={busy || disabled}>Восстановить метаданные пресета</button>
          {/if}
          {#if !metadataValid}<p role="alert">Сократите текст метаданных файла.</p>{/if}
        </details>
      {/if}
      <button type="button" disabled={busy || disabled || !valid || !Object.keys(draftSettings()).length}
        onclick={() => onapply(selected, draftSettings())}>Применить правки к экспорту</button>
      <button type="button" disabled={busy || disabled || !valid || !name.trim()} onclick={() => save(false)}>Сохранить копию пресета</button>
      {#if !picked.builtin}
        <button type="button" disabled={busy || disabled || !valid || !Object.keys(draftSettings()).length}
          onclick={() => save(true)}>Сохранить изменения пресета</button>
      {/if}
    {/if}
    <label>Импорт пресетов <input aria-label="Импорт пресетов Filmcraft" type="file" accept="application/json,.json" disabled={busy || disabled} onchange={(event) => { importFile(event.currentTarget.files?.[0]); event.currentTarget.value = '' }} /></label>
    {#if busy}<span role="status">Загрузка пресетов…</span>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  {/if}
</section>
