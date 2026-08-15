<script lang="ts">
  import { CaptureController, formatElapsed, INITIAL_CAPTURE_SNAPSHOT } from './mediaRecorder.js'
  import { AnnotationModel } from './annotations.js'
  import type { AnnotationPoint, AnnotationSnapshot, AnnotationTool } from './annotations.js'
  import {
    advanceTeleprompter,
    configureTeleprompter,
    createTeleprompterState,
    pauseTeleprompter,
    resetTeleprompter,
    startTeleprompter,
  } from './teleprompter.js'
  import type { TeleprompterState } from './teleprompter.js'
  import type {
    CaptureCompleteHandler,
    CaptureErrorHandler,
    CaptureOptions,
    CaptureRuntime,
    CaptureSnapshot,
    DisplaySourcePreference,
  } from './types.js'

  interface Props {
    oncapture?: CaptureCompleteHandler
    oncaptureerror?: CaptureErrorHandler
    runtime?: CaptureRuntime
    countdownSeconds?: number
    title?: string
    class?: string
  }

  let {
    oncapture,
    oncaptureerror,
    runtime,
    countdownSeconds = 3,
    title = 'Запись экрана',
    class: className = '',
  }: Props = $props()

  let panelElement: HTMLElement
  let previewHost = $state.raw<HTMLDivElement>()
  let teleprompterContent = $state.raw<HTMLDivElement>()

  function deliverCapture(file: File): void | Promise<void> {
    panelElement?.dispatchEvent(new CustomEvent<File>('capture', { detail: file, bubbles: true }))
    return oncapture?.(file)
  }

  function deliverError(message: string): void {
    panelElement?.dispatchEvent(new CustomEvent<string>('captureerror', { detail: message, bubbles: true }))
    oncaptureerror?.(message)
  }

  let controller = $state.raw<CaptureController | null>(null)
  let snapshot = $state<CaptureSnapshot>(INITIAL_CAPTURE_SNAPSHOT)

  const annotationModel = new AnnotationModel()
  let annotationSnapshot = $state<AnnotationSnapshot>(annotationModel.getSnapshot())
  const unsubscribeAnnotations = annotationModel.subscribe((next) => {
    annotationSnapshot = next
  })
  let annotationTool = $state<AnnotationTool>('pen')
  let annotationColor = $state('#ff3b30')
  let annotationWidth = $state(5)
  let drawingPointerId = $state<number | null>(null)

  let teleprompter = $state<TeleprompterState>(createTeleprompterState())
  let teleprompterFrame: number | null = null

  let options = $state<CaptureOptions>({
    displaySurface: 'monitor',
    includeSystemAudio: true,
    includeMicrophone: false,
    microphoneDeviceId: undefined,
    includeWebcam: false,
    webcamDeviceId: undefined,
    countdownSeconds: 3,
  })

  const settingsDisabled = $derived(['requesting', 'countdown', 'recording', 'paused', 'stopping'].includes(snapshot.phase))
  const canStart = $derived(['idle', 'completed', 'error'].includes(snapshot.phase))
  const canAnnotate = $derived(['countdown', 'recording', 'paused'].includes(snapshot.phase) && Boolean(snapshot.previewCanvas))

  const phaseLabel = $derived.by(() => {
    switch (snapshot.phase) {
      case 'requesting': return 'Выберите источник в окне браузера'
      case 'countdown': return `Запись начнётся через ${snapshot.countdownSeconds}`
      case 'recording': return 'Идёт запись'
      case 'paused': return 'Запись приостановлена'
      case 'stopping': return 'Сохраняем запись…'
      case 'completed': return 'Запись готова'
      case 'error': return 'Запись не началась'
      default: return 'Готово к записи'
    }
  })

  function deviceLabel(device: MediaDeviceInfo, index: number, fallback: string): string {
    return device.label || `${fallback} ${index + 1}`
  }

  function setDisplaySurface(event: Event): void {
    options.displaySurface = (event.currentTarget as HTMLSelectElement).value as DisplaySourcePreference
  }

  function startCapture(): void {
    void controller?.start({
      ...options,
      microphoneDeviceId: options.microphoneDeviceId || undefined,
      webcamDeviceId: options.webcamDeviceId || undefined,
      countdownSeconds,
    })
  }

  function annotationPoint(event: PointerEvent): AnnotationPoint | null {
    const canvas = snapshot.previewCanvas
    if (!canvas) return null
    const bounds = canvas.getBoundingClientRect()
    if (bounds.width <= 0 || bounds.height <= 0) return null
    return {
      x: (event.clientX - bounds.left) / bounds.width,
      y: (event.clientY - bounds.top) / bounds.height,
    }
  }

  function beginAnnotation(event: PointerEvent): void {
    if (!canAnnotate || event.button !== 0) return
    const point = annotationPoint(event)
    if (!point) return
    event.preventDefault()
    drawingPointerId = event.pointerId
    previewHost?.setPointerCapture?.(event.pointerId)
    annotationModel.begin(annotationTool, point, { color: annotationColor, width: annotationWidth })
  }

  function moveAnnotation(event: PointerEvent): void {
    if (drawingPointerId !== event.pointerId) return
    const point = annotationPoint(event)
    if (point) annotationModel.append(point)
  }

  function endAnnotation(event: PointerEvent): void {
    if (drawingPointerId !== event.pointerId) return
    const point = annotationPoint(event)
    annotationModel.end(point ?? undefined)
    previewHost?.releasePointerCapture?.(event.pointerId)
    drawingPointerId = null
  }

  function cancelAnnotation(event: PointerEvent): void {
    if (drawingPointerId !== event.pointerId) return
    annotationModel.cancel()
    drawingPointerId = null
  }

  function teleprompterLimit(): number {
    return Math.max(0, (previewHost?.clientHeight ?? 0) + (teleprompterContent?.scrollHeight ?? 0))
  }

  function cancelTeleprompterFrame(): void {
    if (teleprompterFrame !== null) cancelAnimationFrame(teleprompterFrame)
    teleprompterFrame = null
  }

  function animateTeleprompter(nowMs: number): void {
    teleprompter = advanceTeleprompter(teleprompter, nowMs, teleprompterLimit())
    if (teleprompter.phase === 'running') teleprompterFrame = requestAnimationFrame(animateTeleprompter)
    else teleprompterFrame = null
  }

  function runTeleprompter(): void {
    teleprompter = startTeleprompter(teleprompter, performance.now())
    if (teleprompter.phase !== 'running') return
    cancelTeleprompterFrame()
    teleprompterFrame = requestAnimationFrame(animateTeleprompter)
  }

  function stopTeleprompter(): void {
    teleprompter = pauseTeleprompter(teleprompter, performance.now(), teleprompterLimit())
    cancelTeleprompterFrame()
  }

  function rewindTeleprompter(): void {
    cancelTeleprompterFrame()
    teleprompter = resetTeleprompter(teleprompter)
  }

  $effect(() => {
    const session = new CaptureController({
      runtime,
      onCapture: deliverCapture,
      onError: deliverError,
      annotations: annotationModel,
    })
    controller = session
    let previousPreviewCanvas: HTMLCanvasElement | null = null
    const unsubscribe = session.subscribe((next) => {
      if (previousPreviewCanvas && !next.previewCanvas) rewindTeleprompter()
      previousPreviewCanvas = next.previewCanvas
      snapshot = next
    })
    void session.loadDevices()

    return () => {
      unsubscribe()
      session.dispose()
      controller = null
    }
  })

  $effect(() => {
    const host = previewHost
    const canvas = snapshot.previewCanvas
    if (!host || !canvas) return
    canvas.className = 'capture-live-preview'
    canvas.style.width = '100%'
    canvas.style.height = 'auto'
    canvas.style.display = 'block'
    host.prepend(canvas)

    return () => {
      if (canvas.parentElement === host) canvas.remove()
    }
  })

  $effect(() => () => {
    unsubscribeAnnotations()
    cancelTeleprompterFrame()
  })
</script>

<section bind:this={panelElement} class={`capture-panel ${className}`.trim()} aria-label={title}>
  <header class="capture-header">
    <h2>{title}</h2>
    <div class:live={snapshot.phase === 'recording'} class:paused={snapshot.phase === 'paused'} class="capture-status" aria-live="polite">
      <span class="status-dot" aria-hidden="true"></span>
      <span>{phaseLabel}</span>
      {#if ['recording', 'paused', 'stopping'].includes(snapshot.phase)}
        <time datetime={`PT${snapshot.elapsedSeconds}S`}>{formatElapsed(snapshot.elapsedSeconds)}</time>
      {/if}
    </div>
  </header>

  <fieldset class="capture-settings" disabled={settingsDisabled}>
    <legend class="sr-only">Параметры записи</legend>

    <label class="capture-field">
      <span>Что записывать</span>
      <select value={options.displaySurface} onchange={setDisplaySurface}>
        <option value="monitor">Весь экран</option>
        <option value="window">Окно приложения</option>
        <option value="browser">Вкладку браузера</option>
      </select>
    </label>

    <div class="capture-toggles">
      <label class="capture-toggle">
        <input type="checkbox" bind:checked={options.includeSystemAudio} />
        <span><strong>Системный звук</strong></span>
      </label>

      <label class="capture-toggle">
        <input type="checkbox" bind:checked={options.includeMicrophone} />
        <span><strong>Микрофон</strong></span>
      </label>

      <label class="capture-toggle">
        <input type="checkbox" bind:checked={options.includeWebcam} />
        <span><strong>Камера · PiP</strong></span>
      </label>
    </div>

    {#if options.includeMicrophone || options.includeWebcam}
      <div class="capture-devices">
        {#if options.includeMicrophone}
          <label class="capture-field">
            <span>Микрофон</span>
            <select bind:value={options.microphoneDeviceId}>
              <option value={undefined}>Системный по умолчанию</option>
              {#each snapshot.devices.microphones as device, index (device.deviceId)}
                <option value={device.deviceId}>{deviceLabel(device, index, 'Микрофон')}</option>
              {/each}
            </select>
          </label>
        {/if}

        {#if options.includeWebcam}
          <label class="capture-field">
            <span>Камера</span>
            <select bind:value={options.webcamDeviceId}>
              <option value={undefined}>Системная по умолчанию</option>
              {#each snapshot.devices.cameras as device, index (device.deviceId)}
                <option value={device.deviceId}>{deviceLabel(device, index, 'Камера')}</option>
              {/each}
            </select>
          </label>
        {/if}

        <button class="capture-link" type="button" disabled={snapshot.devicesLoading} onclick={() => void controller?.loadDevices()}>
          {snapshot.devicesLoading ? 'Обновляем…' : 'Обновить устройства'}
        </button>
      </div>
    {/if}
  </fieldset>

  <details class="teleprompter-panel">
    <summary>Локальный телесуфлёр</summary>
    <div class="teleprompter-controls">
      <label class="capture-field teleprompter-script">
        <span>Текст</span>
        <textarea
          rows="4"
          placeholder="Вставьте текст, который будете читать…"
          value={teleprompter.text}
          oninput={(event) => {
            teleprompter = configureTeleprompter(teleprompter, { text: (event.currentTarget as HTMLTextAreaElement).value })
          }}
        ></textarea>
      </label>
      <label class="capture-range">
        <span>Скорость <strong>{teleprompter.speedPxPerSecond} px/с</strong></span>
        <input
          type="range"
          min="10"
          max="180"
          step="5"
          value={teleprompter.speedPxPerSecond}
          oninput={(event) => {
            teleprompter = configureTeleprompter(teleprompter, { speedPxPerSecond: Number((event.currentTarget as HTMLInputElement).value) })
          }}
        />
      </label>
      <label class="capture-range">
        <span>Шрифт <strong>{teleprompter.fontSizePx}px</strong></span>
        <input
          type="range"
          min="16"
          max="72"
          step="2"
          value={teleprompter.fontSizePx}
          oninput={(event) => {
            teleprompter = configureTeleprompter(teleprompter, { fontSizePx: Number((event.currentTarget as HTMLInputElement).value) })
          }}
        />
      </label>
      <div class="teleprompter-actions">
        {#if teleprompter.phase === 'running'}
          <button class="capture-button secondary" type="button" onclick={stopTeleprompter}>Пауза суфлёра</button>
        {:else}
          <button class="capture-button secondary" type="button" disabled={!snapshot.previewCanvas || !teleprompter.text.trim()} onclick={runTeleprompter}>
            {teleprompter.phase === 'paused' ? 'Продолжить суфлёр' : 'Запустить суфлёр'}
          </button>
        {/if}
        <button class="capture-link" type="button" disabled={teleprompter.offsetPx === 0 && teleprompter.phase === 'idle'} onclick={rewindTeleprompter}>В начало</button>
      </div>
      <p class="teleprompter-note">Суфлёр не рисуется в composed canvas.</p>
    </div>
  </details>

  {#if snapshot.deviceError}
    <p class="capture-note" role="status">Не удалось прочитать список устройств: {snapshot.deviceError}</p>
  {/if}

  {#if snapshot.previewCanvas}
    <div class="capture-preview-section">
      <div
        bind:this={previewHost}
        class:drawing={canAnnotate}
        class="capture-live-stage"
        role="application"
        tabindex="-1"
        aria-label="Live preview и аннотации"
        onpointerdown={beginAnnotation}
        onpointermove={moveAnnotation}
        onpointerup={endAnnotation}
        onpointercancel={cancelAnnotation}
      >
        {#if teleprompter.phase !== 'idle' && teleprompter.text.trim()}
          <div class="teleprompter-overlay" aria-hidden="true">
            <div
              bind:this={teleprompterContent}
              class="teleprompter-content"
              style:font-size={`${teleprompter.fontSizePx}px`}
              style:transform={`translateY(-${teleprompter.offsetPx}px)`}
            >{teleprompter.text}</div>
          </div>
        {/if}
        {#if snapshot.phase === 'countdown'}
          <div class="capture-countdown" role="status" aria-live="assertive">
            <strong>{snapshot.countdownSeconds}</strong>
            <span>Приготовьтесь</span>
          </div>
        {/if}
      </div>

      <div class="annotation-toolbar" role="toolbar" aria-label="Аннотации">
        {#each [
          { id: 'pen', label: 'Перо' },
          { id: 'highlighter', label: 'Маркер' },
          { id: 'rectangle', label: 'Прямоугольник' },
          { id: 'arrow', label: 'Стрелка' },
        ] as tool (tool.id)}
          <button
            class:active={annotationTool === tool.id}
            class="annotation-tool"
            type="button"
            aria-pressed={annotationTool === tool.id}
            onclick={() => { annotationTool = tool.id as AnnotationTool }}
          >{tool.label}</button>
        {/each}
        <label class="annotation-color" title="Цвет аннотации">
          <span class="sr-only">Цвет аннотации</span>
          <input type="color" bind:value={annotationColor} />
        </label>
        <label class="annotation-width">
          <span>Толщина {annotationWidth}px</span>
          <input type="range" min="1" max="24" step="1" bind:value={annotationWidth} />
        </label>
        <button class="capture-link" type="button" disabled={!annotationSnapshot.active && annotationSnapshot.strokes.length === 0} onclick={() => annotationModel.undo()}>Отменить</button>
        <button class="capture-link danger-link" type="button" disabled={!annotationSnapshot.active && annotationSnapshot.strokes.length === 0} onclick={() => annotationModel.clear()}>Очистить</button>
      </div>
    </div>
  {/if}

  {#if snapshot.hasSystemAudio === false}
    <p class="capture-note" role="status">Браузер не предоставил системный звук. Запись продолжится без него.</p>
  {/if}

  {#if snapshot.error}
    <p class="capture-error" role="alert">{snapshot.error}</p>
  {/if}

  <div class="capture-actions">
    {#if canStart}
      <button class="capture-button primary" type="button" onclick={startCapture}>
        {snapshot.phase === 'completed' ? 'Записать ещё' : 'Начать запись'}
      </button>
    {:else if snapshot.phase === 'requesting' || snapshot.phase === 'countdown'}
      <button class="capture-button danger" type="button" onclick={() => controller?.stop()}>Отмена</button>
    {:else if snapshot.phase === 'recording'}
      <button class="capture-button secondary" type="button" onclick={() => controller?.pause()}>Пауза</button>
      <button class="capture-button danger" type="button" onclick={() => controller?.stop()}>Завершить</button>
    {:else if snapshot.phase === 'paused'}
      <button class="capture-button primary" type="button" onclick={() => controller?.resume()}>Продолжить</button>
      <button class="capture-button danger" type="button" onclick={() => controller?.stop()}>Завершить</button>
    {:else}
      <button class="capture-button secondary" type="button" disabled>Сохраняем…</button>
    {/if}
    <span class="capture-privacy">Локально до загрузки.</span>
  </div>

  {#if snapshot.previewUrl && snapshot.file}
    <div class="capture-result">
      <video controls preload="metadata" src={snapshot.previewUrl} aria-label="Предпросмотр записи">
        <track kind="captions" />
      </video>
      <div>
        <strong>{snapshot.file.name}</strong>
        <span>{(snapshot.file.size / 1024 / 1024).toFixed(1)} МБ · WebM</span>
      </div>
    </div>
  {/if}
</section>

<style>
  .capture-panel {
    --capture-bg: color-mix(in srgb, Canvas 96%, #64748b 4%);
    --capture-border: color-mix(in srgb, CanvasText 16%, transparent);
    --capture-muted: color-mix(in srgb, CanvasText 62%, transparent);
    display: grid;
    gap: 1rem;
    padding: 1.1rem;
    color: CanvasText;
    background: var(--capture-bg);
    border: 1px solid var(--capture-border);
    border-radius: 1rem;
    box-shadow: 0 12px 30px color-mix(in srgb, CanvasText 8%, transparent);
  }

  .capture-header,
  .capture-actions,
  .capture-status,
  .capture-toggle,
  .capture-result {
    display: flex;
    align-items: center;
  }

  .capture-header {
    justify-content: space-between;
    gap: 1rem;
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    font-size: 1.12rem;
  }

  .capture-status {
    gap: 0.45rem;
    min-height: 2rem;
    padding: 0.4rem 0.65rem;
    color: var(--capture-muted);
    background: color-mix(in srgb, CanvasText 5%, transparent);
    border-radius: 999px;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .capture-status time {
    min-width: 3.2rem;
    color: CanvasText;
    font-variant-numeric: tabular-nums;
    font-weight: 750;
    text-align: right;
  }

  .status-dot {
    width: 0.55rem;
    height: 0.55rem;
    background: #94a3b8;
    border-radius: 50%;
  }

  .capture-status.live .status-dot {
    background: #ef4444;
    box-shadow: 0 0 0 0.22rem color-mix(in srgb, #ef4444 22%, transparent);
  }

  .capture-status.paused .status-dot {
    background: #f59e0b;
  }

  .capture-settings {
    display: grid;
    gap: 0.9rem;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 0;
  }

  .capture-settings:disabled {
    opacity: 0.66;
  }

  .capture-field {
    display: grid;
    gap: 0.35rem;
    min-width: 0;
    color: var(--capture-muted);
    font-size: 0.78rem;
  }

  .capture-field > span {
    color: CanvasText;
    font-weight: 700;
  }

  .capture-field select {
    width: 100%;
    min-height: 2.5rem;
    padding: 0 0.7rem;
    color: CanvasText;
    background: Canvas;
    border: 1px solid var(--capture-border);
    border-radius: 0.65rem;
  }

  .capture-toggles {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 0.6rem;
  }

  .capture-toggle {
    align-items: flex-start;
    gap: 0.55rem;
    padding: 0.7rem;
    background: color-mix(in srgb, CanvasText 4%, transparent);
    border: 1px solid var(--capture-border);
    border-radius: 0.75rem;
  }

  .capture-toggle input {
    margin-top: 0.15rem;
    accent-color: #6366f1;
  }

  .capture-toggle strong {
    display: block;
    margin-bottom: 0.15rem;
    font-size: 0.82rem;
  }

  .capture-devices {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.7rem;
    align-items: end;
  }

  .capture-link {
    justify-self: start;
    padding: 0;
    color: #6366f1;
    background: transparent;
    border: 0;
    cursor: pointer;
    font: inherit;
    font-size: 0.76rem;
    font-weight: 700;
  }

  .capture-link:disabled {
    cursor: default;
    opacity: 0.45;
  }

  .danger-link {
    color: #dc2626;
  }

  .teleprompter-panel {
    padding: 0.75rem;
    background: color-mix(in srgb, CanvasText 4%, transparent);
    border: 1px solid var(--capture-border);
    border-radius: 0.75rem;
  }

  .teleprompter-panel summary {
    cursor: pointer;
    font-size: 0.82rem;
    font-weight: 750;
  }

  .teleprompter-controls {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(10rem, 0.45fr) minmax(10rem, 0.45fr);
    gap: 0.75rem;
    align-items: end;
    padding-top: 0.75rem;
  }

  .teleprompter-script {
    grid-row: span 2;
  }

  .teleprompter-script textarea {
    box-sizing: border-box;
    width: 100%;
    padding: 0.65rem;
    resize: vertical;
    color: CanvasText;
    background: Canvas;
    border: 1px solid var(--capture-border);
    border-radius: 0.65rem;
    font: inherit;
    line-height: 1.45;
  }

  .capture-range {
    display: grid;
    gap: 0.3rem;
    color: var(--capture-muted);
    font-size: 0.72rem;
  }

  .capture-range span {
    display: flex;
    justify-content: space-between;
    gap: 0.5rem;
  }

  .capture-range input,
  .annotation-width input {
    accent-color: #6366f1;
  }

  .teleprompter-actions {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }

  .teleprompter-note {
    grid-column: 2 / -1;
    color: var(--capture-muted);
    font-size: 0.68rem;
    line-height: 1.4;
  }

  .capture-preview-section {
    display: grid;
    gap: 0.65rem;
  }

  .capture-live-stage {
    position: relative;
    min-height: 8rem;
    overflow: hidden;
    touch-action: none;
    background: #0f172a;
    border: 1px solid var(--capture-border);
    border-radius: 0.85rem;
    outline: none;
  }

  .capture-live-stage.drawing {
    cursor: crosshair;
  }

  .capture-live-stage:focus-visible {
    box-shadow: 0 0 0 0.2rem color-mix(in srgb, #6366f1 35%, transparent);
  }

  .capture-live-stage :global(.capture-live-preview) {
    position: relative;
    z-index: 1;
  }

  .teleprompter-overlay {
    position: absolute;
    z-index: 3;
    inset: 0;
    overflow: hidden;
    pointer-events: none;
    background: linear-gradient(to bottom, rgba(0, 0, 0, 0.34), transparent 25%, transparent 75%, rgba(0, 0, 0, 0.34));
  }

  .teleprompter-content {
    position: absolute;
    top: 100%;
    right: 8%;
    left: 8%;
    padding: 1em 0 45%;
    color: white;
    font-weight: 700;
    line-height: 1.45;
    text-align: center;
    text-shadow: 0 2px 8px black;
    white-space: pre-wrap;
  }

  .annotation-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.45rem;
  }

  .annotation-tool {
    min-height: 2rem;
    padding: 0 0.65rem;
    color: CanvasText;
    background: color-mix(in srgb, CanvasText 6%, Canvas);
    border: 1px solid var(--capture-border);
    border-radius: 0.55rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.72rem;
    font-weight: 700;
  }

  .annotation-tool.active {
    color: white;
    background: #4f46e5;
    border-color: #4f46e5;
  }

  .annotation-color input {
    width: 2.1rem;
    height: 2rem;
    padding: 0.12rem;
    background: Canvas;
    border: 1px solid var(--capture-border);
    border-radius: 0.5rem;
  }

  .annotation-width {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    color: var(--capture-muted);
    font-size: 0.68rem;
  }

  .annotation-width input {
    width: 7rem;
  }

  .capture-countdown {
    position: absolute;
    z-index: 4;
    inset: 0;
    display: grid;
    place-items: center;
    color: white;
    pointer-events: none;
    background: radial-gradient(circle at 50% 35%, rgba(129, 140, 248, 0.9), rgba(67, 56, 202, 0.78));
  }

  .capture-countdown strong {
    font-size: 3.5rem;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .capture-countdown span {
    margin-top: -1.4rem;
    font-size: 0.78rem;
  }

  .capture-note,
  .capture-error {
    padding: 0.7rem 0.8rem;
    border-radius: 0.65rem;
    font-size: 0.78rem;
    line-height: 1.4;
  }

  .capture-note {
    color: #92400e;
    background: color-mix(in srgb, #f59e0b 14%, Canvas);
  }

  .capture-error {
    color: #b91c1c;
    background: color-mix(in srgb, #ef4444 12%, Canvas);
  }

  .capture-actions {
    flex-wrap: wrap;
    gap: 0.55rem;
  }

  .capture-button {
    min-height: 2.45rem;
    padding: 0 0.9rem;
    color: CanvasText;
    background: color-mix(in srgb, CanvasText 8%, Canvas);
    border: 1px solid var(--capture-border);
    border-radius: 0.65rem;
    cursor: pointer;
    font: inherit;
    font-size: 0.8rem;
    font-weight: 750;
  }

  .capture-button.primary {
    color: white;
    background: #4f46e5;
    border-color: #4f46e5;
  }

  .capture-button.danger {
    color: white;
    background: #dc2626;
    border-color: #dc2626;
  }

  .capture-button:disabled {
    cursor: wait;
    opacity: 0.65;
  }

  .capture-privacy {
    margin-left: auto;
    color: var(--capture-muted);
    font-size: 0.7rem;
  }

  .capture-result {
    align-items: flex-start;
    gap: 0.8rem;
    padding-top: 1rem;
    border-top: 1px solid var(--capture-border);
  }

  .capture-result video {
    width: min(13rem, 42%);
    aspect-ratio: 16 / 9;
    background: #0f172a;
    border-radius: 0.6rem;
  }

  .capture-result div {
    display: grid;
    gap: 0.3rem;
    min-width: 0;
  }

  .capture-result strong {
    overflow: hidden;
    font-size: 0.82rem;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .capture-result span {
    color: var(--capture-muted);
    font-size: 0.72rem;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  @media (max-width: 680px) {
    .capture-header {
      align-items: flex-start;
      flex-direction: column;
    }

    .capture-toggles,
    .capture-devices,
    .teleprompter-controls {
      grid-template-columns: 1fr;
    }

    .teleprompter-script,
    .teleprompter-note {
      grid-column: 1;
      grid-row: auto;
    }

    .capture-privacy {
      width: 100%;
      margin-left: 0;
    }
  }
</style>
