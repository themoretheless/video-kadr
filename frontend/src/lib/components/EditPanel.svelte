<script lang="ts">
  import TimelineEditor from './TimelineEditor.svelte'
  import TrimSlider from './TrimSlider.svelte'
  import CurvesEditor from './edit/CurvesEditor.svelte'
  import AudioEffects from './edit/AudioEffects.svelte'
  import ExportControls from './edit/ExportControls.svelte'
  import HslColorWheels from './edit/HslColorWheels.svelte'
  import LutControl from './edit/LutControl.svelte'
  import type { Preset } from '$lib/state/store.svelte.js'
  import {
    applyPreset,
    beginEditTransaction,
    deletePreset,
    endEditTransaction,
    history,
    normalizeCrop,
    parseTime,
    presets,
    redo,
    resetColor,
    savePreset,
    setTrimEndFromPlayer,
    setTrimStartFromPlayer,
    state as appState,
    undo,
  } from '$lib/state/store.svelte.js'

  let presetName = $state('')
  let startStr = $state('')
  let endStr = $state('')
  let startInvalid = $state(false)
  let endInvalid = $state(false)
  let editingStart = false
  let editingEnd = false

  let canUndo = $derived(history.past.length > 0)
  let canRedo = $derived(history.future.length > 0)
  let duration = $derived(appState.video?.duration ?? 0)
  let selectedDuration = $derived(Math.max(0, appState.edit.trimEnd - appState.edit.trimStart))
  let keptDuration = $derived.by(() => {
    const edit = appState.edit
    const start = Math.max(edit.trimStart, Math.min(edit.cut.start, edit.trimEnd))
    const end = Math.max(edit.trimStart, Math.min(edit.cut.end, edit.trimEnd))
    return Math.max(0, selectedDuration - Math.max(0, end - start))
  })
  let curvesCapability = $derived(appState.capabilities?.filters?.find((option) =>
    ['curves', 'color-curves', 'custom-curves'].includes(option.id.toLowerCase()),
  ))
  let curvesUnavailableReason = $derived(!appState.capabilities ? '' : !curvesCapability
    ? 'Нужен обновлённый сервер с поддержкой кривых'
    : curvesCapability.available ? '' : curvesCapability.reason || 'Кривые недоступны в текущей сборке сервера')
  let chromaCapability = $derived(appState.capabilities?.filters?.find((option) =>
    option.id.toLowerCase() === 'chroma-key',
  ))
  let chromaUnavailableReason = $derived(!appState.capabilities ? '' : !chromaCapability
    ? 'Нужен обновлённый сервер с поддержкой chroma key'
    : chromaCapability.available ? '' : chromaCapability.reason || 'Chroma key недоступен в текущей сборке сервера')
  let chromaSpillCapability = $derived(appState.capabilities?.filters?.find((option) =>
    option.id.toLowerCase() === 'chroma-spill',
  ))
  let chromaSpillUnavailableReason = $derived(!appState.capabilities ? '' : !chromaSpillCapability
    ? 'Нужен обновлённый сервер с поддержкой подавления chroma spill'
    : chromaSpillCapability.available ? '' : chromaSpillCapability.reason || 'Подавление chroma spill недоступно в текущей сборке сервера')

  const speeds = [0.5, 0.75, 1, 1.25, 1.5, 2]

  function commitSpeed(event: Event): void {
    const value = Number((event.currentTarget as HTMLInputElement).value)
    if (Number.isFinite(value)) appState.edit.speed = Math.min(16, Math.max(0.05, value))
  }
  const widthPresets = [
    { label: '1080p', w: 1920 }, { label: '720p', w: 1280 },
    { label: '480p', w: 854 }, { label: '360p', w: 640 },
  ]
  const aspects = [
    { label: '9:16', rw: 9, rh: 16 }, { label: '1:1', rw: 1, rh: 1 },
    { label: '4:5', rw: 4, rh: 5 }, { label: '4:3', rw: 4, rh: 3 },
    { label: '16:9', rw: 16, rh: 9 },
  ]
  const rotations = [0, 90, 180, 270]
  const filters = [
    { v: '', label: 'Нет' }, { v: 'grayscale', label: 'Ч/Б' },
    { v: 'sepia', label: 'Сепия' }, { v: 'warm', label: 'Тёплый' },
    { v: 'cold', label: 'Холодный' }, { v: 'teal-orange', label: 'Teal-Orange' },
    { v: 'faded', label: 'Выцветший' }, { v: 'noir', label: 'Нуар' },
    { v: 'vintage', label: 'Винтаж' },
  ]
  const fpsPresets = [
    { v: null as number | null, label: 'ориг.' }, { v: 60, label: '60' },
    { v: 30, label: '30' }, { v: 24, label: '24' }, { v: 15, label: '15' },
  ]
  const censorColors = [
    { v: 'black', label: 'Чёрный' }, { v: 'white', label: 'Белый' }, { v: 'gray', label: 'Серый' },
  ]
  const padAspects = [
    { v: '', label: 'Нет' }, { v: '9:16', label: '9:16' }, { v: '1:1', label: '1:1' },
    { v: '4:5', label: '4:5' }, { v: '16:9', label: '16:9' },
  ]

  function fmt(time: number): string {
    if (!Number.isFinite(time)) return '0:00.0'
    const minutes = Math.floor(time / 60)
    const remainder = time - minutes * 60
    let seconds = Math.floor(remainder)
    let decisecond = Math.round((remainder - seconds) * 10)
    if (decisecond === 10) { decisecond = 0; seconds += 1 }
    return `${minutes}:${String(seconds).padStart(2, '0')}.${decisecond}`
  }
  $effect(() => { if (!editingStart) startStr = fmt(appState.edit.trimStart) })
  $effect(() => { if (!editingEnd) endStr = fmt(appState.edit.trimEnd) })
  $effect(() => {
    if (!appState.edit.cutEnabled) return
    const { trimStart, trimEnd, cut } = appState.edit
    if (!(cut.end > cut.start) || cut.start < trimStart || cut.end > trimEnd) {
      const span = trimEnd - trimStart
      appState.edit.cut = { start: trimStart + span / 3, end: trimStart + span * 2 / 3 }
    }
  })
  $effect(() => {
    if (!appState.edit.censorEnabled || !appState.video) return
    if (!(appState.edit.censor.w > 1 && appState.edit.censor.h > 1)) {
      appState.edit.censor = {
        x: Math.round(appState.video.width * .3), y: Math.round(appState.video.height * .3),
        w: Math.round(appState.video.width * .4), h: Math.round(appState.video.height * .25),
      }
    }
  })

  function onSavePreset(): void { savePreset(presetName); presetName = '' }
  function commitStart(): void {
    editingStart = false
    const value = parseTime(startStr)
    if (value === null) { startInvalid = true; startStr = fmt(appState.edit.trimStart); return }
    startInvalid = false
    appState.edit.trimStart = Math.max(0, Math.min(value, appState.edit.trimEnd - .1))
    startStr = fmt(appState.edit.trimStart)
  }
  function commitEnd(): void {
    editingEnd = false
    const value = parseTime(endStr)
    if (value === null) { endInvalid = true; endStr = fmt(appState.edit.trimEnd); return }
    endInvalid = false
    appState.edit.trimEnd = Math.min(duration, Math.max(value, appState.edit.trimStart + .1))
    endStr = fmt(appState.edit.trimEnd)
  }
  function setWidth(width: number): void {
    appState.edit.scaleEnabled = true
    appState.edit.scale = { w: width, h: -2 }
  }
  function setAspect(ratioWidth: number, ratioHeight: number): void {
    const video = appState.video
    if (!video) return
    appState.edit.cropEnabled = true
    appState.edit.cropAspectLock = `${ratioWidth}:${ratioHeight}` as typeof appState.edit.cropAspectLock
    const ratio = ratioWidth / ratioHeight
    let width = video.width
    let height = Math.round(width / ratio)
    if (height > video.height) { height = video.height; width = Math.round(height * ratio) }
    width -= width % 2
    height -= height % 2
    appState.edit.crop = { x: Math.floor((video.width - width) / 2), y: Math.floor((video.height - height) / 2), w: width, h: height }
  }
  function resetCrop(): void {
    appState.edit.cropAspectLock = ''
    if (appState.video) appState.edit.crop = { x: 0, y: 0, w: appState.video.width, h: appState.video.height }
  }
  function unlockCrop(): void { appState.edit.cropAspectLock = '' }
  function normalizeCropDimension(driver: 'width' | 'height'): void {
    const lock = appState.edit.cropAspectLock
    if (!lock) return normalizeCrop()
    const [width, height] = lock.split(':').map(Number)
    const crop = appState.edit.crop
    if (driver === 'width') crop.h = Math.round(crop.w * height! / width!)
    else crop.w = Math.round(crop.h * width! / height!)
    normalizeCrop()
  }
  const filterCapability = (id: string) => id ? appState.capabilities?.filters.find((option) => option.id === id) : undefined
  function filterUnavailableReason(id: string): string | undefined {
    const option = filterCapability(id)
    return option && !option.available ? option.reason || 'Недоступно в текущей сборке' : undefined
  }
  function selectFilter(id: string): void { if (!filterUnavailableReason(id)) appState.edit.filter = id }
  function applyPlatform(name: string): void {
    if (!appState.video) return
    appState.edit.format = 'mp4'; appState.edit.codec = 'h264'; appState.edit.fps = null
    if (name === 'shorts' || name === 'reels') {
      setAspect(9, 16); setWidth(1080); appState.edit.fps = 30
    } else {
      appState.edit.cropEnabled = false
      setWidth(name === 'youtube' ? 1920 : 1280)
    }
  }
</script>

<div class="card edit">
  <div class="edit-head">
    <h2>Редактирование</h2>
    <div class="history-btns">
      <button class="btn ghost sm" disabled={!canUndo} title="Отменить (Cmd/Ctrl+Z)" onclick={undo}>↶ Отменить</button>
      <button class="btn ghost sm" disabled={!canRedo} title="Повторить (Cmd/Ctrl+Shift+Z)" onclick={redo}>↷ Повторить</button>
    </div>
  </div>

  <section class="group">
    <div class="group-title">Пресеты эффектов</div>
    <div class="field">
      <div class="preset-row">
        <input class="time-input preset-name" bind:value={presetName} placeholder="Имя пресета" onkeydown={(event) => { if (event.key === 'Enter') onSavePreset() }} />
        <button class="btn ghost sm" disabled={!presetName.trim()} onclick={onSavePreset}>Сохранить</button>
      </div>
      {#if presets.list.length}
        <div class="chips preset-chips">
          {#each presets.list as preset (preset.name)}
            <span class="preset-chip">
              <button class="chip" title={`Применить «${preset.name}»`} onclick={() => void applyPreset(preset as Preset)}>{preset.name}</button>
              <button class="preset-del" title={`Удалить «${preset.name}»`} onclick={() => deletePreset(preset.name)}>×</button>
            </span>
          {/each}
        </div>
      {:else}<p class="hint">Сохрани текущий образ (цвет, скорость и звук) и применяй к другим клипам.</p>{/if}
    </div>
  </section>

  <section class="group">
    <div class="group-title">Время</div>
    <TimelineEditor />
    {#if !appState.edit.timelineEnabled}
      <div class="field">
        <div class="trim-head"><span class="trim-label">Обрезка</span><span class="trim-dur">выбрано {fmt(selectedDuration)}</span></div>
        <TrimSlider min={0} max={duration} start={appState.edit.trimStart} end={appState.edit.trimEnd} onstartchange={(start) => { appState.edit.trimStart = start }} onendchange={(end) => { appState.edit.trimEnd = end }} />
        <div class="trim-times">
          <label class="tt"><span>Начало</span><input class:invalid={startInvalid} class="time-input" bind:value={startStr} onfocus={() => { editingStart = true; startInvalid = false }} onblur={commitStart} onkeydown={(event) => { if (event.key === 'Enter') commitStart() }} /></label>
          <div class="pos-btns">
            <button class="btn ghost sm" title="Начало от позиции плеера" onclick={setTrimStartFromPlayer}>⟦ от позиции</button>
            <button class="btn ghost sm" title="Конец от позиции плеера" onclick={setTrimEndFromPlayer}>до позиции ⟧</button>
          </div>
          <label class="tt right"><span>Конец</span><input class:invalid={endInvalid} class="time-input" bind:value={endStr} onfocus={() => { editingEnd = true; endInvalid = false }} onblur={commitEnd} onkeydown={(event) => { if (event.key === 'Enter') commitEnd() }} /></label>
        </div>
      </div>
      <div class="field">
        <label class="toggle"><input type="checkbox" bind:checked={appState.edit.cutEnabled} /> Вырезать кусок из середины</label>
        {#if appState.edit.cutEnabled}
          <TrimSlider min={appState.edit.trimStart} max={appState.edit.trimEnd} start={appState.edit.cut.start} end={appState.edit.cut.end} onstartchange={(start) => { appState.edit.cut.start = start }} onendchange={(end) => { appState.edit.cut.end = end }} />
          <p class="hint">Удаляем {fmt(Math.max(0, appState.edit.cut.end - appState.edit.cut.start))}, останется {fmt(keptDuration)}. Доступно для MP4/WebM/AV1/ProRes.</p>
        {/if}
      </div>
    {/if}
    <div class="field">
      <span class="field-label">Скорость</span>
      <div class="chips">{#each speeds as speed (speed)}<button class:active={appState.edit.speed === speed} class="chip" onclick={() => { appState.edit.speed = speed }}>{speed}×</button>{/each}</div>
      <label class="tt"><span>Точная скорость</span><input aria-label="Точная скорость" type="number" min="0.05" max="16" step="0.05" value={appState.edit.speed} onchange={commitSpeed} /></label>
    </div>
    <div class="field inline"><label class="toggle"><input type="checkbox" bind:checked={appState.edit.reverse} /> Реверс</label>{#if appState.edit.reverse}<span class="hint">короткие отрезки: реверс грузит весь клип в память</span>{/if}</div>
    <div class="field"><div class="grid2">
      <label>Появление: {appState.edit.fadeIn.toFixed(1)} c <input type="range" min="0" max="5" step="0.1" bind:value={appState.edit.fadeIn} /></label>
      <label>Затухание: {appState.edit.fadeOut.toFixed(1)} c <input type="range" min="0" max="5" step="0.1" bind:value={appState.edit.fadeOut} /></label>
    </div></div>
  </section>

  <section class="group">
    <div class="group-title">Кадр</div>
    <div class="field">
      <label class="toggle"><input type="checkbox" bind:checked={appState.edit.scaleEnabled} /> Изменить размер</label>
      {#if appState.edit.scaleEnabled}<div class="chips">{#each widthPresets as preset (preset.w)}<button class:active={appState.edit.scale.w === preset.w} class="chip" onclick={() => setWidth(preset.w)}>{preset.label}</button>{/each}</div><p class="hint">Ширина {appState.edit.scale.w}px, высота пропорционально.</p>{/if}
    </div>
    <div class="field">
      <label class="toggle"><input type="checkbox" bind:checked={appState.edit.cropEnabled} /> Кадрировать</label>
      {#if appState.edit.cropEnabled}
        <div class="chips">{#each aspects as aspect (aspect.label)}<button class:active={appState.edit.cropAspectLock === aspect.label} class="chip" onclick={() => appState.edit.cropAspectLock === aspect.label ? unlockCrop() : setAspect(aspect.rw, aspect.rh)}>{aspect.label}</button>{/each}<button class="chip" onclick={resetCrop}>сброс</button></div>
        <div class="grid2">
          <label>X <input type="number" min="0" bind:value={appState.edit.crop.x} onblur={normalizeCrop} /></label><label>Y <input type="number" min="0" bind:value={appState.edit.crop.y} onblur={normalizeCrop} /></label>
          <label>Ширина <input type="number" min="2" bind:value={appState.edit.crop.w} onblur={() => normalizeCropDimension('width')} /></label><label>Высота <input type="number" min="2" bind:value={appState.edit.crop.h} onblur={() => normalizeCropDimension('height')} /></label>
        </div>
      {/if}
    </div>
    <div class="field"><span class="field-label">Поворот</span><div class="chips">{#each rotations as rotation (rotation)}<button class:active={appState.edit.rotate === rotation} class="chip" onclick={() => { appState.edit.rotate = rotation }}>{rotation}°</button>{/each}</div></div>
    <div class="field inline"><label class="toggle"><input type="checkbox" bind:checked={appState.edit.flipH} /> Отразить ↔</label><label class="toggle"><input type="checkbox" bind:checked={appState.edit.flipV} /> Отразить ↕</label></div>
    <div class="field"><span class="field-label">Частота кадров</span><div class="chips">{#each fpsPresets as fps (String(fps.v))}<button class:active={appState.edit.fps === fps.v} class="chip" onclick={() => { appState.edit.fps = fps.v }}>{fps.label}</button>{/each}</div></div>
    <div class="field">
      <label class="toggle"><input type="checkbox" bind:checked={appState.edit.censorEnabled} /> Замазать область</label>
      {#if appState.edit.censorEnabled}<div class="chips">{#each censorColors as color (color.v)}<button class:active={appState.edit.censorColor === color.v} class="chip" onclick={() => { appState.edit.censorColor = color.v }}>{color.label}</button>{/each}</div><p class="hint">Выдели красный прямоугольник прямо на видео.</p>{/if}
    </div>
    <div class="field"><span class="field-label">Поля под пропорции (letterbox)</span><div class="chips">{#each padAspects as pad (pad.v)}<button class:active={appState.edit.pad === pad.v} class="chip" onclick={() => { appState.edit.pad = pad.v }}>{pad.label}</button>{/each}</div></div>
  </section>

  <section class="group">
    <div class="group-title">Цвет</div>
    <div class="field"><div class="chips">{#each filters as filter (filter.v)}<button class:active={appState.edit.filter === filter.v} class="chip" aria-disabled={filterCapability(filter.v)?.available === false} aria-label={filterUnavailableReason(filter.v) ? `${filter.label}. ${filterUnavailableReason(filter.v)}` : filter.label} title={filterUnavailableReason(filter.v)} onclick={() => selectFilter(filter.v)}>{filter.label}</button>{/each}</div></div>
    <div class="field"><div class="grid2">
      <label>Яркость: {appState.edit.brightness.toFixed(2)} <input type="range" min="-1" max="1" step="0.05" bind:value={appState.edit.brightness} /></label>
      <label>Контраст: {appState.edit.contrast.toFixed(2)} <input type="range" min="0" max="2" step="0.05" bind:value={appState.edit.contrast} /></label>
      <label>Насыщенность: {appState.edit.saturation.toFixed(2)} <input type="range" min="0" max="3" step="0.05" bind:value={appState.edit.saturation} /></label>
      <button class="btn ghost sm reset-color" onclick={resetColor}>Сбросить цвет</button>
    </div></div>
    <div class="field chroma-key-controls">
      <label class="toggle"><input type="checkbox" bind:checked={appState.edit.chromaKeyEnabled} /> Chroma key</label>
      {#if appState.edit.chromaKeyEnabled}
        <div class="grid2">
          <label>Цвет фона <input type="color" bind:value={appState.edit.chromaKeyColor} aria-label="Цвет chroma key" /></label>
          <label>Сходство: {appState.edit.chromaKeySimilarity.toFixed(2)} <input type="range" min="0.01" max="1" step="0.01" bind:value={appState.edit.chromaKeySimilarity} /></label>
          <label>Мягкость края: {appState.edit.chromaKeyBlend.toFixed(2)} <input type="range" min="0" max="1" step="0.01" bind:value={appState.edit.chromaKeyBlend} /></label>
          <label>Подавление засветки: {appState.edit.chromaKeySpill.toFixed(2)} <input type="range" min="0" max="1" step="0.01" bind:value={appState.edit.chromaKeySpill} /></label>
        </div>
        {#if chromaUnavailableReason}<p class="hint" role="status">{chromaUnavailableReason}</p>{/if}
        {#if appState.edit.chromaKeySpill > 0 && chromaSpillUnavailableReason}<p class="hint" role="status">{chromaSpillUnavailableReason}</p>{/if}
        <p class="hint">Прозрачность chroma key видна в итоговом экспорте.</p>
      {/if}
    </div>
    <div class="advanced-color-stack">
      <LutControl />
      <HslColorWheels />
      {#if !curvesUnavailableReason}
        <CurvesEditor value={appState.edit.curves} onchange={(curves) => { appState.edit.curves = curves }} oninteractionstart={() => beginEditTransaction('curves')} oninteractionend={endEditTransaction} />
        <p class="advanced-color-note">Кривые не отображаются в предпросмотре; точный результат виден после экспорта.</p>
      {:else}<div class="color-tool is-unavailable curves-unavailable" role="status"><strong>Кривые недоступны</strong><span>{curvesUnavailableReason}</span></div>{/if}
    </div>
    <div class="field inline"><label class="toggle"><input type="checkbox" bind:checked={appState.edit.vignette} /> Виньетка</label><label class="toggle"><input type="checkbox" bind:checked={appState.edit.denoise} /> Шумодав</label></div>
    <div class="field"><div class="grid2">
      <label>Резкость: {appState.edit.sharpen.toFixed(1)} <input type="range" min="0" max="3" step="0.1" bind:value={appState.edit.sharpen} /></label>
      <label>Зерно: {Math.round(appState.edit.grain)} <input type="range" min="0" max="60" step="1" bind:value={appState.edit.grain} /></label>
    </div></div>
  </section>

  <section class="group">
    <div class="group-title">Звук</div>
    <div class="field inline"><label class="toggle"><input type="checkbox" bind:checked={appState.edit.mute} /> Без звука</label></div>
    {#if !appState.edit.mute}
      <div class="field"><label for="audio-volume">Громкость: {Math.round(appState.edit.volume * 100)}%</label><input id="audio-volume" type="range" min="0" max="2" step="0.05" aria-label="Громкость" bind:value={appState.edit.volume} /></div>
      <div class="field inline"><label class="toggle"><input type="checkbox" bind:checked={appState.edit.normalizeAudio} /> Нормализация громкости</label><label class="toggle"><input type="checkbox" bind:checked={appState.edit.highpass} /> Убрать гул (highpass)</label></div>
      <AudioEffects />
    {/if}
  </section>

  <ExportControls onplatform={applyPlatform} />
</div>
