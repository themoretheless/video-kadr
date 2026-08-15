<script lang="ts">
  import {
    CaptureController,
    formatElapsed,
    INITIAL_CAPTURE_SNAPSHOT,
    type RecordingOutputProfile,
  } from '$lib/capture/mediaRecorder.js'
  import type { CaptureOptions, CaptureRuntime, CaptureSnapshot } from '$lib/capture/types.js'
  import { DEFAULT_VOICEOVER_DSP } from './dsp.js'
  import {
    browserProcessingStillEnabled,
    buildMicrophoneConstraints,
    createVoiceoverRuntime,
    formatVoiceoverError,
  } from './voiceRecorder.js'
  import type {
    AudioGraphPort,
    VoiceoverCompleteHandler,
    VoiceoverErrorHandler,
    VoiceoverOptions,
    VoiceoverRuntime,
  } from './types.js'

  interface Props {
    oncapture?: VoiceoverCompleteHandler
    onvoiceovererror?: VoiceoverErrorHandler
    runtime?: VoiceoverRuntime
    countdownSeconds?: number
    title?: string
    class?: string
    busy?: boolean
    blocked?: boolean
  }

  let {
    oncapture,
    onvoiceovererror,
    runtime,
    countdownSeconds = 3,
    title = 'Запись голоса',
    class: className = '',
    busy = false,
    blocked = false,
  }: Props = $props()

  const AUDIO_OUTPUT: RecordingOutputProfile = {
    mimeTypes: [
      'audio/webm;codecs=opus',
      'audio/webm',
      'audio/ogg;codecs=opus',
      'audio/mp4;codecs=mp4a.40.2',
      'audio/mp4',
    ],
    fallbackMimeType: 'audio/webm',
    filenamePrefix: 'voiceover',
    fallbackExtension: 'webm',
    extensions: { 'audio/ogg': 'ogg', 'audio/mp4': 'm4a' },
  }
  const CAPTURE_OPTIONS: CaptureOptions = {
    displaySurface: 'monitor',
    includeSystemAudio: false,
    includeMicrophone: false,
    includeWebcam: false,
    countdownSeconds: 0,
  }

  let panelElement: HTMLElement
  let controller = $state.raw<CaptureController | null>(null)
  let sessionRuntime = $state.raw<VoiceoverRuntime | null>(null)
  let graph = $state.raw<AudioGraphPort | null>(null)
  let inputTrack = $state.raw<MediaStreamTrack | null>(null)
  let level = $state({ rms: 0, peak: 0 })
  let permission = $state<'prompt' | 'granted' | 'denied' | 'unsupported'>('prompt')
  let snapshot = $state<CaptureSnapshot>(INITIAL_CAPTURE_SNAPSHOT)
  let options = $state<VoiceoverOptions>({
    microphoneDeviceId: undefined,
    countdownSeconds: 3,
    dsp: { ...DEFAULT_VOICEOVER_DSP },
  })

  const startablePhase = $derived(['idle', 'completed', 'error'].includes(snapshot.phase))
  const settingsDisabled = $derived(busy || blocked || ['requesting', 'countdown', 'recording', 'paused', 'stopping'].includes(snapshot.phase))
  const canStart = $derived(!busy && !blocked && startablePhase)
  const showMeter = $derived(['countdown', 'recording', 'paused'].includes(snapshot.phase))
  const rmsDb = $derived(level.rms > 0 ? Math.max(-60, 20 * Math.log10(level.rms)) : -60)

  const phaseLabel = $derived.by(() => {
    switch (snapshot.phase) {
      case 'requesting': return 'Разрешите доступ к микрофону'
      case 'countdown': return `Запись начнётся через ${snapshot.countdownSeconds}`
      case 'recording': return 'Идёт запись голоса'
      case 'paused': return 'Запись приостановлена'
      case 'stopping': return 'Сохраняем…'
      case 'completed': return 'Запись готова'
      case 'error': return 'Ошибка записи'
      default: return 'Готово'
    }
  })

  const permissionLabel = $derived.by(() => {
    switch (permission) {
      case 'granted': return 'Микрофон доступен.'
      case 'denied': return 'Доступ запрещён. Разрешите микрофон в браузере.'
      case 'unsupported': return 'Нет MediaRecorder или Web Audio.'
      default: return 'При старте браузер запросит микрофон.'
    }
  })

  function deviceLabel(device: MediaDeviceInfo, index: number): string {
    return device.label || `Микрофон ${index + 1}`
  }

  function deliverCapture(file: File): void | Promise<void> {
    panelElement?.dispatchEvent(new CustomEvent<File>('voiceover', { detail: file, bubbles: true }))
    panelElement?.dispatchEvent(new CustomEvent<File>('capture', { detail: file, bubbles: true }))
    return oncapture?.(file)
  }

  function deliverError(message: string): void {
    panelElement?.dispatchEvent(new CustomEvent<string>('voiceovererror', { detail: message, bubbles: true }))
    onvoiceovererror?.(message)
  }

  function startRecording(): void {
    if (busy || blocked) return
    if (!sessionRuntime?.isSupported()) permission = 'unsupported'
    void controller?.start({
      ...CAPTURE_OPTIONS,
      countdownSeconds,
    })
  }

  $effect(() => {
    const voiceRuntime = runtime ?? createVoiceoverRuntime()
    sessionRuntime = voiceRuntime
    const inputEnded = (): void => controller?.stop()
    const sourceAdapter = {
      acquire: async () => {
        let input: MediaStream | null = null
        try {
          input = await voiceRuntime.getUserMedia(buildMicrophoneConstraints(options.microphoneDeviceId || undefined))
          const track = input.getAudioTracks()[0]
          if (!track) throw new Error('Микрофон не предоставил аудиодорожку.')
          const browserProcessing = browserProcessingStillEnabled(track.getSettings?.() ?? {})
          if (browserProcessing.length) {
            throw new Error(`Браузер не позволил отключить встроенную обработку: ${browserProcessing.join(', ')}.`)
          }
          permission = 'granted'
          return input
        } catch (error) {
          input?.getTracks().forEach((track) => track.stop())
          if (error instanceof Error && ['NotAllowedError', 'PermissionDeniedError', 'SecurityError'].includes(error.name)) {
            permission = 'denied'
          }
          throw error
        }
      },
      prepare: async (display: MediaStream) => {
        const sessionGraph = await voiceRuntime.createGraph(display, { ...options.dsp })
        graph = sessionGraph
        level = sessionGraph.readLevel()
        inputTrack = display.getAudioTracks()[0] ?? null
        inputTrack?.addEventListener('ended', inputEnded)
        const previewCanvas = document.createElement('canvas')
        return {
          stream: sessionGraph.output,
          previewCanvas,
          cleanup() {
            inputTrack?.removeEventListener('ended', inputEnded)
            inputTrack = null
            if (graph === sessionGraph) graph = null
            level = { rms: 0, peak: 0 }
            return sessionGraph.cleanup()
          },
        }
      },
    }
    const session = new CaptureController({
      runtime: voiceRuntime as unknown as CaptureRuntime,
      sourceAdapter,
      onCapture: deliverCapture,
      onError: deliverError,
      output: AUDIO_OUTPUT,
      formatError: formatVoiceoverError,
      unsupportedMessage: 'Этот браузер не поддерживает безопасную локальную запись с Web Audio и MediaRecorder.',
    })
    controller = session
    const unsubscribe = session.subscribe((next) => { snapshot = next })
    void session.loadDevices()

    return () => {
      unsubscribe()
      session.dispose()
      controller = null
      sessionRuntime = null
      graph = null
      inputTrack = null
    }
  })

  $effect(() => {
    const activeGraph = graph
    const activeRuntime = sessionRuntime
    if (!showMeter || !activeGraph || !activeRuntime) return
    const update = (): void => { level = activeGraph.readLevel() }
    update()
    const timer = activeRuntime.setInterval(update, 50)
    return () => activeRuntime.clearInterval(timer)
  })
</script>

<section bind:this={panelElement} class={`voiceover-panel ${className}`.trim()} aria-label={title}>
  <header class="voiceover-header">
    <h2>{title}</h2>
    <div class:live={snapshot.phase === 'recording'} class:paused={snapshot.phase === 'paused'} class="voiceover-status" aria-live="polite">
      <span class="status-dot" aria-hidden="true"></span>
      <span>{phaseLabel}</span>
      {#if ['recording', 'paused', 'stopping'].includes(snapshot.phase)}
        <time datetime={`PT${snapshot.elapsedSeconds}S`}>{formatElapsed(snapshot.elapsedSeconds)}</time>
      {/if}
    </div>
  </header>

  <fieldset class="voiceover-settings" disabled={settingsDisabled}>
    <legend class="sr-only">Параметры голосовой записи</legend>
    <div class="microphone-row">
      <label class="voiceover-field">
        <span>Микрофон</span>
        <select
          value={options.microphoneDeviceId ?? ''}
          onchange={(event) => {
            options.microphoneDeviceId = (event.currentTarget as HTMLSelectElement).value || undefined
          }}
        >
          <option value="">Системный по умолчанию</option>
          {#each snapshot.devices.microphones as device, index (device.deviceId)}
            <option value={device.deviceId}>{deviceLabel(device, index)}</option>
          {/each}
        </select>
      </label>
      <button class="voiceover-link" type="button" disabled={snapshot.devicesLoading} onclick={() => void controller?.loadDevices()}>
        {snapshot.devicesLoading ? 'Обновляем…' : 'Обновить устройства'}
      </button>
    </div>

    <p class:denied={permission === 'denied'} class="permission-note">{permissionLabel}</p>

    <div class="dsp-panel">
      <div class="dsp-heading">
        <strong>Локальная обработка записи</strong>
        <span>Локально · без AI</span>
      </div>

      <label class="voiceover-range">
        <span>Входной gain <strong>{options.dsp.inputGainDb > 0 ? '+' : ''}{options.dsp.inputGainDb} dB</strong></span>
        <input type="range" min="-24" max="18" step="1" bind:value={options.dsp.inputGainDb} />
      </label>

      <label class="dsp-toggle">
        <input type="checkbox" bind:checked={options.dsp.highPassEnabled} />
        <span><strong>High-pass</strong></span>
      </label>
      {#if options.dsp.highPassEnabled}
        <label class="voiceover-range compact">
          <span>Частота <strong>{options.dsp.highPassHz} Hz</strong></span>
          <input type="range" min="40" max="240" step="10" bind:value={options.dsp.highPassHz} />
        </label>
      {/if}

      <label class="dsp-toggle">
        <input type="checkbox" bind:checked={options.dsp.compressorEnabled} />
        <span><strong>Компрессор</strong></span>
      </label>
      <label class="dsp-toggle">
        <input type="checkbox" bind:checked={options.dsp.limiterEnabled} />
        <span><strong>Лимитер −1 dB</strong></span>
      </label>
    </div>
  </fieldset>

  {#if snapshot.deviceError}
    <p class="voiceover-note" role="status">Не удалось прочитать устройства: {snapshot.deviceError}</p>
  {/if}

  {#if showMeter}
    <div class="level-panel" aria-label="Уровень микрофона">
      <div class="level-heading"><span>Входной уровень после обработки</span><strong>{rmsDb.toFixed(1)} dBFS</strong></div>
      <meter min="0" max="1" low="0.08" high="0.8" optimum="0.55" value={level.peak}>{level.peak}</meter>
    </div>
  {/if}

  {#if snapshot.phase === 'countdown'}
    <div class="voiceover-countdown" role="status" aria-live="assertive">
      <strong>{snapshot.countdownSeconds}</strong><span>Приготовьтесь говорить</span>
    </div>
  {/if}

  {#if snapshot.error}
    <p class="voiceover-error" role="alert">{snapshot.error}</p>
  {/if}

  <div class="voiceover-actions">
    {#if busy}
      <button class="voiceover-button secondary" type="button" disabled>Сохраняем…</button>
    {:else if blocked && startablePhase}
      <button class="voiceover-button secondary" type="button" disabled>Сначала сохраните текущую запись</button>
    {:else if canStart}
      <button class="voiceover-button primary" type="button" onclick={startRecording}>
        {snapshot.phase === 'completed' ? 'Записать ещё' : 'Начать запись'}
      </button>
    {:else if snapshot.phase === 'requesting' || snapshot.phase === 'countdown'}
      <button class="voiceover-button danger" type="button" onclick={() => controller?.stop()}>Отмена</button>
    {:else if snapshot.phase === 'recording'}
      <button class="voiceover-button secondary" type="button" onclick={() => controller?.pause()}>Пауза</button>
      <button class="voiceover-button danger" type="button" onclick={() => controller?.stop()}>Завершить</button>
    {:else if snapshot.phase === 'paused'}
      <button class="voiceover-button primary" type="button" onclick={() => controller?.resume()}>Продолжить</button>
      <button class="voiceover-button danger" type="button" onclick={() => controller?.stop()}>Завершить</button>
    {:else}
      <button class="voiceover-button secondary" type="button" disabled>Сохраняем…</button>
    {/if}
  </div>

  {#if snapshot.previewUrl && snapshot.file}
    <div class="voiceover-result">
      <audio controls preload="metadata" src={snapshot.previewUrl} aria-label="Предпросмотр голосовой записи"></audio>
      <div><strong>{snapshot.file.name}</strong><span>{(snapshot.file.size / 1024).toFixed(1)} КБ · {snapshot.file.type}</span></div>
    </div>
  {/if}
</section>

<style>
  .voiceover-panel {
    --border: color-mix(in srgb, CanvasText 16%, transparent);
    --muted: color-mix(in srgb, CanvasText 62%, transparent);
    display: grid;
    gap: 1rem;
    padding: 1.1rem;
    color: CanvasText;
    background: color-mix(in srgb, Canvas 96%, #64748b 4%);
    border: 1px solid var(--border);
    border-radius: 1rem;
  }

  .voiceover-header,
  .voiceover-status,
  .microphone-row,
  .voiceover-actions,
  .voiceover-result,
  .level-heading,
  .dsp-toggle {
    display: flex;
    align-items: center;
  }

  .voiceover-header { justify-content: space-between; gap: 1rem; }
  h2, p { margin: 0; }
  h2 { font-size: 1.12rem; }
  .voiceover-status { gap: 0.45rem; padding: 0.4rem 0.65rem; color: var(--muted); background: color-mix(in srgb, CanvasText 5%, transparent); border-radius: 999px; font-size: 0.78rem; white-space: nowrap; }
  .voiceover-status time { min-width: 3.2rem; color: CanvasText; font-variant-numeric: tabular-nums; font-weight: 750; text-align: right; }
  .status-dot { width: 0.55rem; height: 0.55rem; background: #94a3b8; border-radius: 50%; }
  .voiceover-status.live .status-dot { background: #ef4444; box-shadow: 0 0 0 0.22rem color-mix(in srgb, #ef4444 22%, transparent); }
  .voiceover-status.paused .status-dot { background: #f59e0b; }

  .voiceover-settings { display: grid; gap: 0.8rem; min-width: 0; margin: 0; padding: 0; border: 0; }
  .voiceover-settings:disabled { opacity: 0.66; }
  .microphone-row { align-items: end; gap: 0.7rem; }
  .voiceover-field { display: grid; flex: 1; gap: 0.35rem; color: var(--muted); font-size: 0.78rem; }
  .voiceover-field > span { color: CanvasText; font-weight: 700; }
  .voiceover-field select { min-height: 2.5rem; padding: 0 0.7rem; color: CanvasText; background: Canvas; border: 1px solid var(--border); border-radius: 0.65rem; }
  .voiceover-link { padding: 0 0 0.65rem; color: #6366f1; background: transparent; border: 0; cursor: pointer; font: inherit; font-size: 0.74rem; font-weight: 700; }
  .voiceover-link:disabled { cursor: default; opacity: 0.5; }
  .permission-note { color: var(--muted); font-size: 0.72rem; }
  .permission-note.denied { color: #b91c1c; }

  .dsp-panel { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 0.65rem; padding: 0.8rem; background: color-mix(in srgb, CanvasText 4%, transparent); border: 1px solid var(--border); border-radius: 0.75rem; }
  .dsp-heading { grid-column: 1 / -1; display: grid; gap: 0.15rem; }
  .dsp-heading strong { font-size: 0.8rem; }
  .dsp-heading span { color: var(--muted); font-size: 0.68rem; }
  .voiceover-range { display: grid; gap: 0.3rem; color: var(--muted); font-size: 0.7rem; }
  .voiceover-range > span { display: flex; justify-content: space-between; gap: 0.5rem; }
  .voiceover-range input { width: 100%; accent-color: #6366f1; }
  .dsp-toggle { align-items: flex-start; gap: 0.45rem; padding: 0.55rem; border: 1px solid var(--border); border-radius: 0.6rem; }
  .dsp-toggle input { margin-top: 0.15rem; accent-color: #6366f1; }
  .dsp-toggle strong { display: block; font-size: 0.75rem; }

  .level-panel { display: grid; gap: 0.35rem; }
  .level-heading { justify-content: space-between; color: var(--muted); font-size: 0.7rem; }
  .level-heading strong { color: CanvasText; font-variant-numeric: tabular-nums; }
  meter { width: 100%; height: 0.85rem; accent-color: #22c55e; }

  .voiceover-countdown { display: grid; place-items: center; min-height: 7rem; color: white; background: radial-gradient(circle at 50% 35%, #818cf8, #4338ca); border-radius: 0.8rem; }
  .voiceover-countdown strong { font-size: 3.2rem; line-height: 1; }
  .voiceover-countdown span { margin-top: -1.2rem; font-size: 0.76rem; }
  .voiceover-note, .voiceover-error { padding: 0.7rem 0.8rem; border-radius: 0.65rem; font-size: 0.76rem; }
  .voiceover-note { color: #92400e; background: color-mix(in srgb, #f59e0b 14%, Canvas); }
  .voiceover-error { color: #b91c1c; background: color-mix(in srgb, #ef4444 12%, Canvas); }

  .voiceover-actions { flex-wrap: wrap; gap: 0.55rem; }
  .voiceover-button { min-height: 2.45rem; padding: 0 0.9rem; color: CanvasText; background: color-mix(in srgb, CanvasText 8%, Canvas); border: 1px solid var(--border); border-radius: 0.65rem; cursor: pointer; font: inherit; font-size: 0.8rem; font-weight: 750; }
  .voiceover-button.primary { color: white; background: #4f46e5; border-color: #4f46e5; }
  .voiceover-button.danger { color: white; background: #dc2626; border-color: #dc2626; }
  .voiceover-button:disabled { cursor: wait; opacity: 0.65; }

  .voiceover-result { align-items: flex-start; gap: 0.8rem; padding-top: 0.9rem; border-top: 1px solid var(--border); }
  .voiceover-result audio { width: min(24rem, 62%); }
  .voiceover-result div { display: grid; gap: 0.25rem; min-width: 0; }
  .voiceover-result strong { overflow: hidden; font-size: 0.78rem; text-overflow: ellipsis; white-space: nowrap; }
  .voiceover-result span { color: var(--muted); font-size: 0.68rem; }

  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }

  @media (max-width: 680px) {
    .voiceover-header { align-items: flex-start; flex-direction: column; }
    .dsp-panel { grid-template-columns: 1fr; }
    .dsp-heading { grid-column: 1; }
    .voiceover-result { flex-direction: column; }
    .voiceover-result audio { width: 100%; }
  }
</style>
