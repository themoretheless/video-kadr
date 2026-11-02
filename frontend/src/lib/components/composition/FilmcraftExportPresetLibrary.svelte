<script lang="ts">
  import { listFilmcraftExportPresets, editFilmcraftExportPresets, type FilmcraftLibraryPreset, type FilmcraftPresetFile } from '$lib/api'
  let { token, disabled = false, onapply }: { token: string | null; disabled?: boolean; onapply: (name: string) => void } = $props()
  let opened = $state(false)
  let entries = $state<FilmcraftLibraryPreset[]>([])
  let selected = $state('')
  let query = $state('')
  let favorites = $state(false)
  let name = $state('')
  let width = $state('')
  let height = $state('')
  let bitrate = $state('')
  let matchSource = $state(false)
  let scale = $state('1')
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
  const filtered = $derived(entries.filter(p => (!favorites || p.favorite) && [p.name, p.category, p.description].some(v => v.toLowerCase().includes(query.toLowerCase()))))
  $effect(() => { if (opened && !filtered.some(p => p.name === selected)) selected = filtered[0]?.name ?? '' })
  const positive = (value: string) => /^\d+$/.test(value) && Number.isSafeInteger(Number(value)) && Number(value) > 0
  const valid = $derived(name.trim().length > 0 && (matchSource ? Number.isFinite(Number(scale)) && Number(scale) > 0 : ((!width && !height) || (positive(width) && positive(height)))) && (!bitrate || positive(bitrate)))
  $effect(() => {
    const owner = token
    ++generation; controller?.abort(); controller = null
    opened = false; entries = []; selected = ''; busy = false; error = ''; query = ''; favorites = false; name = ''; width = ''; height = ''; bitrate = ''; matchSource = false; scale = '1'; captions = 'preset'; h264Level = 'preset'; sidecar = 'preset'
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
  function save() {
    if (!valid || !picked) return
    const settings: Record<string, unknown> = {}
    if (matchSource) { settings.frameSize = null; settings.scale = Number(scale) }
    else if (width && height) settings.frameSize = [Number(width), Number(height)]
    if (bitrate) { settings.bitrateKbps = Number(bitrate); settings.adaptiveBitrate = null }
    if (captions !== 'preset') settings.burnCaptions = captions === 'burn'
    if (sidecar !== 'preset') settings.captionSidecar = sidecar === 'none' ? null : sidecar
    if (h264Level !== 'preset' && (picked.settings.format === 'h264' || picked.settings.mxfVideoCodec === 'h264')) settings.h264Level = h264Level === 'auto' ? null : Number(h264Level)
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
      <label>Битрейт, кбит/с <input aria-label="Битрейт нового пресета Filmcraft" inputmode="numeric" bind:value={bitrate} disabled={busy || disabled} /></label>
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
      <button type="button" disabled={busy || disabled || !valid} onclick={save}>Сохранить копию пресета</button>
    {/if}
    <label>Импорт пресетов <input aria-label="Импорт пресетов Filmcraft" type="file" accept="application/json,.json" disabled={busy || disabled} onchange={(event) => { importFile(event.currentTarget.files?.[0]); event.currentTarget.value = '' }} /></label>
    {#if busy}<span role="status">Загрузка пресетов…</span>{/if}
    {#if error}<p role="alert">{error}</p>{/if}
  {/if}
</section>
